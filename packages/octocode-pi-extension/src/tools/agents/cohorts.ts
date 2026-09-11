import { effectiveAgentStatus } from './display-state.js';

export interface AgentCohortSummary {
  id: string;
  total: number;
  running: number;
  done: number;
  blocked: number;
  failed: number;
  attention: boolean;
}

/** Bounded, redacted aggregation for fleet views and inspect results. */
export function summarizeAgentCohorts(
  records: ReadonlyArray<{
    id: string;
    cohortId?: string;
    status?: string;
    normalizedStatus?: string;
    pendingMessages?: number;
  }>,
  limit = 12,
): AgentCohortSummary[] {
  const cohorts = new Map<string, AgentCohortSummary>();
  for (const record of records) {
    const id = record.cohortId?.trim() || 'ungrouped';
    const cohort = cohorts.get(id) ?? { id, total: 0, running: 0, done: 0, blocked: 0, failed: 0, attention: false };
    const state = effectiveAgentStatus(record);
    cohort.total += 1;
    if (state === 'failed') cohort.failed += 1;
    else if (state === 'blocked') cohort.blocked += 1;
    else if (state === 'done' || state === 'killed') cohort.done += 1;
    else cohort.running += 1;
    cohort.attention = cohort.failed > 0 || cohort.blocked > 0;
    cohorts.set(id, cohort);
  }
  return [...cohorts.values()]
    .sort((a, b) => Number(b.attention) - Number(a.attention) || b.failed - a.failed || b.blocked - a.blocked || a.id.localeCompare(b.id))
    .slice(0, Math.max(0, limit));
}
