import { describe, expect, it } from 'vitest';
import { summarizeAgentCohorts } from '../src/tools/agents/cohorts.js';

describe('agent cohorts', () => {
  it('groups workers by explicit cohort and sorts attention before healthy work', () => {
    expect(summarizeAgentCohorts([
      { id: 'a', cohortId: 'implementation', status: 'idle', normalizedStatus: 'done' },
      { id: 'b', cohortId: 'review', status: 'idle', normalizedStatus: 'blocked' },
      { id: 'c', cohortId: 'implementation', status: 'running' },
      { id: 'd', status: 'failed' },
    ])).toEqual([
      { id: 'ungrouped', total: 1, running: 0, done: 0, blocked: 0, failed: 1, attention: true },
      { id: 'review', total: 1, running: 0, done: 0, blocked: 1, failed: 0, attention: true },
      { id: 'implementation', total: 2, running: 1, done: 1, blocked: 0, failed: 0, attention: false },
    ]);
  });

  it('returns bounded aggregate data without task text or worker output', () => {
    const cohorts = summarizeAgentCohorts(Array.from({ length: 80 }, (_, index) => ({
      id: `worker-${index}`,
      cohortId: `cohort-${index}`,
      status: 'running',
      task: `private-${index}`,
    })), 3);
    expect(cohorts).toHaveLength(3);
    expect(JSON.stringify(cohorts)).not.toContain('private-');
  });
});
