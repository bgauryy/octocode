import { describe, expect, it } from 'vitest';
import { assessOperationalState, assessRuntimeRegulation } from '../src/attend-physiology.js';

const scope = { workboard: {}, agentId: 'pi:session', workspacePath: '/repo', files: [], recalled: 0, referenceWarnings: 0 };
const observation = {
  schema_version: 1,
  source: 'pi_runtime',
  session: { owner: 'pi', session_id: 'session', generation: 1, observed_at: 100 },
  context: { measurement: 'host_reported', current_tokens: 95, measured_at: 100, input_limit_tokens: 100, remaining_input_tokens: 5, saturation_basis_points: 9_500 },
  tools: { window: 32, observed: 2, total_observed: 2, latest_outcome: 'failed', failed: 1, cancelled: 0, blocked: 0 },
  compaction: { owner: 'pi', committed: 1, failed: 0 },
} as const;

describe('Pi operational observations', () => {
  it('assesses runtime receipts without inventing a workspace observation', () => {
    expect(assessRuntimeRegulation(observation)).toEqual({
      runtime: observation,
      regulation: { advisory: true, actions: ['inspect_recent_tool_failures', 'inspect_context_headroom'] },
    });
  });
  it('assesses trusted Pi receipts without claiming native control ownership', () => {
    const result = assessOperationalState({ ...scope, runtimeObservation: observation });
    expect(result.operational_state.runtime).toEqual(observation);
    expect(result.regulation).toEqual({ advisory: true, actions: ['inspect_recent_tool_failures', 'inspect_context_headroom'] });
    expect(result.operational_state.unavailable).toContain('budget');
    expect(result.operational_state.unavailable).not.toContain('context');
    expect(result.operational_state.unavailable).not.toContain('tool_health');
  });

  it('keeps absent Pi samples unknown instead of treating them as healthy zeroes', () => {
    const result = assessOperationalState({ ...scope, runtimeObservation: {
      schema_version: 1, source: 'pi_runtime', session: observation.session,
    } });
    expect(result.operational_state.unavailable).toEqual(expect.arrayContaining(['context', 'tool_health', 'budget']));
    expect(result.regulation.actions).toEqual([]);
  });

  it('rejects content smuggling and inconsistent measurement arithmetic', () => {
    for (const invalid of [
      { ...observation, transcript: 'untrusted text' },
      { ...observation, context: { ...observation.context, remaining_input_tokens: 6 } },
      { ...observation, session: { ...observation.session, observed_at: 99 } },
    ]) {
      expect(() => assessOperationalState({ ...scope, runtimeObservation: invalid as never })).toThrow(/runtime observation/i);
    }
  });
});
