import { resolve } from 'node:path';
import type { AwarenessQueryRow } from './repo-model.js';

/** Redacted, session-local receipts supplied by a host; never accepted from model tool arguments. */
export interface RuntimeObservation {
  schema_version: 1;
  source: 'native_runtime';
  context?: {
    current_tokens: number;
    measured_at: number;
    input_limit_tokens?: number;
    remaining_input_tokens?: number;
    saturation_basis_points?: number;
  };
  tools: { window: 32; observed: number; failed: number; cancelled: number; blocked: number };
  controls: {
    owner: 'agent_core';
    compactions_committed: number;
    compactions_failed: number;
    retries_scheduled: number;
    provider_attempt?: number;
    provider_max_attempts?: number;
  };
}

function validateRuntimeObservation(value: RuntimeObservation): RuntimeObservation {
  const count = (number: number): boolean => Number.isSafeInteger(number) && number >= 0;
  const { context, tools, controls } = value;
  const validContext = context === undefined || (() => {
    if (!count(context.current_tokens) || !count(context.measured_at)) return false;
    const hasInputLimit = context.input_limit_tokens !== undefined;
    if (hasInputLimit !== (context.remaining_input_tokens !== undefined) ||
      hasInputLimit !== (context.saturation_basis_points !== undefined)) return false;
    if (!hasInputLimit) return true;
    const inputLimit = context.input_limit_tokens!;
    const remaining = context.remaining_input_tokens!;
    const saturation = context.saturation_basis_points!;
    return count(inputLimit) && inputLimit > 0 && count(remaining) &&
      remaining === Math.max(0, inputLimit - context.current_tokens) && count(saturation) &&
      saturation <= 10_000 &&
      saturation === Math.min(10_000, Math.floor((context.current_tokens / inputLimit) * 10_000));
  })();
  if (value.schema_version !== 1 || value.source !== 'native_runtime' || !tools || !controls ||
    tools.window !== 32 || ![tools.observed, tools.failed, tools.cancelled, tools.blocked,
      controls.compactions_committed, controls.compactions_failed, controls.retries_scheduled].every(count) ||
    tools.observed > tools.window || tools.failed + tools.cancelled + tools.blocked > tools.observed ||
    controls.owner !== 'agent_core' || !validContext ||
    ((controls.provider_attempt === undefined) !== (controls.provider_max_attempts === undefined)) ||
    (controls.provider_attempt !== undefined && (!count(controls.provider_attempt) || controls.provider_attempt < 1 ||
      !count(controls.provider_max_attempts!) || controls.provider_attempt > controls.provider_max_attempts!))) {
    throw new Error('Invalid runtime observation');
  }
  // Copy only the declared fields: telemetry cannot smuggle prompt/tool content into attend.
  return { schema_version: 1, source: 'native_runtime',
    ...(context === undefined ? {} : { context: {
      current_tokens: context.current_tokens, measured_at: context.measured_at,
      ...(context.input_limit_tokens === undefined ? {} : {
        input_limit_tokens: context.input_limit_tokens,
        remaining_input_tokens: context.remaining_input_tokens!,
        saturation_basis_points: context.saturation_basis_points!,
      }),
    } }),
    tools: { window: 32, observed: tools.observed, failed: tools.failed, cancelled: tools.cancelled, blocked: tools.blocked },
    controls: { owner: 'agent_core', compactions_committed: controls.compactions_committed,
      compactions_failed: controls.compactions_failed, retries_scheduled: controls.retries_scheduled,
      ...(controls.provider_attempt === undefined ? {} : { provider_attempt: controls.provider_attempt, provider_max_attempts: controls.provider_max_attempts! }) } };
}

/** Observations from one bounded read, never a model or runtime health score. */
export interface OperationalState {
  scope: 'workspace_observation';
  verification: { owned_observed: number; total: number };
  coordination: { overlaps_observed: number; locks_observed: number };
  evidence: { recalled: number; reference_warnings: number };
  coverage: { bounded: true; omitted_rows: number };
  unavailable: readonly string[];
  runtime?: RuntimeObservation;
}

export type RegulationAction = 'verify_owned_work' | 'inspect_lock' | 'inspect_overlap' | 'revalidate_memory' | 'narrow_read' | 'inspect_recent_tool_failures' | 'inspect_context_headroom';
export interface Regulation {
  advisory: true;
  actions: RegulationAction[];
}

export function scopedWorkRows(workboard: Record<string, AwarenessQueryRow[]>, workspacePath: string, files: string[]): AwarenessQueryRow[] {
  const paths = new Set(files.map(file => resolve(workspacePath, file)));
  return (workboard['FilesUnderWork'] ?? []).filter(row => {
    const path = String(row['path'] ?? row['file_path'] ?? '');
    return path !== '' && paths.has(resolve(workspacePath, path));
  });
}

export function assessOperationalState(input: {
  workboard: Record<string, AwarenessQueryRow[]>;
  agentId: string;
  workspacePath: string;
  files: string[];
  recalled: number;
  referenceWarnings: number;
  runtimeObservation?: RuntimeObservation;
}): { operational_state: OperationalState; regulation: Regulation } {
  const verify = input.workboard['Verify'] ?? [];
  const scopedFiles = scopedWorkRows(input.workboard, input.workspacePath, input.files);
  const operational_state: OperationalState = {
    scope: 'workspace_observation',
    verification: {
      owned_observed: input.agentId ? verify.filter(row => String(row['agent_id'] ?? '') === input.agentId).length : 0,
      total: Number(verify[0]?.['column_total'] ?? verify.length),
    },
    coordination: {
      overlaps_observed: input.agentId ? scopedFiles.filter(row =>
        Array.isArray(row['agents']) && row['agents'].some(agent => String(agent) !== input.agentId)).length : 0,
      locks_observed: scopedFiles.filter(row => row['locked'] === true && String(row['lock_agent'] ?? '') !== input.agentId).length,
    },
    evidence: { recalled: input.recalled, reference_warnings: input.referenceWarnings },
    coverage: {
      bounded: true,
      omitted_rows: Object.values(input.workboard).reduce((sum, rows) =>
        sum + Math.max(0, Number(rows[0]?.['column_total'] ?? rows.length) - rows.length), 0),
    },
    unavailable: ['context', 'budget', 'tool_health', 'repetition', 'uncertainty', 'reversibility', 'branch_divergence', 'yield'],
  };
  if (input.runtimeObservation !== undefined) {
    const runtime = validateRuntimeObservation(input.runtimeObservation);
    operational_state.runtime = runtime;
    operational_state.unavailable = operational_state.unavailable.filter(sensor =>
      !(sensor === 'context' && runtime.context !== undefined) &&
      !(sensor === 'tool_health' && runtime.tools.observed > 0));
  }
  const actions: RegulationAction[] = [];
  if (operational_state.verification.owned_observed > 0) actions.push('verify_owned_work');
  if (operational_state.coordination.locks_observed > 0) actions.push('inspect_lock');
  if (operational_state.coordination.overlaps_observed > 0) actions.push('inspect_overlap');
  if (operational_state.evidence.reference_warnings > 0) actions.push('revalidate_memory');
  if (operational_state.coverage.omitted_rows > 0) actions.push('narrow_read');
  if ((operational_state.runtime?.tools.failed ?? 0) > 0) actions.push('inspect_recent_tool_failures');
  if ((operational_state.runtime?.context?.saturation_basis_points ?? 0) >= 9_000)
    actions.push('inspect_context_headroom');
  return { operational_state, regulation: { advisory: true, actions } };
}
