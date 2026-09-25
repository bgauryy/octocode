import type { ExternalPlanScope } from './plan-contract.js';
/**
 * plan-presentation — rendering helpers, artifact writers, and shared tool utilities.
 * Used by both plan-command.ts and plan-registration.ts.
 */

import path from 'node:path';
import { pathToFileURL } from 'node:url';
import type { PlanScope } from './plan-scope.js';
import type { PiContext } from '../../types.js';
import { getCurrentPlanReadModel, renderPlanContext, type PlanReadModelV1 } from '../plan-read-model.js';
import {
  writeCurrentPlanArtifacts as writeCanonicalPlanArtifacts,
  readRfcDoc,
} from '../plan-html.js';
import {
  getPlanRfc,
  getPlanCoordination,
  updatePlanCoordination,
} from './plan-store.js';
import { MARK, displayStatus, dependencyIndexes } from './plan-types.js';
import type { DisplayStatus, PlanStep } from './plan-types.js';
import { appendSessionAuditForContext } from '../session-audit.js';
import { createSessionArtifactContext } from '../session-artifacts.js';
import { projectSessionPlan } from '../session-index.js';

const TEXT_MARK: Record<DisplayStatus, string> = { ...MARK, blocked: '[!]' };

export function renderList(steps: PlanStep[]): string {
  if (steps.length === 0) return '(no active plan)';
  return steps.map((s, i) => {
    const ds = displayStatus(s, steps);
    const dependencies = dependencyIndexes(s, steps);
    const needs = ds === 'blocked' && dependencies.length ? ` (needs ${dependencies.join(',')})` : '';
    return `${TEXT_MARK[ds]} ${i + 1}. ${s.text}${needs}`;
  }).join('\n');
}

export function planPresentation(ctx: PiContext | undefined, scope: PlanScope) {
  const plan = getCurrentPlanReadModel(ctx, scope);
  return { plan, steps: plan.tasks, addendum: renderPlanContext(plan) };
}

export function planWorkspace(scope: PlanScope): string {
  return scope.split('\0')[0] || scope;
}

export function requestedPlanScope(scope: PlanScope, explicit?: ExternalPlanScope): ExternalPlanScope {
  if (explicit) return explicit;
  const mode = getPlanCoordination(scope).mode;
  return mode === 'required' ? 'shared' : mode === 'local' ? 'session' : 'auto';
}

export function sharedStartContractError(steps: PlanStep[]): string | undefined {
  const stepIds = new Set(steps.map((step) => step.id));
  for (const [index, step] of steps.entries()) {
    if (step.dependsOnStepIds?.some((id) => !stepIds.has(id))) {
      const missingDependency = step.dependsOnStepIds.find((id) => !stepIds.has(id));
      return `step ${index + 1} references missing dependency ${missingDependency}`;
    }
    if (!step.paths?.length && !step.reasoning?.trim()) {
      return `step ${index + 1} must declare paths or explain why it has no path scope`;
    }
    if (!step.acceptance?.trim()) return `step ${index + 1} must declare acceptance criteria`;
  }
  return undefined;
}

export function projectPlanIndexes(ctx: PiContext | undefined, model: PlanReadModelV1 | undefined): void {
  if (!ctx) return;
  try {
    projectSessionPlan(createSessionArtifactContext(ctx), model);
  } catch (error) {
    appendSessionAuditForContext(ctx, {
      event: 'session.projection.failed',
      detail: { message: error instanceof Error ? error.message : String(error) },
    });
  }
}

export function writeCurrentPlanArtifacts(ctx: PiContext | undefined, scope: PlanScope, status: 'draft' | 'approved' | 'active' = 'active') {
  const artifacts = writeCanonicalPlanArtifacts(ctx, scope, { status, workspace: planWorkspace(scope) });
  projectPlanIndexes(ctx, getCurrentPlanReadModel(ctx, scope));
  return artifacts;
}

/** Compact, review-safe handoff for local-file, chat, and headless surfaces. */
export function buildRfcReviewTldr(
  scope: PlanScope,
  steps: PlanStep[],
  revision: string,
  artifacts?: { htmlPath: string; mdPath: string },
): string {
  const rfc = readRfcDoc(scope);
  const title = rfc?.markdown.match(/^#\s+(.+?)\s*$/m)?.[1] ?? 'RFC review';
  const status = rfc?.status ?? 'Draft';
  const localPath = rfc?.path ?? getPlanRfc(scope) ?? '(RFC path unavailable)';
  const fileUri = path.isAbsolute(localPath) ? pathToFileURL(localPath).href : undefined;
  const stepLines = steps.slice(0, 5).map((step, index) => `  ${index + 1}. ${step.text}`);
  if (steps.length > 5) stepLines.push(`  … ${steps.length - 5} more in plan.md`);
  return [
    `[PLAN] RFC plan overview · rev ${revision.slice(0, 8)}`,
    '',
    'Summary',
    `- ${title} · ${status}`,
    `- ${steps.length} dependency-ordered step${steps.length === 1 ? '' : 's'}.`,
    ...stepLines,
    '',
    `RFC file: ${localPath}`,
    fileUri ? `RFC URI: ${fileUri}` : undefined,
    artifacts ? `Plan Markdown: ${artifacts.mdPath}` : undefined,
    artifacts ? `Plan HTML: ${artifacts.htmlPath}` : undefined,
    '',
    'Decision:',
    '- Review or start implementation: open the plan from /configuration',
  ].filter((line): line is string => typeof line === 'string').join('\n');
}


export function configurePlanScope(scope: PlanScope, requested?: ExternalPlanScope): void {
 if (requested === 'shared') throw new Error('Shared plans are retired; use a session plan and assign work through agent communication.');
 if (getPlanCoordination(scope).mode !== 'local') updatePlanCoordination(scope, { mode: 'local', localReason: 'Pi session plan' });
}
export function ensureUnifiedProjection(scope: PlanScope, explicit: ExternalPlanScope | undefined, _ctx?: PiContext): 'session' { configurePlanScope(scope, explicit); return 'session'; }
