import type { ToolCallResult, PiContext } from '../../types.js';
import { runAskPrompt } from '../ask-user-tool.js';
import {
  PLAN_APPROVE_DESC, PLAN_APPROVE_LABEL, PLAN_APPROVAL_HEADER, PLAN_PROPOSE_HINT,
  PLAN_REJECT_DESC, PLAN_REJECT_LABEL, PLAN_RFC_REVIEW_HEADER,
} from '../../tui/content.js';
import { setManagedActivity } from '../runtime-renderer.js';
import {
  setPlan, getPlan, getPlanCoordination, setPlanRfc, getPlanRfc, addPlanDecision,
} from './plan-store.js';
import { resolveRfcPath } from './plan-rfc.js';
import { activatePlan } from './plan-executor.js';
import { proposePlanReview, requestPlanChanges } from './plan-lifecycle.js';
import {
  renderList, planPresentation, planWorkspace, configurePlanScope,
  ensureUnifiedProjection, buildRfcReviewTldr, writeCurrentPlanArtifacts,
} from './plan-presentation.js';
import {
  refreshPlanUi, startReviewedPlan, buildPlanStartAuthorizationOptionId,
} from './plan-command.js';
import { planError as errorResult, planResult } from './plan-result.js';
import type { PlanParams } from './plan-contract.js';
import type { PlanScope } from './plan-scope.js';
import type { PlanStep, StepInput } from './plan-types.js';

type AuditPlanEvent = (ctx: PiContext | undefined, scope: PlanScope, event: string, detail?: Record<string, unknown>) => void;

/** Review-worthy design boundaries. Step count alone is execution complexity, not RFC consequence. */
const CONSEQUENTIAL_PATTERNS: ReadonlyArray<{ label: string; pattern: RegExp }> = [
  { label: 'migration', pattern: /\b(?:migrat\w*|backfill\w*|schema\s+(?:change|migration))\b/i },
  { label: 'public contract', pattern: /\b(?:breaking\s+change|public[\s-]+api|published\s+contract|wire\s+protocol)\b/i },
  { label: 'security boundary', pattern: /\b(?:authentication|authorization|credentials?|secrets?|permissions?|encryption)\b/i },
  { label: 'destructive data operation', pattern: /\b(?:drop\s+(?:table|column|database)|truncate\s+table|delete\s+(?:persistent\s+)?data)\b/i },
];

/** Infer whether the steps cross a review-worthy migration, public, security, or data boundary. */
export function inferConsequential(steps: StepInput[]): { consequential: boolean; signals: string[] } {
  const text = steps.map((step) => typeof step === 'string' ? step : step?.text ?? '').join('\n');
  const signals = CONSEQUENTIAL_PATTERNS
    .filter(({ pattern }) => pattern.test(text))
    .map(({ label }) => label);
  return { consequential: signals.length > 0, signals };
}

export function resolveRfcGate(
  p: PlanParams,
  scope: PlanScope,
): { rfc?: string; hasNewRfc: boolean; error?: ToolCallResult } {
  const fail = (text: string, error = 'rfc-gate'): ToolCallResult => errorResult(text, { action: p.action, error });
  const supplied = typeof p.rfcPath === 'string' ? p.rfcPath.trim() : '';
  if (supplied) {
    const resolved = resolveRfcPath(planWorkspace(scope), supplied);
    if (resolved.error) {
      return { hasNewRfc: false, error: fail(`[PLAN] rfcPath ${supplied} did not resolve: ${resolved.error}`, 'rfc-unresolvable') };
    }
    const existing = getPlanRfc(scope);
    return { rfc: resolved.path!, hasNewRfc: resolved.path !== existing };
  }
  const existing = getPlanRfc(scope);
  if (existing) return { rfc: existing, hasNewRfc: false };
  if (p.action !== 'propose') return { hasNewRfc: false };

  const consequence = inferConsequential(Array.isArray(p.steps) ? p.steps : []);
  if (p.consequential === false && consequence.consequential && !p.reason?.trim()) {
    return {
      hasNewRfc: false,
      error: fail(
        `[PLAN] consequential:false overrides detected review boundaries (${consequence.signals.join('; ')}); provide a non-empty reason.`,
        'override-reason-required',
      ),
    };
  }
  if (!(p.consequential ?? consequence.consequential)) return { hasNewRfc: false };
  const signals = consequence.signals.join('; ') || 'explicit consequential:true';
  return {
    hasNewRfc: false,
    error: fail(
      `[PLAN] consequential proposal requires a reviewable RFC (${signals}). Create or update .octocode/rfc/<name>/RFC.md and pass rfcPath, or set consequential:false with a non-empty reason that justifies the override.`,
      'rfc-required',
    ),
  };
}

export async function executeProposal(
  p: PlanParams,
  ctx: PiContext | undefined,
  scope: PlanScope,
  auditPlanEvent: AuditPlanEvent,
): Promise<ToolCallResult> {
  let steps: PlanStep[];
  ctx?.ui?.notify?.('Creating plan…', 'info');
  setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail: 'Creating plan…' });
  const gate = resolveRfcGate(p, scope);
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
      return errorResult(`[PLAN] could not enter RFC review: ${proposed.message}`, {
        action: p.action,
        error: proposed.code,
        steps: proposed.steps,
      });
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
        return errorResult(`[PLAN] implementation did not start: ${started.message}`, {
          action: p.action,
          ...planPresentation(ctx, scope),
          error: 'start-failed',
          revision,
        });
      }
      steps = started.steps;
      const activeArtifacts = writeCurrentPlanArtifacts(ctx, scope, 'active');
      refreshPlanUi(ctx);
      const startVerdict = `[PLAN] approved and started · rev ${revision.slice(0, 8)}`;
      auditPlanEvent(ctx, scope, 'start', { revision, source: 'propose' });
      return planResult(startVerdict, {
        action: p.action,
        ...planPresentation(ctx, scope),
        verdict: startVerdict,
        revision,
        decision: 'start',
        ...(activeArtifacts ? { artifacts: activeArtifacts } : {}),
      });
    }

    if (outcome?.status === 'selected' && outcome.value === 'changes') {
      const changed = requestPlanChanges(scope);
      if (changed.ok) writeCurrentPlanArtifacts(ctx, scope, 'draft');
      refreshPlanUi(ctx);
      const changesVerdict = '[PLAN] changes requested — revise the RFC and re-propose.';
      auditPlanEvent(ctx, scope, 'changes', { revision, feedbackProvided: false });
      return planResult(changesVerdict, {
        action: p.action,
        ...planPresentation(ctx, scope),
        verdict: changesVerdict,
        revision,
        decision: outcome.status,
        ...(artifacts ? { artifacts } : {}),
      });
    }

    if (outcome?.status === 'text' && outcome.value) {
      const feedback = outcome.value.trim();
      const changed = requestPlanChanges(scope);
      if (changed.ok) writeCurrentPlanArtifacts(ctx, scope, 'draft');
      refreshPlanUi(ctx);
      if (feedback) addPlanDecision(scope, 'Requested plan changes', feedback);
      const textVerdict = `[PLAN] changes requested: ${feedback}\nRevise the RFC and re-propose.`;
      auditPlanEvent(ctx, scope, 'changes', { revision, feedbackProvided: Boolean(feedback) });
      return planResult(textVerdict, {
        action: p.action,
        ...planPresentation(ctx, scope),
        verdict: textVerdict,
        revision,
        decision: outcome.status,
        ...(artifacts ? { artifacts } : {}),
      });
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
    return planResult(showFullContext ? `${pendingOrUnavailableVerdict}\n${summary}` : pendingOrUnavailableVerdict, {
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
    });
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
  return planResult(isNonInteractivePropose ? `${proposeVerdict}\n${renderList(steps)}` : proposeVerdict, {
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
  });
}

