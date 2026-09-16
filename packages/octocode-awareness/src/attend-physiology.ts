import { resolve } from 'node:path';
import type { AwarenessQueryRow } from './repo-model.js';

import { RuntimeObservationSchema, type RuntimeObservation } from './physiology.js';

function validateRuntimeObservation(value: RuntimeObservation): RuntimeObservation {
  const parsed = RuntimeObservationSchema.safeParse(value);
  if (!parsed.success) throw new Error('Invalid runtime observation');
  return parsed.data;
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

/** Host receipts only; no workspace queries or runtime control authority. */
export function assessRuntimeRegulation(observation: RuntimeObservation): { runtime: RuntimeObservation; regulation: Regulation } {
  const runtime = validateRuntimeObservation(observation);
  const actions: RegulationAction[] = [];
  if ((runtime.tools?.failed ?? 0) > 0) actions.push('inspect_recent_tool_failures');
  if ((runtime.context?.saturation_basis_points ?? 0) >= 9_000) actions.push('inspect_context_headroom');
  return { runtime, regulation: { advisory: true, actions } };
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
  const runtimeAssessment = input.runtimeObservation === undefined ? undefined : assessRuntimeRegulation(input.runtimeObservation);
  if (runtimeAssessment !== undefined) {
    const { runtime } = runtimeAssessment;
    operational_state.runtime = runtime;
    operational_state.unavailable = operational_state.unavailable.filter(sensor =>
      !(sensor === 'context' && runtime.context !== undefined) &&
      !(sensor === 'tool_health' && (runtime.tools?.observed ?? 0) > 0));
  }
  const actions: RegulationAction[] = [];
  if (operational_state.verification.owned_observed > 0) actions.push('verify_owned_work');
  if (operational_state.coordination.locks_observed > 0) actions.push('inspect_lock');
  if (operational_state.coordination.overlaps_observed > 0) actions.push('inspect_overlap');
  if (operational_state.evidence.reference_warnings > 0) actions.push('revalidate_memory');
  if (operational_state.coverage.omitted_rows > 0) actions.push('narrow_read');
  actions.push(...(runtimeAssessment?.regulation.actions ?? []));
  return { operational_state, regulation: { advisory: true, actions } };
}
