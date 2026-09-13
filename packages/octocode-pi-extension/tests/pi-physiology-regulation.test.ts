import { describe, expect, it } from 'vitest';
import type { PiRuntimeObservation } from '@octocodeai/agent-contracts/physiology';
import { createPiPhysiologyAdvisory } from '../src/adapters/pi-physiology-regulation.js';

const sample = (failed = 1, generation = 1): PiRuntimeObservation => ({
  schema_version: 1, source: 'pi_runtime',
  session: { owner: 'pi', session_id: 's', generation, observed_at: 100 },
  tools: {
    window: 32,
    observed: 1,
    total_observed: 1,
    latest_outcome: failed > 0 ? 'failed' : 'succeeded',
    failed,
    cancelled: 0,
    blocked: 0,
  },
});
describe('Pi physiology advisory delivery', () => {
  it('deduplicates pressure without interpreting unavailable measurements as recovery', () => {
    const prepare = createPiPhysiologyAdvisory();
    const project = (observation: PiRuntimeObservation) => {
      const delivery = prepare(observation);
      delivery.commit();
      return delivery.content;
    };
    expect(project(sample())).toContain('inspect_recent_tool_failures');
    expect(project(sample())).toBe('');
    const unknown = sample();
    delete unknown.tools;
    expect(project(unknown)).toBe('');
    expect(project(sample())).toBe('');
    expect(project(sample(0))).toBe('');
    expect(project(sample())).toContain('inspect_recent_tool_failures');
    expect(project(sample(1, 2))).toContain('inspect_recent_tool_failures');
  });
  it('omits unavailable state and bounds feedback to canonical action names', () => {
    const project = createPiPhysiologyAdvisory();
    expect(project(undefined).content).toBe('');
    const advisory = project(sample()).content;
    expect(advisory).toContain('context.orient');
    expect(advisory.length).toBeLessThan(512);
  });
  it('keeps a prepared advisory retryable until prompt delivery commits', () => {
    const project = createPiPhysiologyAdvisory();
    const rejected = project(sample());
    const retry = project(sample());
    expect(retry.content).toEqual(rejected.content);
    expect(retry.content).not.toBe('');
    retry.commit();
    expect(project(sample()).content).toBe('');
  });
  it('ignores stale commits after a newer session delivery', () => {
    const project = createPiPhysiologyAdvisory();
    const stale = project(sample(1, 1));
    project(sample(1, 2)).commit();
    stale.commit();
    expect(project(sample(1, 2)).content).toBe('');
  });
});
