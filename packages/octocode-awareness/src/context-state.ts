import type { ContextAdvisory, ContextAdvisoryKind } from './context-regulation.js';

export interface RunState {
  basis: 'attributed-observations';
  status: 'unknown' | 'pressured' | 'stuck' | 'disrupted' | 'progressing' | 'observed';
  conditions: ContextAdvisoryKind[];
}
export interface ContextNudge {
  advisory: true;
  advisory_id: string;
  kind: ContextAdvisoryKind;
  urgency: 'attention' | 'notice';
  message: string;
  observation_ids: string[];
}
const priority: ContextAdvisoryKind[] = ['context-pressure', 'repetition', 'stalled-progress', 'tool-failure'];
const messages: Record<ContextAdvisoryKind, string> = {
  'context-pressure': 'Context is nearly full. Preserve the goal and next step before expanding it.',
  repetition: 'This approach is repeating without new evidence or progress. Try a different route.',
  'stalled-progress': 'Reported progress has stalled. Choose one verifiable next milestone.',
  'tool-failure': 'The latest tool call failed. Inspect its result before retrying.',
};

/** Interpreted run state remains independent of whether advice was dismissed. */
export function interpretRunState(fresh: boolean, advisories: readonly ContextAdvisory[], progressing = false): RunState {
  const conditions = fresh ? priority.filter(kind => advisories.some(a => a.kind === kind)) : [];
  const status = !fresh ? 'unknown' : conditions.includes('context-pressure') ? 'pressured'
    : conditions.some(kind => kind === 'repetition' || kind === 'stalled-progress') ? 'stuck'
      : conditions.includes('tool-failure') ? 'disrupted' : progressing ? 'progressing' : 'observed';
  return { basis: 'attributed-observations', status, conditions };
}

/** One short prompt-facing offer; complete advice remains available through orient. */
export function selectContextNudge(advisories: readonly ContextAdvisory[]): ContextNudge | undefined {
  const selected = priority.flatMap(kind => advisories.filter(a => a.kind === kind))[0];
  if (!selected) return undefined;
  return { advisory: true, advisory_id: selected.id, kind: selected.kind,
    urgency: selected.kind === 'tool-failure' ? 'notice' : 'attention',
    message: messages[selected.kind], observation_ids: selected.observation_ids };
}
