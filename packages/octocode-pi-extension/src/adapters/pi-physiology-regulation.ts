import { assessRuntimeRegulation } from '@octocodeai/octocode-awareness/host';
import type { PiRuntimeObservation } from '@octocodeai/agent-contracts/physiology';

/** Advisory projection only. Pi owns compaction, retries, and provider execution. */
export function createPiPhysiologyAdvisory(): (observation: PiRuntimeObservation | undefined) => string {
  let session = '';
  const offered = new Set<string>();
  return observation => {
    if (!observation) return '';
    const { runtime, regulation } = assessRuntimeRegulation(observation);
    const identity = JSON.stringify([observation.session.session_id, observation.session.generation]);
    if (identity !== session) { session = identity; offered.clear(); }
    // Missing samples cannot establish recovery or reset a delivered warning.
    if (runtime.tools && runtime.tools.observed > 0 && !regulation.actions.includes('inspect_recent_tool_failures'))
      offered.delete('inspect_recent_tool_failures');
    if (runtime.context?.saturation_basis_points !== undefined && !regulation.actions.includes('inspect_context_headroom'))
      offered.delete('inspect_context_headroom');
    const actions = regulation.actions.filter(action => !offered.has(action));
    actions.forEach(action => offered.add(action));
    return actions.length ? `Runtime observation (advisory): ${actions.join(', ')}. Pi owns compaction and retries; inspect the available evidence before choosing the next action.` : '';
  };
}
