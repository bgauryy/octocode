import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { createAwarenessClient, type AwarenessOperationResult, type ContextObservation } from '@octocodeai/octocode-awareness';
import type { PiRuntimeObservation } from '@octocodeai/octocode-awareness/host';
import type { PiContext } from '../src/types.js';
import { createPiAwarenessObservationSink } from '../src/adapters/pi-awareness-observation.js';

const roots: string[] = [];
afterEach(() => {
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function sample(input: Partial<PiRuntimeObservation> = {}): PiRuntimeObservation {
  return {
    schema_version: 1,
    source: 'pi_runtime',
    session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_000 },
    context: {
      measurement: 'host_reported', current_tokens: 90, measured_at: 1_000,
      input_limit_tokens: 100, remaining_input_tokens: 10, saturation_basis_points: 9_000,
    },
    ...input,
  };
}

describe('Pi durable context observation sink', () => {
  test('maps changed host context and tool outcomes without replaying unchanged samples', async () => {
    const observe = vi.fn(async (_ctx: PiContext, _input: ContextObservation): Promise<AwarenessOperationResult> => (
      { exitCode: 0, payload: { ok: true } }
    ));
    const sink = createPiAwarenessObservationSink({ enabled: () => true, observe });
    const ctx = {} as PiContext;

    await sink(sample(), ctx);
    await sink(sample({ session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_001 } }), ctx);
    await sink(sample({
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_002 },
      tools: {
        window: 32, observed: 1, total_observed: 1, latest_outcome: 'failed',
        failed: 1, cancelled: 0, blocked: 0,
      },
    }), ctx);
    await sink(sample({
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_003 },
      tools: {
        window: 32, observed: 1, total_observed: 1, latest_outcome: 'failed',
        failed: 1, cancelled: 0, blocked: 0,
      },
    }), ctx);

    expect(observe).toHaveBeenCalledTimes(2);
    expect(observe.mock.calls[0]?.[1]).toMatchObject({
      source: 'host', acquisition: 'passive', context: { used: 90, limit: 100 },
    });
    expect(observe.mock.calls[1]?.[1]).toMatchObject({
      action_fingerprint: 'pi-runtime-tool', outcome: 'failure',
    });
  });

  test('reports the first tool outcome and outcomes after the rolling window fills', async () => {
    const reports: ContextObservation[] = [];
    const sink = createPiAwarenessObservationSink({
      enabled: () => true,
      observe: async (_ctx, input) => {
        reports.push(input);
        return { exitCode: 0, payload: { ok: true } };
      },
    });
    const ctx = {} as PiContext;

    await sink(sample({ context: undefined }), ctx);
    await sink(sample({
      context: undefined,
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_001 },
      tools: {
        window: 32, observed: 1, total_observed: 1, latest_outcome: 'failed',
        failed: 1, cancelled: 0, blocked: 0,
      },
    }), ctx);
    await sink(sample({
      context: undefined,
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_002 },
      tools: {
        window: 32, observed: 32, total_observed: 32, latest_outcome: 'succeeded',
        failed: 1, cancelled: 0, blocked: 0,
      },
    }), ctx);
    await sink(sample({
      context: undefined,
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_003 },
      tools: {
        window: 32, observed: 32, total_observed: 33, latest_outcome: 'blocked',
        failed: 0, cancelled: 0, blocked: 1,
      },
    }), ctx);

    expect(reports.map(report => report.outcome)).toEqual(['failure', 'success']);
  });

  test.each(['cancelled', 'blocked'] as const)('does not invent a failure for a %s tool', async latest_outcome => {
    const reports: ContextObservation[] = [];
    const sink = createPiAwarenessObservationSink({
      enabled: () => true,
      observe: async (_ctx, input) => {
        reports.push(input);
        return { exitCode: 0, payload: { ok: true } };
      },
    });
    await sink(sample({ tools: {
      window: 32, observed: 1, total_observed: 1, latest_outcome,
      failed: 0, cancelled: Number(latest_outcome === 'cancelled'), blocked: Number(latest_outcome === 'blocked'),
    } }), {} as PiContext);

    expect(reports).toHaveLength(1);
    expect(reports[0]?.context).toEqual({ used: 90, limit: 100 });
    expect(reports[0]).not.toHaveProperty('outcome');
    expect(reports[0]).not.toHaveProperty('action_fingerprint');
  });

  test('missing sensors do not erase the last delivered projection or replay old outcomes', async () => {
    const observe = vi.fn(async (): Promise<AwarenessOperationResult> => ({ exitCode: 0, payload: { ok: true } }));
    const sink = createPiAwarenessObservationSink({ enabled: () => true, observe });
    const first = sample({ tools: {
      window: 32, observed: 1, total_observed: 1, latest_outcome: 'failed', failed: 1, cancelled: 0, blocked: 0,
    } });
    await sink(first, {} as PiContext);
    await sink(sample({ context: undefined }), {} as PiContext);
    await sink({ ...first, session: { ...first.session, observed_at: 1_002 } }, {} as PiContext);

    expect(observe).toHaveBeenCalledTimes(1);
  });

  test('keeps same-millisecond tool observations distinct without replaying the same terminal event', async () => {
    const reports: ContextObservation[] = [];
    const sink = createPiAwarenessObservationSink({
      enabled: () => true,
      observe: async (_ctx, input) => {
        reports.push(input);
        return { exitCode: 0, payload: { ok: true } };
      },
    });
    const ctx = {} as PiContext;
    const terminal = (count: number) => sample({
      context: undefined,
      tools: {
        window: 32, observed: count, total_observed: count, latest_outcome: 'succeeded',
        failed: 0, cancelled: 0, blocked: 0,
      },
    });
    await sink(terminal(1), ctx);
    await sink(terminal(2), ctx);
    await sink(terminal(2), ctx);

    expect(reports).toHaveLength(2);
    expect(reports[0]?.observed_at).toBe(reports[1]?.observed_at);
    expect(reports[0]?.observation_id).not.toBe(reports[1]?.observation_id);
  });

  test('retries transient persistence failures before committing the projection', async () => {
    const observe = vi.fn()
      .mockResolvedValueOnce({ exitCode: 1, payload: { error: 'busy' } })
      .mockResolvedValueOnce({ exitCode: 0, payload: { ok: true } });
    const sink = createPiAwarenessObservationSink({ enabled: () => true, observe });
    const ctx = {} as PiContext;

    await sink(sample(), ctx);
    await sink(sample({ session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_001 } }), ctx);

    expect(observe).toHaveBeenCalledTimes(2);
  });

  test('keeps failed projections retryable when diagnostics throw', async () => {
    const observe = vi.fn()
      .mockResolvedValueOnce({ exitCode: 1, payload: { error: 'busy-1' } })
      .mockResolvedValueOnce({ exitCode: 1, payload: { error: 'busy-2' } })
      .mockResolvedValueOnce({ exitCode: 1, payload: { error: 'busy-3' } })
      .mockResolvedValueOnce({ exitCode: 0, payload: { ok: true } });
    const sink = createPiAwarenessObservationSink({
      enabled: () => true,
      observe,
      onError: () => { throw new Error('diagnostic failure'); },
    });
    const ctx = {} as PiContext;

    await sink(sample(), ctx);
    await sink(sample({ session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_001 } }), ctx);
    await sink(sample({ session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 1_002 } }), ctx);

    expect(observe).toHaveBeenCalledTimes(4);
  });

  test('bounds committed session projections', async () => {
    const observe = vi.fn(async (): Promise<AwarenessOperationResult> => (
      { exitCode: 0, payload: { ok: true } }
    ));
    const sink = createPiAwarenessObservationSink({ enabled: () => true, observe });
    const ctx = {} as PiContext;

    for (let generation = 1; generation <= 9; generation++) {
      await sink(sample({
        session: { owner: 'pi', session_id: 'session-1', generation, observed_at: 1_000 + generation },
      }), ctx);
    }
    await sink(sample({
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: 2_000 },
    }), ctx);

    expect(observe).toHaveBeenCalledTimes(10);
  });

  test('stays inert when persistent storage is disabled', async () => {
    const observe = vi.fn(async (_ctx: PiContext, _input: ContextObservation): Promise<AwarenessOperationResult> => (
      { exitCode: 0, payload: { ok: true } }
    ));
    const sink = createPiAwarenessObservationSink({ enabled: () => false, observe });
    await sink(sample(), {} as PiContext);
    expect(observe).not.toHaveBeenCalled();
  });

  test('persists a passive Pi measurement through the real Awareness client', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-awareness-observation-'));
    roots.push(root);
    const database = path.join(root, 'awareness.sqlite3');
    vi.stubEnv('OCTOCODE_AWARENESS_DB', database);
    vi.stubEnv('OCTOCODE_AGENT_ID', 'pi:sink-test');
    const ctx = {
      cwd: root,
      sessionManager: { getSessionId: () => 'session-1' },
    } as PiContext;
    const now = Date.now();
    const sink = createPiAwarenessObservationSink({ enabled: () => true });
    const observation = sample({
      session: { owner: 'pi', session_id: 'session-1', generation: 1, observed_at: now },
      context: {
        measurement: 'host_reported', current_tokens: 95, measured_at: now,
        input_limit_tokens: 100, remaining_input_tokens: 5, saturation_basis_points: 9_500,
      },
    });
    await sink(observation, ctx);
    for (const count of [1, 2, 2]) {
      await sink({
        ...observation,
        tools: {
          window: 32, observed: count, total_observed: count, latest_outcome: 'succeeded',
          failed: 0, cancelled: 0, blocked: 0,
        },
      }, ctx);
    }

    const orientation = await createAwarenessClient({
      workspace: root, database, agentId: 'pi:sink-test', sessionId: 'session-1',
    }).orient();
    expect(orientation.unchanged).toBe(false);
    if (!orientation.unchanged) {
      expect(orientation.operational.runtime).toMatchObject({
        source: 'host', acquisition: 'passive', context: { used: 95, limit: 100 },
        coverage: { observations: 3 },
      });
      expect(orientation.run_state.status).toBe('pressured');
    }
  });

  test('binds queued observations to their originating actor and database before a session switch', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-awareness-binding-'));
    roots.push(root);
    const database = path.join(root, 'origin.sqlite3');
    vi.stubEnv('OCTOCODE_AWARENESS_DB', database);
    vi.stubEnv('OCTOCODE_AGENT_ID', 'pi:origin');
    let sessionId = 'session-1';
    const ctx = { cwd: root, sessionManager: { getSessionId: () => sessionId } } as PiContext;
    const now = Date.now();
    const sink = createPiAwarenessObservationSink({ enabled: () => true });
    const pending = sink(sample({
      session: { owner: 'pi', session_id: sessionId, generation: 1, observed_at: now },
    }), ctx);
    sessionId = 'session-2';
    vi.stubEnv('OCTOCODE_AGENT_ID', 'pi:next');
    vi.stubEnv('OCTOCODE_AWARENESS_DB', path.join(root, 'next.sqlite3'));
    await pending;

    const original = await createAwarenessClient({ workspace: root, database, agentId: 'pi:origin', sessionId: 'session-1' }).orient();
    expect(original.unchanged).toBe(false);
    if (!original.unchanged) expect(original.operational.runtime?.coverage.observations).toBe(1);
    expect(fs.existsSync(path.join(root, 'next.sqlite3'))).toBe(false);
  });
});
