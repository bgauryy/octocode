/**
 * plan-integration.ts — Agent profile constants, plan-mode effect resolution,
 * and same-batch cross-reference guard.
 *
 * Side effect: registers resolveAgentBatchEffect with registerAgentToolEffectResolver
 * on first import, so that the plan-mode gate is active whenever this module loads.
 *
 * Owners: AGENT_PROFILES, AGENT_OPERATIONS, typed registry mapping,
 *         resolveAgentBatchEffect, rejectCrossBatchReference.
 */
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


