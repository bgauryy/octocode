/**
 * plan-registration — tool schema, execute handler, and registration.
 * Owns: inferConsequential, executePlanQuery, registerPlanTool.
 * Imports from planning module split; does not import from active-plan or plan-tool.
 */

import path from 'node:path';
import type { ToolDefinition, ToolCallResult, PiContext, PiTheme, RenderResultOptions } from '../../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from '../octocode-tools.js';
import { CLI_STATUS_TEXT } from '../../tui/cli-design.js';
import { runAskPrompt, type AskOutcome } from '../ask-user-tool.js';
import { planArtifactsDir } from '../plan-html.js';
import {
  PLAN_APPROVE_DESC,
  PLAN_APPROVE_LABEL,
  PLAN_APPROVAL_HEADER,
  PLAN_PROPOSE_HINT,
  PLAN_REJECT_DESC,
  PLAN_REJECT_LABEL,
  PLAN_RFC_REVIEW_HEADER,
} from '../../tui/content.js';
import { buildQueryCallBlocks, buildToolView } from '../render-helpers.js';
import { setManagedActivity } from '../runtime-renderer.js';
import {
  completeExternalPlanTask,
  finalizeExternalPlan,
  type ObservedCheckReceipt,
  type ExternalPlanScope,
} from '@octocodeai/octocode-awareness/host';
import { getAwarenessAgentId } from '../awareness-shared.js';
import { assertPersistentAwarenessEnabled } from '../storage-policy.js';
import { executeQueryBatch, toToolSchema, type QueryRecord } from '../query-envelope.js';
import { appendSessionAuditForContext } from '../session-audit.js';

// ─── Planning modules ────────────────────────────────────────────────────────
import {
  activePlanScope,
  setPlan,
  setPlanLifecycle,
  finishPlanVerification,
  clearPlan,
  getPlan,
  getPlanReviewState,
  getPlanCoordination,
  resolveRfcPath,
  setPlanRfc,
  getPlanRfc,
  addPlanDecision,
  getPlanDecisions,
} from './plan-store.js';
import { activatePlan, addStep, startStep, restorePlanSteps, completeStep, removeStep } from './plan-executor.js';
import {
  proposePlanReview,
  requestPlanChanges,
} from './plan-lifecycle.js';
import {
  renderList,
  planPresentation,
  planWorkspace,
  configurePlanScope,
  requestedPlanScope,
  ensureUnifiedProjection,
  buildRfcReviewTldr,
  projectPlanIndexes,
  writeCurrentPlanArtifacts,
} from './plan-presentation.js';
import {
  tearDownPlanHtml,
  refreshPlanUi,
  startReviewedPlan,
  buildPlanStartAuthorizationOptionId,
  setPlanBrowserMessageSender,
} from './plan-command.js';
import { stepLabel, depsMet, type PlanStep, type StepInput } from './plan-types.js';

// ─── Constants ───────────────────────────────────────────────────────────────

/** At/above this step count a plan is treated as consequential regardless of self-report. */
const CONSEQUENTIAL_STEP_COUNT = 5;
/** Risk vocabulary that flags consequential work in a step’s text. */
const RISK_RE = /\b(migrat|schema|auth|delete|\bdrop\b|truncate|rename|breaking|public[\s-]?api|secret|credential|\btoken\b|encrypt|permission|rollback|backfill|lockfile|release)\w*/i;

/** Cap on questions per clarify call — a bounded interview, not an interrogation. */
const MAX_CLARIFY = 3;

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;
type PlanAction = 'set' | 'propose' | 'clarify' | 'add' | 'start' | 'complete' | 'remove' | 'clear' | 'show';

/** Fields allowed per action — used to reject stray root-level fields. */
const PLAN_ACTION_FIELDS: Readonly<Record<PlanAction, readonly string[]>> = Object.freeze({
  set: ['scope', 'steps', 'consequential', 'reason', 'rfcPath'],
  propose: ['scope', 'steps', 'consequential', 'reason', 'rfcPath'],
  clarify: ['questions'],
  add: ['scope', 'text', 'activeForm', 'dependsOn', 'paths', 'taskReasoning', 'acceptance', 'checkCommand'],
  start: ['scope', 'index', 'revision', 'authorizationInteractionId'],
  complete: ['scope', 'index', 'receipt'],
  remove: ['scope', 'index'],
  clear: ['scope'],
  show: ['scope'],
});

function assertPlanActionFields(query: QueryRecord, action: PlanAction): void {
  const allowed = new Set(['reasoning', 'action', ...PLAN_ACTION_FIELDS[action]]);
  const extra = Object.keys(query).filter((field) => !allowed.has(field));
  if (extra.length > 0) throw new Error(`action:${action} does not accept ${extra.join(', ')}.`);
}

/** One clarify-phase question: a prompt plus optional multiple-choice options. */
interface ClarifyQuestion {
  prompt: string;
  options?: Array<{ value?: string; label: string; description?: string; recommended?: boolean; pros?: string[]; cons?: string[] }>;
}

interface PlanParams extends QueryRecord {
  action: PlanAction;
  scope?: ExternalPlanScope;
  receipt?: ObservedCheckReceipt;
  steps?: StepInput[];
  text?: string;
  activeForm?: string;
  dependsOn?: number[];
  paths?: string[];
  taskReasoning?: string;
  acceptance?: string;
  checkCommand?: string;
  index?: number;
  /** For reviewed action:start — the exact displayed RFC revision. */
  revision?: string;
  /** For noninteractive reviewed action:start — the answered authorization interaction. */
  authorizationInteractionId?: string;
  /** For propose: require RFC review when true; false plus reason may justify overriding heuristic inference. */
  consequential?: boolean;
  /** For set/propose: path to the reviewable RFC (a `.octocode/rfc/<name>/` dir or its RFC.md). Renders on the plan page. */
  rfcPath?: string;
  /** For action:clarify — up to 3 high-impact questions to ask the user before proposing. */
  questions?: ClarifyQuestion[];
  /** Required with consequential:false to override the heuristic. */
  reason?: string;
}

// ─── Exported pure helpers ──────────────────────────────────────────────────

/**
 * Heuristic "does this look consequential?" from the proposed steps alone — step
 * count and risk vocabulary. Pure and exported for testing. Returns the verdict
 * plus the human-readable signals that fired (for the gate’s block message).
 */
export function inferConsequential(steps: StepInput[]): { consequential: boolean; signals: string[] } {
  const texts = steps.map((s) => (typeof s === 'string' ? s : s?.text ?? ''));
  const signals: string[] = [];
  if (texts.length >= CONSEQUENTIAL_STEP_COUNT) signals.push(`${texts.length} steps`);
  const hits = new Set<string>();
  for (const t of texts) {
    const m = t.match(RISK_RE);
    if (m) hits.add(m[0].toLowerCase());
  }
  if (hits.size) signals.push(`risk terms: ${[...hits].slice(0, 4).join(', ')}`);
  return { consequential: signals.length > 0, signals };
}

// ─── Private execute helpers ──────────────────────────────────────────────────

function auditPlanEvent(
  ctx: PiContext | undefined,
  scope: string,
  event: string,
  detail: Record<string, unknown> = {},
): void {
  const review = getPlanReviewState(scope);
  appendSessionAuditForContext(ctx, {
    event: `plan.${event}`,
    detail: {
      phase: review.phase,
      generation: review.generation,
      steps: getPlan(scope).length,
      ...detail,
    },
  });
}

/**
 * Core per-query plan executor. Handles one action (PlanParams) against the
 * given ctx and returns a ToolCallResult. Called from inside executeQueryBatch.
 */
async function executePlanQuery(p: PlanParams, ctx: PiContext | undefined): Promise<ToolCallResult> {
  const scope = activePlanScope(ctx);
  let steps: PlanStep[];
  // Reject unavailable shared writes before changing the local plan or its scope.
  if (p.action !== 'show' && p.action !== 'clarify'
    && (requestedPlanScope(scope, p.scope) === 'shared' || getPlanCoordination(scope).awarenessPlanId)) {
    assertPersistentAwarenessEnabled();
  }

  // ── Clarify phase (interview) ───────────────────────────────────────────
  if (p.action === 'clarify') {
    const clarifyResult = (text: string, isError = false, extraDetails: Record<string, unknown> = {}): ToolCallResult => ({
      content: [{ type: 'text' as const, text }],
      ...(isError ? { isError: true } : {}),
      details: { action: 'clarify', decisions: getPlanDecisions(scope), ...extraDetails },
    }) as unknown as ToolCallResult;
    const questions = (Array.isArray(p.questions) ? p.questions : []).filter((q) => q && String(q.prompt ?? '').trim()).slice(0, MAX_CLARIFY);
    if (questions.length === 0) {
      return clarifyResult('[PLAN] clarify needs a questions[] list (≤3 high-impact questions the repo cannot answer). Skip clarify for obvious work.', true);
    }
    if (!ctx) {
      return clarifyResult(`[PLAN] this host cannot prompt — ask these inline and continue:\n${questions.map((q, i) => `${i + 1}. ${q.prompt}`).join('\n')}`);
    }
    const settleClarify = (detail: string): void => {
      if (getPlanReviewState(scope).phase === 'abandoned') setPlanLifecycle(scope, 'researching');
      if (getPlanReviewState(scope).phase !== 'draft') setPlanLifecycle(scope, 'draft');
      setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail });
    };
    const recorded: string[] = [];
    let halted: string | undefined;
    let pendingInteraction: AskOutcome['interaction'] | undefined;
    let qi = 0;
    while (qi < questions.length) {
      const q = questions[qi]!;
      const prompt = String(q.prompt).trim();
      const outcome = await runAskPrompt(ctx, {
        question: prompt,
        options: (q.options ?? []).map((o) => ({
          value: o.value ?? o.label,
          label: o.label,
          description: o.description,
          recommended: o.recommended,
          pros: o.pros,
          cons: o.cons,
        })),
        pagination: { current: qi + 1, total: questions.length },
      });
      if (!outcome) {
        halted = '[PLAN] clarify cancelled before an answer was recorded.';
        break;
      }
      if (outcome.status === 'pending') {
        pendingInteraction = outcome.interaction;
        halted = `[PLAN] clarify paused pending continuation (correlation=${outcome.interaction?.correlationId ?? 'unavailable'})`;
        break;
      }
      if (outcome.status === 'unavailable') {
        addPlanDecision(scope, prompt, '(awaiting user reply)');
        recorded.push(prompt);
        qi++;
        continue;
      }
      if (outcome.status !== 'text' && outcome.status !== 'selected') {
        halted = `[PLAN] clarify ${outcome.status}.`;
        break;
      }
      const answer = outcome.status === 'text'
        ? (outcome.value ?? '').trim()
        : String(outcome.label ?? outcome.value ?? '');
      if (!answer) { qi++; continue; }
      addPlanDecision(scope, prompt, answer);
      recorded.push(prompt);
      qi++;
    }
    if (recorded.length > 0) auditPlanEvent(ctx, scope, 'clarify', { decisionsRecorded: recorded.length });
    const decisionsText = recorded.length > 0
      ? `[PLAN] recorded ${recorded.length} decision(s) · decision-complete. Proceed to propose.`
      : '[PLAN] no new decisions recorded · decision-complete.';
    if (halted) {
      if (!pendingInteraction) {
        settleClarify('decision-cancelled');
        return clarifyResult(halted);
      }
      return clarifyResult(halted, false, {
        pendingInteraction: {
          version: pendingInteraction!.version,
          interactionId: pendingInteraction!.interactionId,
          correlationId: pendingInteraction!.correlationId,
          sessionId: pendingInteraction!.sessionId,
        },
        continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
      });
    }
    settleClarify('decision-complete');
    return clarifyResult(decisionsText);
  }

  // ── RFC gate (set/propose only) ───────────────────────────────────────
  const resolveGate = (): { rfc?: string; hasNewRfc: boolean; error?: ToolCallResult } => {
    const gateError = (text: string, error = 'rfc-gate'): ToolCallResult => ({
      content: [{ type: 'text' as const, text }],
      isError: true,
      details: { action: p.action, error },
    }) as unknown as ToolCallResult;
    const supplied = typeof p.rfcPath === 'string' ? p.rfcPath.trim() : '';
    if (supplied) {
      const res = resolveRfcPath(planWorkspace(scope), supplied);
      if (res.error) {
        return { hasNewRfc: false, error: gateError(`[PLAN] rfcPath ${supplied} did not resolve: ${res.error}`, 'rfc-unresolvable') };
      }
      const existingRfc = getPlanRfc(scope);
      return { rfc: res.path!, hasNewRfc: res.path !== existingRfc };
    }
    const existingRfc = getPlanRfc(scope);
    if (existingRfc) return { rfc: existingRfc, hasNewRfc: false };
    if (p.action === 'propose') {
      const stepsForGate = Array.isArray(p.steps) ? p.steps : [];
      const consequence = inferConsequential(stepsForGate);
      const isConsequential = p.consequential ?? consequence.consequential;
      if (isConsequential) {
        const signals = consequence.signals.join('; ');
        return {
          hasNewRfc: false,
          error: gateError(
            `[PLAN] consequential proposal requires a reviewable RFC (${signals}). Create or update .octocode/rfc/<name>/RFC.md and pass rfcPath, or set consequential:false with a non-empty reason that justifies the override.`,
            'rfc-required',
          ),
        };
      }
    }
    return { rfc: existingRfc, hasNewRfc: false };
  };

  switch (p.action) {
    case 'set': {
      const gate = resolveGate();
      if (gate.error) return gate.error;
      steps = setPlan(scope, Array.isArray(p.steps) ? p.steps : []);
      configurePlanScope(scope, p.scope);
      if (gate.hasNewRfc) setPlanRfc(scope, gate.rfc);
      ensureUnifiedProjection(scope, p.scope, ctx);
      steps = getPlan(scope);
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    }
    case 'propose': {
      ctx?.ui?.notify?.('Creating plan…', 'info');
      setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail: 'Creating plan…' });
      const gate = resolveGate();
      if (gate.error) {
        refreshPlanUi(ctx);
        return gate.error;
      }
      steps = setPlan(scope, Array.isArray(p.steps) ? p.steps : [], 'draft');
      configurePlanScope(scope, p.scope);
      if (gate.hasNewRfc) setPlanRfc(scope, gate.rfc);

      if (gate.rfc) {
        const proposed = proposePlanReview(scope);
        if (!proposed.ok) {
          return {
            content: [{ type: 'text' as const, text: `[PLAN] could not enter RFC review: ${proposed.message}` }],
            isError: true,
            details: { action: p.action, error: proposed.code, steps: proposed.steps },
          } as unknown as ToolCallResult;
        }
        steps = proposed.steps;
        const revision = proposed.state.revision!;
        const artifacts = writeCurrentPlanArtifacts(ctx, scope, 'draft');
        refreshPlanUi(ctx);
        setManagedActivity(ctx, { kind: 'reviewing', planScope: scope, revision });
        const summary = buildRfcReviewTldr(scope, steps, revision, artifacts);
        const planId = getPlanCoordination(scope).sourcePlanKey;
        const startOptionId = buildPlanStartAuthorizationOptionId(planId, revision);
        auditPlanEvent(ctx, scope, 'propose', { revision });
        const outcome = ctx
          ? await runAskPrompt(ctx, {
              question: `Plan overview ready · rev ${revision.slice(0, 8)} · ${steps.length} step${steps.length === 1 ? '' : 's'} — start implementation?`,
              headerLabel: PLAN_RFC_REVIEW_HEADER,
              kind: 'authorization',
              freeTextLabel: 'Request changes',
              options: [
                {
                  value: 'start',
                  brokerId: startOptionId,
                  label: PLAN_APPROVE_LABEL,
                  description: 'approve this exact RFC revision and begin the first runnable step',
                  recommended: true,
                  preview: summary,
                },
                {
                  value: 'changes',
                  label: PLAN_REJECT_LABEL,
                  description: PLAN_REJECT_DESC,
                  preview: summary,
                },
              ],
            })
          : undefined;

        if (outcome?.status === 'selected' && outcome.value === 'start') {
          const started = startReviewedPlan(scope, revision, ctx);
          if (!started.ok) {
            return {
              content: [{ type: 'text', text: `[PLAN] implementation did not start: ${started.message}` }],
              isError: true,
              details: { action: p.action, ...planPresentation(ctx, scope), error: 'start-failed', revision },
            } as unknown as ToolCallResult;
          }
          steps = started.steps;
          const activeArtifacts = writeCurrentPlanArtifacts(ctx, scope, 'active');
          refreshPlanUi(ctx);
          const startVerdict = `[PLAN] approved and started · rev ${revision.slice(0, 8)}`;
          auditPlanEvent(ctx, scope, 'start', { revision, source: 'propose' });
          return {
            content: [{ type: 'text', text: startVerdict }],
            details: { action: p.action, ...planPresentation(ctx, scope), verdict: startVerdict, revision, decision: 'start', ...(activeArtifacts ? { artifacts: activeArtifacts } : {}) },
          } as unknown as ToolCallResult;
        }

        if (outcome?.status === 'selected' && outcome.value === 'changes') {
          const changed = requestPlanChanges(scope);
          if (changed.ok) writeCurrentPlanArtifacts(ctx, scope, 'draft');
          refreshPlanUi(ctx);
          const changesVerdict = '[PLAN] changes requested — revise the RFC and re-propose.';
          auditPlanEvent(ctx, scope, 'changes', { revision, feedbackProvided: false });
          return {
            content: [{ type: 'text', text: changesVerdict }],
            details: { action: p.action, ...planPresentation(ctx, scope), verdict: changesVerdict, revision, decision: outcome.status, ...(artifacts ? { artifacts } : {}) },
          } as unknown as ToolCallResult;
        }

        if (outcome?.status === 'text' && outcome.value) {
          const feedback = outcome.value.trim();
          const changed = requestPlanChanges(scope);
          if (changed.ok) writeCurrentPlanArtifacts(ctx, scope, 'draft');
          refreshPlanUi(ctx);
          if (feedback) addPlanDecision(scope, 'Requested plan changes', feedback);
          const textVerdict = `[PLAN] changes requested: ${feedback}\nRevise the RFC and re-propose.`;
          auditPlanEvent(ctx, scope, 'changes', { revision, feedbackProvided: Boolean(feedback) });
          return {
            content: [{ type: 'text', text: textVerdict }],
            details: { action: p.action, ...planPresentation(ctx, scope), verdict: textVerdict, revision, decision: outcome.status, ...(artifacts ? { artifacts } : {}) },
          } as unknown as ToolCallResult;
        }

        const pendingOrUnavailableVerdict = (() => {
          if (!outcome || outcome.status === 'unavailable') {
            return '[PLAN] plan ready — show this overview inline. Decision: Start implementation or Request changes.';
          }
          if (outcome.status === 'pending') {
            return `[PLAN] approval pending (correlation=${outcome.interaction?.correlationId ?? 'unavailable'}) — do not execute until the durable host continuation records approval.`;
          }
          if (outcome.status === 'cancelled' || outcome.status === 'back') {
            return '[PLAN] review cancelled — the RFC remains ready.';
          }
          return '[PLAN] rejected — do not execute. Ask the user how to proceed.';
        })();
        // For non-interactive outcomes (unavailable/pending), the widget did not run — include the plan overview.
        // For interactive outcomes (cancelled/back/rejected), the plan widget was shown; suppress steps and file paths.
        const showFullContext = !outcome || outcome.status === 'unavailable' || outcome.status === 'pending';
        return {
          content: [{ type: 'text', text: showFullContext ? `${pendingOrUnavailableVerdict}\n${summary}` : pendingOrUnavailableVerdict }],
          details: {
            action: p.action,
            ...planPresentation(ctx, scope),
            verdict: pendingOrUnavailableVerdict,
            revision,
            decision: outcome?.status ?? 'unavailable',
            ...(outcome?.status === 'pending' && outcome.interaction ? {
              pendingInteraction: {
                version: outcome.interaction.version,
                interactionId: outcome.interaction.interactionId,
                correlationId: outcome.interaction.correlationId,
                sessionId: outcome.interaction.sessionId,
              },
              continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
            } : {}),
            ...(artifacts ? { artifacts } : {}),
          },
        } as unknown as ToolCallResult;
      }

      // Non-RFC propose (simple approval gate)
      writeCurrentPlanArtifacts(ctx, scope, 'draft');
      refreshPlanUi(ctx);
      auditPlanEvent(ctx, scope, 'propose');
      const proposeOutcome = ctx
        ? await runAskPrompt(ctx, {
            question: `${steps.length} step${steps.length === 1 ? '' : 's'} ready for review — ${PLAN_PROPOSE_HINT}`,
            headerLabel: PLAN_APPROVAL_HEADER,
            options: [
              {
                value: 'start',
                label: PLAN_APPROVE_LABEL,
                description: PLAN_APPROVE_DESC,
                recommended: true,
              },
              {
                value: 'reject',
                label: PLAN_REJECT_LABEL,
                description: PLAN_REJECT_DESC,
              },
            ],
          })
        : undefined;
      const approved = proposeOutcome?.status === 'selected' && proposeOutcome.value === 'start';
      if (approved) {
        steps = activatePlan(scope);
        ensureUnifiedProjection(scope, p.scope, ctx);
        steps = getPlan(scope);
        refreshPlanUi(ctx);
        auditPlanEvent(ctx, scope, 'start', { source: 'propose' });
      }
      const proposeVerdict = (() => {
        if (!proposeOutcome || proposeOutcome.status === 'unavailable') {
          return '[PLAN] proposed, but this host cannot prompt — present the plan inline and get approval in your reply before executing.';
        }
        if (proposeOutcome.status === 'pending') {
          return `[PLAN] approval pending (correlation=${proposeOutcome.interaction?.correlationId ?? 'unavailable'}) — do not execute until the durable host continuation records approval.`;
        }
        if (approved) {
          return '[PLAN] approved and started — keep steps updated via complete.';
        }
        if (proposeOutcome.status === 'text' && proposeOutcome.value) {
          return `[PLAN] adjust requested: ${proposeOutcome.value}\nRevise the plan and re-propose.`;
        }
        return '[PLAN] rejected — do not execute. Ask the user how to proceed.';
      })();
      if (approved) {
        writeCurrentPlanArtifacts(ctx, scope, 'approved');
      }
      // For non-interactive outcomes (unavailable/pending), no widget ran — include steps for the model to present.
      // For interactive outcomes (approved/rejected/text), the plan widget is visible; suppress steps and file paths.
      const isNonInteractivePropose = !proposeOutcome || proposeOutcome.status === 'unavailable' || proposeOutcome.status === 'pending';
      return {
        content: [{ type: 'text', text: isNonInteractivePropose ? `${proposeVerdict}\n${renderList(steps)}` : proposeVerdict }],
        details: {
          action: p.action,
          ...planPresentation(ctx, scope),
          verdict: proposeVerdict,
          ...(proposeOutcome?.status === 'pending' && proposeOutcome.interaction ? {
            pendingInteraction: {
              version: proposeOutcome.interaction.version,
              interactionId: proposeOutcome.interaction.interactionId,
              correlationId: proposeOutcome.interaction.correlationId,
              sessionId: proposeOutcome.interaction.sessionId,
            },
            continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
          } : {}),
        },
      } as unknown as ToolCallResult;
    }
    case 'add':
      steps = addStep(scope, {
        text: String(p.text ?? ''),
        ...(p.activeForm ? { activeForm: p.activeForm } : {}),
        ...(p.dependsOn ? { dependsOn: p.dependsOn } : {}),
        ...(p.paths ? { paths: p.paths } : {}),
        ...(p.taskReasoning ? { reasoning: p.taskReasoning } : {}),
        ...(p.acceptance ? { acceptance: p.acceptance } : {}),
        ...(p.checkCommand ? { checkCommand: p.checkCommand } : {}),
      });
      if (getPlanCoordination(scope).awarenessPlanId) {
        ensureUnifiedProjection(scope, p.scope, ctx);
        steps = getPlan(scope);
      }
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    case 'start':
    case 'complete':
    case 'remove': {
      const current = getPlan(scope);
      const planError = (msg: string, error: string) => ({
        content: [{ type: 'text' as const, text: `${msg}\n${renderList(current)}` }],
        isError: true,
        details: { action: p.action, ...planPresentation(ctx, scope), error },
      }) as unknown as ToolCallResult;
      if (current.length === 0) {
        return planError(`[PLAN] no active plan — nothing to ${p.action}. Use plan set first.`, 'invalid-index');
      }
      const reviewState = getPlanReviewState(scope);
      const reviewPhase = reviewState.phase;
      if (p.action === 'start' && (reviewPhase === 'in_review' || reviewPhase === 'accepted')) {
        const revision = p.revision?.trim();
        const interactionId = p.authorizationInteractionId?.trim();
        const recoveringAcceptedStart = reviewPhase === 'accepted' && !interactionId;
        if (!revision || p.index !== undefined || (reviewPhase === 'in_review' && !interactionId)) {
          return planError('[PLAN] reviewed implementation Start requires the exact revision; an in-review plan also requires authorizationInteractionId from the answered human Start interaction. index is not valid for this transition.', 'authorization-required');
        }
        if (recoveringAcceptedStart && !reviewState.acceptAuthorizationReceiptId) {
          return planError('[PLAN] the accepted plan has no persisted human authorization receipt and cannot be resumed without a new answered Start interaction.', 'authorization-required');
        }
        const planId = getPlanCoordination(scope).sourcePlanKey;
        const started = startReviewedPlan(scope, revision, ctx, interactionId ? {
          interactionId,
          expectedOptionId: buildPlanStartAuthorizationOptionId(planId, revision),
        } : undefined);
        if (!started.ok) {
          refreshPlanUi(ctx);
          return planError(`[PLAN] implementation did not start: ${started.message}`, 'authorization-required');
        }
        steps = started.steps;
        writeCurrentPlanArtifacts(ctx, scope, 'active');
        refreshPlanUi(ctx);
        auditPlanEvent(ctx, scope, 'start', { revision, source: recoveringAcceptedStart ? 'accepted-recovery' : 'interaction' });
        return {
          content: [{ type: 'text' as const, text: `[PLAN] approved and started · rev ${revision.slice(0, 8)}\n${renderList(steps)}` }],
          details: { action: p.action, ...planPresentation(ctx, scope), revision, decision: 'start' },
        } as unknown as ToolCallResult;
      }
      if (p.action === 'start' && (p.revision?.trim() || p.authorizationInteractionId?.trim())) {
        return planError(`[PLAN] reviewed Start fields are only valid while a plan is in_review or accepted (current phase: ${reviewPhase}). During execution, omit revision and authorizationInteractionId and use optional index only.`, 'wrong-start-variant');
      }
      if (p.action === 'start' && reviewPhase !== 'executing' && reviewPhase !== 'verifying') {
        return planError(`[PLAN] implementation cannot start from ${reviewPhase}; propose the plan for review and obtain an explicit human Start decision first.`, 'authorization-required');
      }
      if (p.action === 'complete' && reviewPhase !== 'executing' && reviewPhase !== 'verifying' && reviewPhase !== 'complete') {
        return planError(`[PLAN] a step cannot complete while the plan is ${reviewPhase}; Start implementation first.`, 'phase-not-executing');
      }
      let idx: number;
      if (p.index === undefined || p.index === null) {
        if (p.action === 'start') {
          idx = current.findIndex((s) => s.status === 'todo' && depsMet(s, current)) + 1;
        } else {
          const doing = current.map((s, i) => ({ step: s, index: i + 1 })).filter(({ step }) => step.status === 'doing');
          if (doing.length > 1) {
            return planError(`[PLAN] ${doing.length} steps are in progress (${doing.map((d) => d.index).join(', ')}); pass index to complete or remove the target step.`, 'ambiguous-target');
          }
          idx = (doing[0]?.index ?? 0);
        }
        if (idx < 1) {
          return p.action === 'start'
            ? planError('[PLAN] no dependency-ready step to start. Pass index to target a specific step.', 'no-runnable-step')
            : planError('[PLAN] no step is in progress. Pass index to target a specific step.', 'no-active-step');
        }
      } else {
        idx = p.index;
      }
      if (idx < 1 || idx > current.length) {
        return planError(`[PLAN] no such step ${p.index} — plan has ${current.length} step(s). Run plan show for indices.`, 'invalid-index');
      }
      if (p.action === 'start' && !depsMet(current[idx - 1]!, current)) {
        return planError(`[PLAN] step ${idx} is blocked by dependencies — complete its prerequisites before starting it.`, 'blocked-step');
      }
      const target = current[idx - 1]!;
      if (p.action === 'remove' && target.awarenessTaskId) {
        return planError('[PLAN] mapped shared steps cannot be removed in place; abandon or revise the shared plan explicitly.', 'shared-remove');
      }
      if (p.action === 'complete' && target.awarenessTaskId) {
        const coordination = getPlanCoordination(scope);
        try {
          assertPersistentAwarenessEnabled();
          const shared = completeExternalPlanTask({
            workspace: coordination.coordinationWorkspace || planWorkspace(scope),
            taskId: target.awarenessTaskId,
            agentId: getAwarenessAgentId(ctx),
            ...(p.receipt ? { receipt: p.receipt } : {}),
          });
          if (!shared.verified) {
            return planError(`[PLAN] observed check failed; shared task ${shared.task.taskId} has verification debt and the local step remains in progress.`, 'check-failed');
          }
        } catch (error) {
          return planError(`[PLAN] shared completion blocked: ${error instanceof Error ? error.message : String(error)}`, 'shared-completion');
        }
      }
      const beforeStart = p.action === 'start' ? current.map((step) => ({ ...step })) : undefined;
      steps = p.action === 'start' ? startStep(scope, idx) : p.action === 'complete' ? completeStep(scope, idx) : removeStep(scope, idx);
      if ((p.action === 'start' || p.action === 'complete') && getPlanCoordination(scope).awarenessPlanId) {
        try {
          ensureUnifiedProjection(scope, p.scope, ctx);
        } catch (error) {
          if (p.action !== 'start' || !beforeStart) throw error;
          restorePlanSteps(scope, beforeStart);
          return planError(`[PLAN] step did not start; local status and Awareness mapping were restored after shared projection failed: ${error instanceof Error ? error.message : String(error)}`, 'shared-start-projection');
        }
        steps = getPlan(scope);
      }
      if (p.action === 'complete' && steps.every((step) => step.status === 'done')) {
        refreshPlanUi(ctx);
        const coordination = getPlanCoordination(scope);
        let verified = true;
        if (coordination.awarenessPlanId) {
          assertPersistentAwarenessEnabled();
          verified = finalizeExternalPlan({
            workspace: coordination.coordinationWorkspace || planWorkspace(scope),
            planId: coordination.awarenessPlanId,
            agentId: getAwarenessAgentId(ctx),
          });
        }
        if (verified) finishPlanVerification(scope, true, 'All declared task checks passed');
        else setPlanLifecycle(scope, 'blocked', 'Shared tasks still have verification debt');
        steps = getPlan(scope);
      }
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    }
    case 'clear': {
      const current = getPlan(scope);
      if (current.some((step) => step.awarenessTaskId) && current.some((step) => step.status !== 'done')) {
        return {
          content: [{ type: 'text' as const, text: '[PLAN] mapped shared plans cannot be cleared while work is unfinished; complete or abandon the shared work first.' }],
          isError: true,
          details: { action: p.action, error: 'shared-clear', ...planPresentation(ctx, scope) },
        } as unknown as ToolCallResult;
      }
      clearPlan(scope);
      tearDownPlanHtml(scope);
      projectPlanIndexes(ctx, undefined);
      steps = [];
      break;
    }
    case 'show':
    default:
      steps = getPlan(scope);
      break;
  }
  refreshPlanUi(ctx);
  if (p.action !== 'show') {
    auditPlanEvent(ctx, scope, p.action, p.index === undefined ? {} : { index: p.index });
  }
  const done = steps.filter((s) => s.status === 'done').length;
  const header = p.action === 'clear' ? '[PLAN] cleared' : `[PLAN] ${done}/${steps.length} done`;
  const artifactHint = (p.action === 'set' || p.action === 'add' || p.action === 'start' || p.action === 'complete' || p.action === 'remove') && steps.length > 0
    ? `\nPlan doc: ${path.join(planArtifactsDir(scope), 'plan.md')}`
    : '';
  const taskIds = steps.length > 0
    ? `\nTask IDs for agent.planStep: ${steps.map((step, index) => `${index + 1}=${step.id}`).join(', ')}`
    : '';
  return {
    content: [{ type: 'text', text: `${header}\n${renderList(steps)}${taskIds}${artifactHint}` }],
    details: { action: p.action, ...planPresentation(ctx, scope) },
  } as unknown as ToolCallResult;
}

// ─── Tool registration ─────────────────────────────────────────────────────────

export function registerPlanTool(
  pi: {
    registerTool?(def: ToolDefinition): void;
    sendUserMessage?(message: string, options?: { deliverAs?: 'steer' | 'followUp'; expandPromptTemplates?: boolean }): void | Promise<void>;
  },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  setPlanBrowserMessageSender(pi.sendUserMessage
    ? (message) => pi.sendUserMessage!(message, {
        deliverAs: 'followUp',
        expandPromptTemplates: false,
      })
    : undefined);
  registerFn(pi, registeredToolNames, {
    name: 'plan',
    label: 'Plan',
    description: DIRECT_TOOL_DESCRIPTIONS.plan!,
    promptSnippet: 'Plan only complex work or an explicit planning request. Routine multi-step work and simple delegation need no plan.',
    promptGuidelines: [
      'Use set for authorized execution and propose when review is required; consequential choices need an RFC.',
      'Complete only after the declared check succeeds, never from a worker DONE claim.',
      'Encode independent-lane dependencies, start runnable steps, and complete each explicit step.',
    ],
    parameters: (() => {
      const reasoning = z.string().max(400).optional().describe('Optional batch label.');
      const scope = z.enum(['auto','session','shared']).optional();
      const step = z.union([
        z.string().min(1),
        z.strictObject({
          text: z.string().min(1),
          acceptance: z.string().optional(),
          activeForm: z.string().optional(),
          checkCommand: z.string().optional(),
          dependsOn: z.array(z.number().int().min(1)).optional().describe('Earlier 1-based prerequisite indices.'),
          paths: z.array(z.string()).optional().describe('Workspace-relative write ownership.'),
          reasoning: z.string().optional(),
        }),
      ]);
      const receipt = z.strictObject({
        command: z.string().min(1).describe('Exact declared check that ran.'),
        status: z.enum(['SUCCESS','FAILED']).describe('Observed result; never predict it.'),
        message: z.string().min(1).describe('Observed counts or failure cause.'),
      });
      const questions = z.array(z.strictObject({
        prompt: z.string().min(1).describe('Decision-changing question repository evidence cannot answer.'),
        options: z.array(z.strictObject({
          label: z.string().min(1), value: z.string().optional(), description: z.string().optional(),
          recommended: z.boolean().optional(), pros: z.array(z.string()).optional(), cons: z.array(z.string()).optional(),
        })).optional().describe('Distinct choices; omit for free text. The UI always permits discussion.'),
      })).min(1).max(3);
      const query = z.union([
        z.strictObject({ reasoning, action: z.enum(['set']), scope, steps: z.array(step).min(1), consequential: z.boolean().optional(), reason: z.string().optional(), rfcPath: z.string().optional() }),
        z.strictObject({ reasoning, action: z.enum(['propose']), scope, steps: z.array(step).min(1), consequential: z.boolean().optional(), reason: z.string().optional(), rfcPath: z.string().optional().describe('Required for consequential review; workspace `.octocode/rfc/<name>/` directory or RFC.md.') }),
        z.strictObject({ reasoning, action: z.enum(['clarify']), questions }),
        z.strictObject({ reasoning, action: z.enum(['add']), scope, text: z.string().min(1), activeForm: z.string().optional(), dependsOn: z.array(z.number().int().min(1)).optional(), paths: z.array(z.string()).optional(), taskReasoning: z.string().optional(), acceptance: z.string().optional(), checkCommand: z.string().optional() }),
        z.strictObject({
          reasoning,
          action: z.enum(['start']),
          scope,
          index: z.number().int().min(1).optional().describe('1-based runnable step; omit to start next dependency-ready step.'),
        }),
        z.strictObject({
          reasoning,
          action: z.enum(['start']),
          scope,
          revision: z.string().min(1).describe('Reviewed plan only: exact displayed RFC revision; omit index.'),
          authorizationInteractionId: z.string().min(1).optional().describe('Answered Start interaction; required in review; omit only for persisted accepted-recovery.'),
        }),
        z.strictObject({ reasoning, action: z.enum(['complete']), scope, index: z.number().int().min(1).optional(), receipt: receipt.optional() }),
        z.strictObject({ reasoning, action: z.enum(['remove']), scope, index: z.number().int().min(1).optional() }),
        z.strictObject({ reasoning, action: z.enum(['clear']), scope }),
        z.strictObject({ reasoning, action: z.enum(['show']), scope }),
      ]);
      return toToolSchema(z.strictObject({
        queries: z.array(query).min(1).max(100).describe('Plan transitions execute sequentially in source order.'),
        queryRunType: z.enum(['sequential']).default('sequential').optional(),
      }));
    })(),

    async execute(toolCallId: string, rawArgs: Record<string, unknown>, signal?: AbortSignal, onUpdate?: (update: ToolCallResult) => void, ctx?: PiContext) {
      return executeQueryBatch({
        toolCallId,
        raw: rawArgs,
        signal,
        onUpdate,
        ctx,
        passthroughSingle: true,
        preflight(query) {
          const action = String(query['action'] ?? '');
          const VALID_ACTIONS: PlanAction[] = ['set', 'propose', 'clarify', 'add', 'start', 'complete', 'remove', 'clear', 'show'];
          if (!VALID_ACTIONS.includes(action as PlanAction)) {
            throw new Error(`unknown plan action: "${action}". Must be one of: ${VALID_ACTIONS.join(', ')}.`);
          }
          assertPlanActionFields(query, action as PlanAction);
          if (query['scope'] !== undefined && !['auto', 'session', 'shared'].includes(String(query['scope']))) {
            throw new Error(`invalid scope: ${String(query['scope'])}. Must be auto, session, or shared.`);
          }
          if (action === 'set' || action === 'propose') {
            if (!Array.isArray(query['steps'])) throw new Error(`action:${action} — steps must be an array.`);
            if (query['steps'].length === 0) throw new Error(`action:${action} — steps must not be empty; use action:clear to remove a plan.`);
          }
          if (action === 'add') {
            const text = typeof query['text'] === 'string' ? query['text'].trim() : '';
            if (!text) throw new Error('action:add requires a non-empty text field.');
          }
          if (query['receipt'] !== undefined) {
            const receipt = query['receipt'];
            if (action !== 'complete' || !receipt || typeof receipt !== 'object' || Array.isArray(receipt)) {
              throw new Error('receipt is only valid as an object for action:complete.');
            }
            const record = receipt as Record<string, unknown>;
            if (typeof record['command'] !== 'string' || !record['command'].trim()) throw new Error('receipt.command is required.');
            if (record['status'] !== 'SUCCESS' && record['status'] !== 'FAILED') throw new Error('receipt.status must be SUCCESS or FAILED.');
            if (typeof record['message'] !== 'string' || !record['message'].trim()) throw new Error('receipt.message is required.');
          }
          if (action === 'start') {
            const hasRevision = typeof query['revision'] === 'string' && query['revision'].trim().length > 0;
            const hasInteraction = typeof query['authorizationInteractionId'] === 'string' && query['authorizationInteractionId'].trim().length > 0;
            if (hasInteraction && !hasRevision) {
              throw new Error('reviewed action:start authorizationInteractionId requires revision.');
            }
            if (hasRevision && query['index'] !== undefined) {
              throw new Error('reviewed action:start cannot include index.');
            }
          }
          if ((action === 'start' || action === 'complete' || action === 'remove') && query['index'] != null) {
            const idx = Number(query['index']);
            if (!Number.isInteger(idx) || idx < 1) {
              throw new Error(`action:${action} — index must be a positive integer when provided (got ${String(query['index'])}).`);
            }
          }
        },
        async execute(query, _queryIndex, _itemId, _sig, _upd, queryCtx) {
          return executePlanQuery(query as PlanParams, queryCtx);
        },
        summarize(result, query) {
          const action = String(query['action'] ?? 'unknown');
          const firstLine = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text?.split('\n').find(Boolean)?.trim();
          return firstLine ?? (result.isError ? `plan(${action}) failed` : `plan(${action}) ok`);
        },
      });
    },

    renderCall(raw: unknown, theme?: PiTheme) {
      return buildQueryCallBlocks(raw, theme, (singleArgs) => {
        const queries = Array.isArray(singleArgs['queries'])
          ? singleArgs['queries'] as Record<string, unknown>[]
          : [];
        const q = (queries[0] ?? {}) as unknown as PlanParams;
        const extra = q.action === 'set' || q.action === 'propose'
          ? ` (${(q.steps ?? []).length} steps)`
          : q.index ? ` #${q.index}` : '';
        return buildToolView({
          name: 'plan',
          state: 'request',
          segments: [
            { text: q.action, token: 'bright' },
            ...(extra ? [{ text: extra.trim().replace(/^\(|\)$/g, ''), token: 'count' as const }] : []),
          ],
        }, theme);
      });
    },

    renderResult(result: ToolCallResult, opts: RenderResultOptions, theme?: PiTheme) {
      if (opts.isPartial) {
        return buildToolView(() => ({ name: 'plan', state: 'running', status: CLI_STATUS_TEXT.running }), theme);
      }
      const r = result as ToolCallResult & { details?: { steps?: PlanStep[]; action?: string; results?: unknown[] } };
      const resultText = r.content?.find((part) => part.type === 'text')?.text ?? '';
      if (r.isError) {
        return buildToolView({
          name: 'plan',
          state: 'error',
          segments: [{ text: (resultText || 'plan operation failed').split('\n')[0]!.trim(), token: 'error' }],
        }, theme);
      }
      if (Array.isArray(r?.details?.results)) {
        const count = r.details!.results!.length;
        return buildToolView({ name: 'plan', state: 'success', segments: [{ text: `${count} operation${count === 1 ? '' : 's'}`, token: 'count' }] }, theme);
      }
      const steps = r?.details?.steps ?? [];
      if (r?.details?.action === 'clear') {
        return buildToolView({ name: 'plan', state: 'success', segments: [{ text: 'cleared', token: 'dim' }] }, theme);
      }
      if (steps.length === 0) {
        return buildToolView({ name: 'plan', state: 'neutral', segments: [{ text: resultText || 'no active plan', token: 'dim' }] }, theme);
      }
      const done = steps.filter((s) => s.status === 'done').length;
      const current = steps.find((s) => s.status === 'doing') ?? steps.find((s) => s.status === 'todo');
      return buildToolView({
        name: 'plan',
        state: done === steps.length ? 'success' : 'neutral',
        segments: [
          { text: `${done}/${steps.length}`, token: 'count' },
          ...(current ? [{ text: stepLabel(current), token: 'bright' as const }] : []),
        ],
      }, theme);
    },
  });
}
