import { RuntimeObservationSchema, type RuntimeObservation } from './physiology.js';
function validateRuntimeObservation(value: RuntimeObservation): RuntimeObservation { return RuntimeObservationSchema.parse(value); }
type RegulationAction = 'inspect_recent_tool_failures' | 'inspect_context_headroom';
interface Regulation { advisory: true; actions: RegulationAction[] }
/** Host receipts only; no workspace queries or runtime control authority. */
export function assessRuntimeRegulation(observation: RuntimeObservation): { runtime: RuntimeObservation; regulation: Regulation } {
  const runtime = validateRuntimeObservation(observation);
  const actions: RegulationAction[] = [];
  if ((runtime.tools?.failed ?? 0) > 0) actions.push('inspect_recent_tool_failures');
  if ((runtime.context?.saturation_basis_points ?? 0) >= 9_000) actions.push('inspect_context_headroom');
  return { runtime, regulation: { advisory: true, actions } };
}
