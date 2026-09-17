/**
 * Plan presentation and browser review actions.
 * Also owns browser-driven plan review (openPlanReview, refreshPlanUi).
 */

import path from 'node:path';
import type { PiContext } from '../../types.js';
import type { PlanScope } from './plan-scope.js';
import { adoptPlanModePolicy, exitPlanMode } from '../plan-mode.js';
import {
  consumeHumanAuthorizationReceipt,
  createHumanAuthorizationReceipt,
  createHumanAuthorizationReceiptFromInteraction,
} from '../interaction-broker.js';
import {
  enablePlanHtmlSync,
  resetPlanHtmlSync,
  openPlanHtml,
  syncCurrentPlanHtmlIfEnabled,
  writePlanReadModelArtifacts,
  planArtifactsDir,
} from '../plan-html.js';
import { serveDirectory, unmount } from '../local-server.js';
import { setManagedActivity } from '../runtime-renderer.js';
import {
  activePlanScope,
  getPlan,
  getPlanCoordination,
  getPlanReviewState,
  addPlanDecision,
} from './plan-store.js';
import { stepLabel } from './plan-types.js';
import type { PlanStep } from './plan-types.js';
import {
  acceptPlanReview,
  requestPlanChanges,
  rollbackAcceptedPlanStart,
  startAcceptedPlan,
} from './plan-lifecycle.js';
import {
  ensureUnifiedProjection,
  planWorkspace,
  sharedStartContractError,
  writeCurrentPlanArtifacts,
} from './plan-presentation.js';
import { getCurrentPlanReadModel } from '../plan-read-model.js';

// ─── UI refresh state ────────────────────────────────────────────────────────────

let planMetricsRefresh: ((ctx?: PiContext) => void) | undefined;

export function setPlanMetricsRefreshForUi(refresh: ((ctx?: PiContext) => void) | undefined): void {
  planMetricsRefresh = refresh;
}

export function publishPlanActivity(ctx: PiContext | undefined, scope: PlanScope, steps: PlanStep[]): void {
  const review = getPlanReviewState(scope);
  switch (review.phase) {
    case 'researching':
      setManagedActivity(ctx, { kind: 'researching', planScope: scope });
      return;
    case 'needs_answers':
      setManagedActivity(ctx, { kind: 'awaiting_input', planScope: scope, question: 'Planning input required' });
      return;
    case 'draft':
      setManagedActivity(ctx, { kind: 'planning', planScope: scope });
      return;
    case 'in_review':
      setManagedActivity(ctx, { kind: 'reviewing', planScope: scope, revision: review.revision });
      return;
    case 'accepted':
      if (review.acceptedRevision) setManagedActivity(ctx, { kind: 'awaiting_start', planScope: scope, revision: review.acceptedRevision });
      return;
    case 'executing': {
      const active = steps.find((step) => step.status === 'doing');
      if (active) {
        setManagedActivity(ctx, { kind: 'working', planScope: scope, stepId: active.id, label: stepLabel(active) });
        return;
      }
      const runnable = steps.find((step) => step.status === 'todo');
      if (runnable) setManagedActivity(ctx, { kind: 'working', planScope: scope, stepId: runnable.id, label: runnable.text });
      return;
    }
    case 'verifying':
      setManagedActivity(ctx, { kind: 'verifying', planScope: scope });
      return;
    case 'complete':
      setManagedActivity(ctx, { kind: 'complete', label: 'Plan complete' });
      return;
    case 'blocked':
      setManagedActivity(ctx, { kind: 'blocked', label: 'Plan blocked' });
      return;
    case 'failed':
      setManagedActivity(ctx, { kind: 'failed', label: 'Plan failed' });
      return;
    case 'abandoned':
      setManagedActivity(ctx, { kind: 'idle' });
      return;
    default:
      setManagedActivity(ctx, { kind: 'idle' });
  }
}

/** Repaint the unified footer and keep any open HTML plan view synchronized. */
export function refreshPlanUi(ctx?: PiContext): void {
  const scope = activePlanScope(ctx);
  const steps = getPlan(scope);
  publishPlanActivity(ctx, scope, steps);
  syncCurrentPlanHtmlIfEnabled(ctx, scope);
  if (steps.length > 0) adoptPlanModePolicy(ctx, getPlanReviewState(scope));
  else exitPlanMode(ctx);
  planMetricsRefresh?.(ctx);
}

// ─── Browser-driven plan review ───────────────────────────────────────────────────

let planBrowserMessageSender: ((message: string) => void | Promise<void>) | undefined;
let planDirectoryServer: typeof serveDirectory = serveDirectory;

/** Test seam for browser-first review without binding the process-wide localhost server. */
export function setPlanDirectoryServerForTests(next?: typeof serveDirectory): void {
  planDirectoryServer = next ?? serveDirectory;
}

export function setPlanBrowserMessageSender(sender: ((message: string) => void | Promise<void>) | undefined): void {
  planBrowserMessageSender = sender;
}

function planMountName(scope: PlanScope): string {
  return `plan-${path.basename(planArtifactsDir(scope))}`;
}

export function tearDownPlanHtml(scope: PlanScope): void {
  resetPlanHtmlSync();
  unmount(planMountName(scope));
}

// ─── ReviewedPlanStart types ────────────────────────────────────────────────────────

interface ReviewedPlanStartResult {
  ok: boolean;
  message: string;
  steps: PlanStep[];
  revision?: string;
}

export interface ReviewedPlanAuthorization {
  interactionId: string;
  expectedOptionId: string;
}

function planStartAuthorizationOptionId(planId: string, revision: string): string {
  return `plan-start:${planId}:${revision}`;
}

export function buildPlanStartAuthorizationOptionId(planId: string, revision: string): string {
  return planStartAuthorizationOptionId(planId, revision);
}

/** Bind one explicit Start decision to the current RFC bytes and begin execution. */
export function startReviewedPlan(
  scope: PlanScope,
  displayedRevision: string,
  ctx?: PiContext,
  authorization?: ReviewedPlanAuthorization,
): ReviewedPlanStartResult {
  const steps = getPlan(scope);
  const contractError = getPlanCoordination(scope).mode === 'required'
    ? sharedStartContractError(steps)
    : undefined;
  if (contractError) return { ok: false, message: `invalid shared step contract — ${contractError}`, steps };

  let state = getPlanReviewState(scope);
  const expectedRevision = state.phase === 'accepted' ? state.acceptedRevision : state.revision;
  if (!expectedRevision || displayedRevision !== expectedRevision) {
    return { ok: false, message: `displayed revision is stale (expected ${expectedRevision?.slice(0, 8) ?? 'none'})`, steps };
  }
  const planId = getPlanCoordination(scope).sourcePlanKey;
  const createReceipt = (revision: string, receiptScope: 'plan.accept' | 'plan.start') => {
    if (!authorization) {
      return createHumanAuthorizationReceipt(ctx, {
        planId,
        revision,
        scope: receiptScope,
        question: `Start implementation of RFC revision ${revision}?`,
      });
    }
    if (!ctx) throw new Error('authorization interaction requires a host context');
    return createHumanAuthorizationReceiptFromInteraction(ctx, {
      interactionId: authorization.interactionId,
      planId,
      revision,
      scope: receiptScope,
      expectedOptionId: authorization.expectedOptionId,
      consumeInteraction: receiptScope === 'plan.start',
    });
  };
  if (state.phase === 'in_review') {
    let acceptReceipt;
    try {
      acceptReceipt = createReceipt(displayedRevision, 'plan.accept');
    } catch (error) {
      return { ok: false, message: error instanceof Error ? error.message : String(error), steps };
    }
    const accepted = acceptPlanReview(scope, displayedRevision, acceptReceipt.receiptId);
    if (!accepted.ok) return { ok: false, message: accepted.message, steps: accepted.steps };
    consumeHumanAuthorizationReceipt(acceptReceipt.workspace, {
      receiptId: acceptReceipt.receiptId,
      planId,
      revision: displayedRevision,
      scope: 'plan.accept',
    });
    state = accepted.state;
  }
  if (state.phase !== 'accepted' || !state.acceptedRevision) {
    return { ok: false, message: `Start is not valid from ${state.phase}`, steps: getPlan(scope) };
  }

  let startReceipt;
  try {
    startReceipt = createReceipt(state.acceptedRevision, 'plan.start');
  } catch (error) {
    return { ok: false, message: error instanceof Error ? error.message : String(error), steps: getPlan(scope) };
  }
  consumeHumanAuthorizationReceipt(startReceipt.workspace, {
    receiptId: startReceipt.receiptId,
    planId,
    revision: state.acceptedRevision,
    scope: 'plan.start',
  });
  const started = startAcceptedPlan(scope, startReceipt.receiptId);
  if (!started.ok) return { ok: false, message: started.message, steps: started.steps };
  try {
    ensureUnifiedProjection(scope, undefined, ctx);
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    rollbackAcceptedPlanStart(scope, `Shared Start failed: ${reason}`);
    return { ok: false, message: `RFC acceptance was preserved: ${reason}`, steps: getPlan(scope) };
  }
  return {
    ok: true,
    message: `Implementation started from revision ${started.state.acceptedRevision?.slice(0, 8) ?? 'unknown'}.`,
    steps: getPlan(scope),
    revision: started.state.acceptedRevision,
  };
}

/** Open the current plan from the configuration page's explicit user action. */
export async function openPlanReview(ctx?: PiContext): Promise<string | undefined> {
  return servePlanPage(ctx, activePlanScope(ctx));
}

async function servePlanPage(ctx: PiContext | undefined, scope: PlanScope): Promise<string | undefined> {
  const model = getCurrentPlanReadModel(ctx, scope);
  const phase = model.phase;
  const artifactStatus = phase === 'accepted'
    ? 'approved'
    : phase === 'executing' || phase === 'verifying' || phase === 'complete'
      ? 'active'
      : 'draft';
  const artifacts = writePlanReadModelArtifacts(scope, model, { status: artifactStatus, workspace: planWorkspace(scope) });
  if (!artifacts) return undefined;
  const served = await planDirectoryServer(planMountName(scope), planArtifactsDir(scope), {
    indexFile: 'plan.html',
    onMessage: planBrowserMessageSender,
    onAction: async (raw) => {
      if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid plan action');
      const value = raw as Record<string, unknown>;
      if ((value['action'] === 'start' || value['action'] === 'approve') && typeof value['revision'] === 'string') {
        const state = getPlanReviewState(scope);
        if (state.acceptedRevision === value['revision'] && getCurrentPlanReadModel(ctx, scope).authorization.startReceiptId && ['executing', 'verifying', 'complete'].includes(state.phase)) return { updated: false };
        const started = startReviewedPlan(scope, value['revision'], ctx);
        if (!started.ok) throw new Error(started.message);
        writeCurrentPlanArtifacts(ctx, scope, 'active');
      } else if (value['action'] === 'changes' || value['action'] === 'revise' || value['action'] === 'reject') {
        const changed = requestPlanChanges(scope);
        if (!changed.ok) throw new Error(changed.message);
        if (typeof value['notes'] === 'string' && value['notes'].trim()) {
          addPlanDecision(
            scope,
            value['action'] === 'reject' ? 'Rejected plan' : 'Requested plan changes',
            value['notes'],
          );
        }
        writeCurrentPlanArtifacts(ctx, scope, 'draft');
      } else throw new Error('Invalid plan action');
      refreshPlanUi(ctx);
      return { updated: true };
    },
  });
  if (!served) return undefined;
  enablePlanHtmlSync(scope);
  if (ctx?.hasUI && ctx.mode === 'tui') {
    const opened = await openPlanHtml(served.url);
    if (!opened.ok && opened.message) ctx.ui?.notify?.(opened.message, 'warn');
  }
  return served.url;
}
