/**
 * plan-integration.ts — Agent profile constants, plan-mode effect resolution,
 * same-batch cross-reference guard, and plan-assignment dispatch.
 *
 * Side effect: registers resolveAgentBatchEffect with registerAgentToolEffectResolver
 * on first import, so that the plan-mode gate is active whenever this module loads.
 *
 * Owners: AGENT_PROFILES, AGENT_OPERATIONS, typed registry mapping,
 *         resolveAgentBatchEffect, rejectCrossBatchReference, resolvePlanAssignment.
 */

import type { PiContext } from '../../types.js';
import {
  SUBAGENT_REGISTRY,
  type SubagentConfig,
  type SubagentName,
} from '../../subagents.js';
import {
  getToolEffect,
  registerAgentToolEffectResolver,
  type ToolEffect,
} from '../plan-mode.js';
import { activePlanScope } from '../planning/plan-store.js';
import { getCurrentPlanReadModel } from '../plan-read-model.js';
import { findLivePlanWorker } from './ledger.js';
import type { QueryRecord } from '../query-envelope.js';

// ─── Profile & operation constants ────────────────────────────────────────────

/** Typed profiles backed by SUBAGENT_REGISTRY. */
const TYPED_REGISTRY_PROFILES = ['researcher', 'planner', 'architect', 'implementer', 'reviewer'] as const;

/** All public profiles exposed on the `agent` tool. */
export const AGENT_PROFILES = [
  ...TYPED_REGISTRY_PROFILES,
  'browser',
  'custom',
] as const;
export type AgentProfile = (typeof AGENT_PROFILES)[number];

/** All operations exposed on the `agent` tool. */
export const AGENT_OPERATIONS = [
  'spawn',
  'inspect',
  'configure',
  'wait',
  'message',
  'steer',
  'abort',
  'kill',
] as const;
export type AgentOperation = (typeof AGENT_OPERATIONS)[number];

// ─── Typed-profile → subagent-registry mapping ───────────────────────────────

export const PROFILE_TO_SUBAGENT: Record<
  (typeof TYPED_REGISTRY_PROFILES)[number],
  SubagentName
> = {
  researcher: 'researcher',
  planner: 'planner',
  architect: 'architect',
  implementer: 'implementer',
  reviewer: 'reviewer',
};

const DYNAMIC_CHILD_TOOLS = new Set([
  'agent',
  'calltool',
  'mcptool',
  'skill',
]);

function isQueryRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function resolveTypedProfileEffect(profile: (typeof TYPED_REGISTRY_PROFILES)[number]): ToolEffect | undefined {
  const config = SUBAGENT_REGISTRY[PROFILE_TO_SUBAGENT[profile]] as SubagentConfig | undefined;
  if (!config || !Array.isArray(config.tools)) return undefined;

  let resolved: ToolEffect = 'coordination-write';
  for (const rawName of config.tools) {
    if (typeof rawName !== 'string' || !rawName.trim()) return undefined;
    const name = rawName.trim();
    const effect = DYNAMIC_CHILD_TOOLS.has(name.toLowerCase())
      ? 'external-effect'
      : getToolEffect(name);
    if (!effect) return undefined;
    if (effect === 'external-effect') return effect;
    if (effect === 'workspace-write') resolved = effect;
  }
  return resolved;
}

/** Resolve the effect of the complete ordered agent batch before any item runs. */
export function resolveAgentBatchEffect(input?: Record<string, unknown>): ToolEffect | undefined {
  const values = input?.['queries'];
  if (!Array.isArray(values) || values.length === 0) return undefined;

  let resolved: ToolEffect = 'coordination-write';
  for (const value of values) {
    if (!isQueryRecord(value)) return undefined;
    const operation = typeof value['type'] === 'string' ? value['type'] : undefined;
    if (!operation || !(AGENT_OPERATIONS as readonly string[]).includes(operation)) return undefined;
    if (operation !== 'spawn') continue;

    if (!String(value['task'] ?? value['goal'] ?? '').trim()) return undefined;
    const profile = typeof value['profile'] === 'string' ? value['profile'] : undefined;
    if (!profile) return undefined;
    if (!(AGENT_PROFILES as readonly string[]).includes(profile)) return undefined;
    if (profile === 'custom' || profile === 'browser') return 'external-effect';

    const profileEffect = resolveTypedProfileEffect(profile as (typeof TYPED_REGISTRY_PROFILES)[number]);
    if (!profileEffect) return undefined;
    if (profileEffect === 'external-effect') return profileEffect;
    if (profileEffect === 'workspace-write') resolved = profileEffect;
  }
  return resolved;
}

// Side effect: register at import time so plan-mode gate is active immediately.
registerAgentToolEffectResolver(resolveAgentBatchEffect);

// ─── Same-batch cross-reference guard ────────────────────────────────────────

/**
 * Reject a batch that mixes `spawn` (which generates new IDs at runtime) with
 * lifecycle queries that supply an explicit `agentId`, because the caller cannot
 * know a freshly generated ID before the call completes.
 *
 * Multiple independent spawns are allowed. Multiple lifecycle ops on
 * pre-existing agents are allowed. Only the ambiguous mixed case is blocked.
 */
export function rejectCrossBatchReference(queries: QueryRecord[]): void {
  const hasSpawn = queries.some((q) => (q['type'] as string) === 'spawn');
  if (!hasSpawn) return;

  const lifecycleWithId = queries.filter(
    (q) =>
      (q['type'] as string) !== 'spawn' &&
      typeof q['agentId'] === 'string' &&
      (q['agentId'] as string).trim().length > 0,
  );

  if (lifecycleWithId.length > 0) {
    const ops = lifecycleWithId.map((q) => q['type'] as string).join(', ');
    throw new Error(
      `Same-batch cross-reference rejected: a batch containing spawn cannot also ` +
      `reference an agentId in lifecycle queries (${ops}). ` +
      `Spawn workers in one call, then use the returned agentId in a subsequent call.`,
    );
  }
}

// ─── Plan assignment dispatch ────────────────────────────────────────────────

function validatePlanAssignment(
  planStep: string,
  scope: string,
  plan: ReturnType<typeof getCurrentPlanReadModel>,
  expectedPlanId?: string,
) {
  if (expectedPlanId && plan.planId !== expectedPlanId) throw new Error('Parent plan changed during worker preparation; inspect the current plan before delegating.');
  if (plan.phase !== 'executing') throw new Error('planStep was provided, which explicitly requests a plan assignment, but the parent plan is not executing. Standalone subagents need no plan: omit planStep.');
  if (plan.pendingInteractionIds.length > 0) throw new Error('Resolve pending plan interactions before delegating a task.');
  const task = plan.tasks.find((candidate) => candidate.id === planStep);
  if (!task) throw new Error(`planStep must be an existing stable task id in the current plan: ${planStep}`);
  if (task.status !== 'doing') throw new Error(`Plan task ${planStep} must be doing before delegation; start its runnable step first.`);
  if (!task.dependsOn.every((index) => plan.tasks.find((candidate) => candidate.index === index)?.status === 'done')) {
    throw new Error(`Plan task ${planStep} has unfinished dependencies.`);
  }
  const owner = findLivePlanWorker(scope, plan.planId, task.id);
  if (owner) throw new Error(`Plan task ${planStep} already has live worker ${owner}; collect or continue that worker before assigning another.`);
  return { scope, planId: plan.planId, task };
}

export function resolvePlanAssignment(planStep: string, ctx?: PiContext, expectedPlanId?: string) {
  const scope = activePlanScope(ctx);
  return validatePlanAssignment(planStep, scope, getCurrentPlanReadModel(ctx, scope), expectedPlanId);
}

/**
 * Treat planStep as an optional-model-field mistake only when no parent plan
 * task exists for it to name. Any existing plan keeps the strict assignment path.
 */
export function resolveOptionalPlanAssignment(planStep: string | undefined, ctx?: PiContext) {
  if (!planStep) return { planStep: undefined, assignment: undefined, ignoredPlanStep: false };
  const scope = activePlanScope(ctx);
  const plan = getCurrentPlanReadModel(ctx, scope);
  if (plan.phase !== 'executing' && plan.tasks.length === 0) {
    return { planStep: undefined, assignment: undefined, ignoredPlanStep: true };
  }
  return {
    planStep,
    assignment: validatePlanAssignment(planStep, scope, plan),
    ignoredPlanStep: false,
  };
}
