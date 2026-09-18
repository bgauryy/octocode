/**
 * Agent spawn policy: step-budget circuit-breaker, spawn-policy evaluation,
 * and worker model/provider validation.
 */
import path from 'node:path';
import { isForbiddenWorkerTool } from '../../contracts/capabilities.js';
import type { SpawnPolicy, SpawnPolicyResult, PiContext } from '../../types.js';
import { assertWorktreeSpawnAllowed } from '../worktree.js';
import {
  type SpawnAgentParams,
  MAX_ACTIVE_AGENTS,
  DEFAULT_SPAWN_POLICY,
} from './types.js';

const SPAWN_POLICY_MAX_ACTIVE_ENV = 'OCTOCODE_AGENT_MAX_ACTIVE';
const SPAWN_POLICY_WARNING_ACTIVE_ENV = 'OCTOCODE_AGENT_WARNING_ACTIVE';
const SPAWN_POLICY_MAX_STEPS_ENV = 'OCTOCODE_AGENT_MAX_STEPS';

function readPositiveIntegerEnv(name: string): number | undefined {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === '') return undefined;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0) return undefined;
  return value;
}

export function resolveSpawnPolicy(policy: SpawnPolicy): SpawnPolicy {
  const maxActiveAgents = Math.min(
    readPositiveIntegerEnv(SPAWN_POLICY_MAX_ACTIVE_ENV) ?? policy.maxActiveAgents,
    MAX_ACTIVE_AGENTS,
  );
  const warningActiveAgents = readPositiveIntegerEnv(SPAWN_POLICY_WARNING_ACTIVE_ENV) ?? policy.warningActiveAgents;
  const maxStepsPerWorker = readPositiveIntegerEnv(SPAWN_POLICY_MAX_STEPS_ENV) ?? policy.maxStepsPerWorker;
  return {
    ...policy,
    maxActiveAgents,
    warningActiveAgents: Math.min(warningActiveAgents, maxActiveAgents),
    maxStepsPerWorker,
  };
}

/**
 * Per-worker step (tool-call) circuit-breaker signal. Returns a warning once a worker's
 * completed tool calls reach the budget so the parent can abort/steer a runaway worker.
 */
export function evaluateStepBudget(
  steps: number,
  maxSteps: number = DEFAULT_SPAWN_POLICY.maxStepsPerWorker,
): { exceeded: boolean; warning?: string } {
  if (!Number.isFinite(maxSteps) || maxSteps <= 0) return { exceeded: false };
  if (steps >= maxSteps) {
    return { exceeded: true, warning: `Worker exceeded step budget (${steps}/${maxSteps} tool calls) \u2014 consider abort/steer; a runaway worker burns tokens.` };
  }
  return { exceeded: false };
}

function looksLikeProviderScopedModel(model: string): boolean {
  return /\//.test(model)
    || /^(?:claude|gpt|llama|mistral|gemini|qwen|zai|deepseek|kimi|codestral)[-_:/.]/i.test(model);
}

export function getWorkerTools(params: SpawnAgentParams): string[] {
  return (params.tools ?? []).filter((toolName) => !isForbiddenWorkerTool(toolName));
}

/**
 * A packet section counts as present only when it anchors a line as a label —
 * e.g. "Goal:", "- Scope:", "## Ownership —", "**Acceptance:**" — optionally
 * preceded by a bullet/heading marker and wrapped in bold. A bare mention
 * inside prose ("there is no clear goal here") does not count: the packet
 * policy exists to catch genuinely unstructured worker briefs, not to be
 * satisfied by incidentally using the right words.
 */
function missingStructuredSections(text: string, sections: string[]): string[] {
  return sections.filter((section) => {
    const label = new RegExp(String.raw`^[ \t]*(?:[-*#>]+[ \t]*)*\**${section}\**[ \t]*[:\u2014-]`, 'im');
    return !label.test(text);
  });
}

export function buildInitialPrompt(params: SpawnAgentParams): string {
  const task = String(params.task ?? '').trim();
  const context = String(params.context ?? '').trim();
  if (!context) return task;
  return `Context for this delegated agent:\n\n${context}\n\nTask:\n\n${task}`;
}

export function resolveWorkerModelParams(params: SpawnAgentParams, ctx?: PiContext): SpawnAgentParams {
  const explicitModel = typeof params.model === 'string' && params.model.trim().length > 0;
  const parentModel = ctx?.model;
  return {
    ...params,
    model: explicitModel ? params.model : parentModel?.id,
    provider: params.provider ?? (!explicitModel || params.model === parentModel?.id ? parentModel?.provider : undefined),
  };
}

export function validateWorkerModelParams(params: SpawnAgentParams, ctx?: PiContext): void {
  const model = String(params.model ?? '').trim();
  const provider = String(params.provider ?? '').trim();
  if (!model) return;
  if (!provider && looksLikeProviderScopedModel(model)) {
    throw new Error(`agent spawn model "${model}" requires an explicit provider from \`pi -ne --list-models\`.`);
  }
  if (provider && ctx?.modelRegistry?.find && !ctx.modelRegistry.find(provider, model)) {
    throw new Error(`agent spawn model/provider not found in the active Pi model registry: ${provider}/${model}. Choose a valid pair from \`pi -ne --list-models\`.`);
  }
}

export function evaluateSpawnPolicy(
  params: SpawnAgentParams,
  activeCount: number = 0,
  policy: SpawnPolicy = DEFAULT_SPAWN_POLICY,
): SpawnPolicyResult {
  const effectivePolicy = resolveSpawnPolicy(policy);
  const warnings: string[] = [];
  if (params.isolation === 'worktree') {
    try {
      assertWorktreeSpawnAllowed(path.resolve(String(params.cwd ?? process.cwd())));
    } catch (error) {
      return {
        allowed: false,
        warnings,
        reason: error instanceof Error ? error.message : String(error),
      };
    }
  }
  if (activeCount >= effectivePolicy.maxActiveAgents) {
    return {
      allowed: false,
      warnings,
      reason: `Agent registry at capacity: ${activeCount}/${effectivePolicy.maxActiveAgents} active agents.`,
    };
  }
  if (activeCount >= effectivePolicy.warningActiveAgents) {
    warnings.push(`High worker fan-out: ${activeCount}/${effectivePolicy.maxActiveAgents} active agents already exist.`);
  }
  const task = buildInitialPrompt(params);
  const missingSections = missingStructuredSections(task, effectivePolicy.requiredPacketSections);
  if (missingSections.length > 0) {
    warnings.push(`Worker packet is missing recommended section(s): ${missingSections.join(', ')}.`);
  }
  const model = String(params.model ?? '');
  if (model && looksLikeProviderScopedModel(model) && !params.provider) {
    warnings.push('Model looks provider-scoped or custom-provider-hosted; pass provider from `pi -ne --list-models` when required.');
  }
  const strippedTools = (params.tools ?? []).filter(isForbiddenWorkerTool);
  if (strippedTools.length > 0) {
    warnings.push(`Recursive worker tool(s) stripped: ${strippedTools.join(', ')}.`);
  }
  return { allowed: true, warnings };
}
