import { assessRuntimeRegulation } from '@octocodeai/octocode-awareness/host';
import type { PiRuntimeObservation } from '@octocodeai/agent-contracts/physiology';

/** Advisory projection only. Pi owns compaction, retries, and provider execution. */
export function createPiPhysiologyAdvisory(): (observation: PiRuntimeObservation | undefined) => { content: string; commit(): void } {
  let session = '';
  let offered = new Set<string>();
  let revision = 0;
  return observation => {
    if (!observation) return { content: '', commit() {} };
    const { runtime, regulation } = assessRuntimeRegulation(observation);
    const identity = JSON.stringify([observation.session.session_id, observation.session.generation]);
    const preparedRevision = revision;
    const nextOffered = identity === session ? new Set(offered) : new Set<string>();
    // Missing samples cannot establish recovery or reset a delivered warning.
    if (runtime.tools && runtime.tools.observed > 0 && !regulation.actions.includes('inspect_recent_tool_failures'))
      nextOffered.delete('inspect_recent_tool_failures');
    if (runtime.context?.saturation_basis_points !== undefined && !regulation.actions.includes('inspect_context_headroom'))
      nextOffered.delete('inspect_context_headroom');
    const actions = regulation.actions.filter(action => !nextOffered.has(action));
    actions.forEach(action => nextOffered.add(action));
    return {
      content: actions.length ? `Runtime observation (advisory): ${actions.join(', ')}. Pi owns compaction and retries; inspect context.orient for attributed evidence before choosing the next action.` : '',
      commit() {
        // Failed prompt assembly must not consume advice. Older prepared turns
        // also cannot overwrite a newer committed session or recovery sample.
        if (revision !== preparedRevision) return;
        session = identity;
        offered = nextOffered;
        revision++;
      },
    };
  };
}
