/**
 * plan-registration — tool schema, execute handler, and registration.
 * Owns the action dispatcher, tool schema, and registration.
 * Imports from planning module split; does not import from active-plan or plan-tool.
 */

import path from 'node:path';
import type { ToolDefinition, ToolCallResult, PiContext, PiTheme, RenderResultOptions } from '../../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from '../octocode-tools.js';
import { CLI_STATUS_TEXT } from '../../tui/cli-design.js';
import { runAskPrompt, type AskOutcome } from '../ask-user-tool.js';
import { planArtifactsDir } from '../plan-html.js';
import { buildQueryCallBlocks, buildToolView } from '../render-helpers.js';
import { setManagedActivity } from '../runtime-renderer.js';
import {
  completeExternalPlanTask,
  finalizeExternalPlan,
} from '@octocodeai/octocode-awareness/host';
import { getAwarenessAgentId } from '../awareness-shared.js';
import { assertPersistentAwarenessEnabled } from '../storage-policy.js';
import { executeQueryBatch, toToolSchema } from '../query-envelope.js';
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
  setPlanRfc,
  addPlanDecision,
  getPlanDecisions,
  getPlanPersistenceFailure,
} from './plan-store.js';
import { addStep, startStep, restorePlanSteps, completeStep, removeStep } from './plan-executor.js';
import {
  renderList,
  planPresentation,
  planWorkspace,
  configurePlanScope,
  requestedPlanScope,
  ensureUnifiedProjection,
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
import { stepLabel, depsMet, type PlanStep } from './plan-types.js';
import { planError as errorResult, planResult, withPlanPersistenceWarning } from './plan-result.js';
import { preflightPlanQuery, type PlanParams } from './plan-contract.js';
import { executeProposal, resolveRfcGate } from './plan-proposal.js';
import type { PlanScope } from './plan-scope.js';

export { inferConsequential } from './plan-proposal.js';

// ─── Constants ───────────────────────────────────────────────────────────────

/** Cap on questions per clarify call — a bounded interview, not an interrogation. */
const MAX_CLARIFY = 3;

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

// ─── Exported pure helpers ──────────────────────────────────────────────────

// ─── Private execute helpers ──────────────────────────────────────────────────

function auditPlanEvent(
  ctx: PiContext | undefined,
  scope: PlanScope,
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

async function executeClarify(
  p: PlanParams,
  ctx: PiContext | undefined,
  scope: ReturnType<typeof activePlanScope>,
): Promise<ToolCallResult> {
  const result = (text: string, isError = false, extraDetails: Record<string, unknown> = {}): ToolCallResult => {
    const details = { action: 'clarify', decisions: getPlanDecisions(scope), ...extraDetails };
    return isError ? errorResult(text, details) : planResult(text, details);
  };
  const questions = (Array.isArray(p.questions) ? p.questions : [])
    .filter((question) => question && String(question.prompt ?? '').trim())
    .slice(0, MAX_CLARIFY);
  if (!questions.length) return result('[PLAN] clarify needs a questions[] list (≤3 high-impact questions the repo cannot answer). Skip clarify for obvious work.', true);
  if (!ctx) return result(`[PLAN] this host cannot prompt — ask these inline and continue:\n${questions.map((question, index) => `${index + 1}. ${question.prompt}`).join('\n')}`);

  const recorded: string[] = [];
  let halted: string | undefined;
  let pendingInteraction: AskOutcome['interaction'] | undefined;
  for (let index = 0; index < questions.length; index++) {
    const question = questions[index]!;
    const prompt = String(question.prompt).trim();
    const outcome = await runAskPrompt(ctx, {
      question: prompt,
      options: (question.options ?? []).map((option) => ({
        value: option.value ?? option.label,
        label: option.label,
        description: option.description,
        recommended: option.recommended,
        pros: option.pros,
        cons: option.cons,
      })),
      pagination: { current: index + 1, total: questions.length },
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
      continue;
    }
    if (outcome.status !== 'text' && outcome.status !== 'selected') {
      halted = `[PLAN] clarify ${outcome.status}.`;
      break;
    }
    const answer = outcome.status === 'text' ? (outcome.value ?? '').trim() : String(outcome.label ?? outcome.value ?? '');
    if (!answer) continue;
    addPlanDecision(scope, prompt, answer);
    recorded.push(prompt);
  }
  if (recorded.length) auditPlanEvent(ctx, scope, 'clarify', { decisionsRecorded: recorded.length });
  const settle = (detail: string): void => {
    if (getPlanReviewState(scope).phase === 'abandoned') setPlanLifecycle(scope, 'researching');
    if (getPlanReviewState(scope).phase !== 'draft') setPlanLifecycle(scope, 'draft');
    setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail });
  };
  if (halted) {
    if (!pendingInteraction) {
      settle('decision-cancelled');
      return result(halted);
    }
    return result(halted, false, {
      pendingInteraction: {
        version: pendingInteraction.version,
        interactionId: pendingInteraction.interactionId,
        correlationId: pendingInteraction.correlationId,
        sessionId: pendingInteraction.sessionId,
      },
      continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
    });
  }
  settle('decision-complete');
  return result(recorded.length
    ? `[PLAN] recorded ${recorded.length} decision(s) · decision-complete. Proceed to propose.`
    : '[PLAN] no new decisions recorded · decision-complete.');
}

/** Execute one normalized plan action. */
async function executePlanQuery(p: PlanParams, ctx: PiContext | undefined): Promise<ToolCallResult> {
  const scope = activePlanScope(ctx);
  let steps: PlanStep[];
  // Reject unavailable shared writes before changing the local plan or its scope.
  if (p.action !== 'show' && p.action !== 'clarify'
    && (requestedPlanScope(scope, p.scope) === 'shared' || getPlanCoordination(scope).awarenessPlanId)) {
    assertPersistentAwarenessEnabled();
  }

  if (p.action === 'clarify') return executeClarify(p, ctx, scope);


  switch (p.action) {
    case 'set': {
      const gate = resolveRfcGate(p, scope);
      if (gate.error) return gate.error;
      steps = setPlan(scope, Array.isArray(p.steps) ? p.steps : []);
      configurePlanScope(scope, p.scope);
      if (gate.hasNewRfc) setPlanRfc(scope, gate.rfc);
      ensureUnifiedProjection(scope, p.scope, ctx);
      steps = getPlan(scope);
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    }
    case 'propose':
      return executeProposal(p, ctx, scope, auditPlanEvent);
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
      const planError = (msg: string, error: string): ToolCallResult =>
        errorResult(`${msg}\n${renderList(current)}`, { action: p.action, ...planPresentation(ctx, scope), error });
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
        return planResult(`[PLAN] approved and started · rev ${revision.slice(0, 8)}\n${renderList(steps)}`, {
          action: p.action,
          ...planPresentation(ctx, scope),
          revision,
          decision: 'start',
        });
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
        return errorResult(
          '[PLAN] mapped shared plans cannot be cleared while work is unfinished; complete or abandon the shared work first.',
          { action: p.action, error: 'shared-clear', ...planPresentation(ctx, scope) },
        );
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
  return planResult(`${header}\n${renderList(steps)}${taskIds}${artifactHint}`, {
    action: p.action,
    ...planPresentation(ctx, scope),
  });
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
      const reasoning = z.string().max(400).optional();
      const scope = z.enum(['auto','session','shared']).optional();
      const step = z.union([
        z.string().min(1),
        z.strictObject({
          text: z.string().min(1),
          acceptance: z.string().optional(),
          activeForm: z.string().optional(),
          checkCommand: z.string().optional(),
          dependsOn: z.array(z.number().int().min(1)).optional(),
          paths: z.array(z.string()).optional(),
          reasoning: z.string().optional(),
        }),
      ]);
      const receipt = z.strictObject({
        command: z.string().min(1),
        status: z.enum(['SUCCESS','FAILED']),
        message: z.string().min(1),
      });
      const questions = z.array(z.strictObject({
        prompt: z.string().min(1),
        options: z.array(z.strictObject({
          label: z.string().min(1), value: z.string().optional(), description: z.string().optional(),
          recommended: z.boolean().optional(), pros: z.array(z.string()).optional(), cons: z.array(z.string()).optional(),
        })).optional(),
      })).min(1).max(3);
      const query = z.union([
        z.strictObject({ reasoning, action: z.enum(['set']), scope, steps: z.array(step).min(1), consequential: z.boolean().optional(), reason: z.string().min(1).optional(), rfcPath: z.string().optional() }),
        z.strictObject({ reasoning, action: z.enum(['propose']), scope, steps: z.array(step).min(1), consequential: z.boolean().optional(), reason: z.string().min(1).optional(), rfcPath: z.string().optional() }),
        z.strictObject({ reasoning, action: z.enum(['clarify']), questions }),
        z.strictObject({ reasoning, action: z.enum(['add']), scope, text: z.string().min(1), activeForm: z.string().optional(), dependsOn: z.array(z.number().int().min(1)).optional(), paths: z.array(z.string()).optional(), taskReasoning: z.string().optional(), acceptance: z.string().optional(), checkCommand: z.string().optional() }),
        z.strictObject({
          reasoning,
          action: z.enum(['start']),
          scope,
          index: z.number().int().min(1).optional(),
        }),
        z.strictObject({
          reasoning,
          action: z.enum(['start']),
          scope,
          revision: z.string().min(1),
          authorizationInteractionId: z.string().min(1).optional(),
        }),
        z.strictObject({ reasoning, action: z.enum(['complete']), scope, index: z.number().int().min(1).optional(), receipt: receipt.optional() }),
        z.strictObject({ reasoning, action: z.enum(['remove']), scope, index: z.number().int().min(1).optional() }),
        z.strictObject({ reasoning, action: z.enum(['clear']), scope }),
        z.strictObject({ reasoning, action: z.enum(['show']), scope }),
      ]);
      return toToolSchema(z.strictObject({
        queries: z.array(query).min(1).max(25).describe('Transitions execute sequentially. Put interactive clarify/propose actions in their own call because they can pause for durable user input.'),
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
        preflight: preflightPlanQuery,
        async execute(query, _queryIndex, _itemId, _sig, _upd, queryCtx) {
          const scope = activePlanScope(queryCtx);
          const result = await executePlanQuery(query as PlanParams, queryCtx);
          return withPlanPersistenceWarning(result, getPlanPersistenceFailure(scope));
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
