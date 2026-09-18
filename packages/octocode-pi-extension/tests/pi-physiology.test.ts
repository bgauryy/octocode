import assert from 'node:assert/strict';
import { describe, test } from 'vitest';
import {
  createPiPhysiologyObserver,
  readPiPhysiology,
  registerPiPhysiology,
} from '../src/adapters/pi-physiology.js';
import type { PiContext, PiInstance } from '../src/types.js';

function context(sessionId: string, usage: { tokens: number | null; contextWindow: number } = { tokens: 250, contextWindow: 1_000 }): PiContext {
  return {
    model: { id: 'model', contextWindow: 1_000 },
    sessionManager: { getSessionId: () => sessionId },
    getContextUsage: () => usage,
  };
}

function harness() {
  const handlers = new Map<string, (...args: any[]) => any>();
  const pi = { on: (event: string, handler: (...args: any[]) => any) => handlers.set(event, handler) } as unknown as PiInstance;
  return { pi, handlers };
}

describe('Pi physiology observer', () => {
  test('binds identity, increments generation, samples safe host-reported context, and clears on shutdown', async () => {
    let now = 100;
    const seen: unknown[] = [];
    const observer = createPiPhysiologyObserver({ now: () => now, onObservation: (value) => { seen.push(value); } });
    const first = context('session-a');

    await observer.sessionStart(first);
    assert.deepEqual(observer.read(first), {
      schema_version: 1,
      source: 'pi_runtime',
      session: { owner: 'pi', session_id: 'session-a', generation: 1, observed_at: 100 },
      context: {
        measurement: 'host_reported', current_tokens: 250, measured_at: 100,
        input_limit_tokens: 1_000, remaining_input_tokens: 750, saturation_basis_points: 2_500,
      },
    });
    assert.equal(seen.length, 1);

    now = 200;
    const second = context('session-b');
    await observer.sessionStart(second);
    assert.equal(observer.read(first), undefined);
    assert.equal(observer.read(second)?.session.generation, 2);
    await observer.sessionShutdown();
    assert.equal(observer.read(second), undefined);
  });

  test('omits unsafe, unknown, or model-mismatched context and invalidates stale samples', async () => {
    const observer = createPiPhysiologyObserver({ now: () => 10 });
    const ctx = context('session', { tokens: 1, contextWindow: 2_000 });
    await observer.sessionStart(ctx);
    assert.equal(observer.read(ctx)?.context, undefined);

    ctx.getContextUsage = () => ({ tokens: null, contextWindow: 1_000 });
    await observer.sampleContext(ctx);
    assert.equal(observer.read(ctx)?.context, undefined);
    ctx.getContextUsage = () => ({ tokens: Number.NaN, contextWindow: 1_000 });
    await observer.sampleContext(ctx);
    assert.equal(observer.read(ctx)?.context, undefined);

    ctx.getContextUsage = () => ({ tokens: 500, contextWindow: 1_000 });
    await observer.sampleContext(ctx);
    assert.equal(observer.read(ctx)?.context?.current_tokens, 500);
    await observer.invalidateContext(ctx);
    assert.equal(observer.read(ctx)?.context, undefined);
  });

  test('keeps the last 32 terminal tool outcomes, handles interleaving, and dedupes terminal events', async () => {
    const observer = createPiPhysiologyObserver({ now: () => 10 });
    const ctx = context('session');
    await observer.sessionStart(ctx);

    await observer.toolStart({ toolCallId: 'a', toolName: 'file', args: {} }, ctx);
    await observer.toolStart({ toolCallId: 'b', toolName: 'bash', args: {} }, ctx);
    await observer.toolTerminal({ toolCallId: 'b', toolName: 'bash', isError: true }, ctx);
    await observer.toolTerminal({ toolCallId: 'a', toolName: 'file', isError: false }, ctx);
    await observer.toolTerminal({ toolCallId: 'a', toolName: 'file', isError: false }, ctx);
    for (let index = 0; index < 32; index++) {
      await observer.toolTerminal({ toolCallId: `later-${index}`, toolName: 'file', isError: false }, ctx);
    }

    assert.deepEqual(observer.read(ctx)?.tools, {
      window: 32,
      observed: 32,
      total_observed: 34,
      latest_outcome: 'succeeded',
      failed: 0,
      cancelled: 0,
      blocked: 0,
    });
  });

  test('classifies cancellation and blocking separately and ignores unknown terminal state', async () => {
    const observer = createPiPhysiologyObserver({ now: () => 10 });
    const ctx = context('session');
    await observer.sessionStart(ctx);
    await observer.toolTerminal({ toolCallId: 'cancel', toolName: 'bash', isError: true, result: { aborted: true } }, ctx);
    await observer.toolTerminal({ toolCallId: 'block', toolName: 'file', blocked: true }, ctx);
    await observer.toolTerminal({ toolCallId: 'unknown', toolName: 'file', error: 'blocked and cancelled', result: {} }, ctx);

    assert.deepEqual(observer.read(ctx)?.tools, {
      window: 32,
      observed: 2,
      total_observed: 2,
      latest_outcome: 'blocked',
      failed: 0,
      cancelled: 1,
      blocked: 1,
    });
  });

  test('excludes Awareness inspection/internal operations without retaining their payloads', async () => {
    const observer = createPiPhysiologyObserver({ now: () => 10 });
    const ctx = context('session');
    await observer.sessionStart(ctx);
    await observer.toolStart({ toolCallId: 'internal', toolName: 'bash', args: { command: 'npx @octocodeai/octocode-awareness context orient --compact' } }, ctx);
    await observer.toolTerminal({ toolCallId: 'internal', toolName: 'bash', isError: false }, ctx);
    await observer.toolStart({
      toolCallId: 'internal-env',
      toolName: 'bash',
      args: { queries: [{ reasoning: 'inspect', command: '"$OCTOCODE_NODE" "$OCTOCODE_AWARENESS_CLI" status', timeout: 10 }] },
    }, ctx);
    await observer.toolTerminal({ toolCallId: 'internal-env', toolName: 'bash', result: { details: { code: 0 } }, isError: false }, ctx);
    await observer.toolTerminal({
      toolCallId: 'internal-env',
      toolName: 'bash',
      input: { queries: [{ command: '"$OCTOCODE_NODE" "$OCTOCODE_AWARENESS_CLI" status' }] },
      isError: false,
    }, ctx);
    await observer.toolTerminal({
      toolCallId: 'internal-terminal-only',
      toolName: 'bash',
      input: { queries: [{ reasoning: 'inspect coordination', command: '"$OCTOCODE_NODE" "$OCTOCODE_AWARENESS_CLI" context orient --compact' }] },
      isError: false,
    }, ctx);
    await observer.toolTerminal({
      toolCallId: 'mixed-batch',
      toolName: 'bash',
      input: { queries: [
        { command: '"$OCTOCODE_NODE" "$OCTOCODE_AWARENESS_CLI" status' },
        { command: 'git status --short' },
      ] },
      isError: false,
    }, ctx);
    await observer.toolTerminal({ toolCallId: 'visible', toolName: 'file', isError: false }, ctx);
    assert.deepEqual(observer.read(ctx)?.tools, {
      window: 32,
      observed: 2,
      total_observed: 2,
      latest_outcome: 'succeeded',
      failed: 0,
      cancelled: 0,
      blocked: 0,
    });
  });

  test('rejects stale and foreign contexts', async () => {
    const observer = createPiPhysiologyObserver({ now: () => 10 });
    const active = context('session');
    const foreign = context('foreign');
    await observer.sessionStart(active);
    const freshDispatchContext = context('session');
    await observer.toolTerminal({ toolCallId: 'fresh', toolName: 'file', isError: false }, freshDispatchContext);
    await observer.toolTerminal({ toolCallId: 'foreign', toolName: 'file', isError: true }, foreign);
    assert.equal(observer.read(active)?.tools?.observed, 1);

    const next = context('next-session');
    await observer.sessionStart(next);
    await observer.sessionShutdown(active);
    assert.equal(observer.read(next)?.session.session_id, 'next-session');
  });

  test('delivers a new generation without queuing it behind an old async callback', async () => {
    let release!: () => void;
    const delayed = new Promise<void>((resolve) => { release = resolve; });
    const seen: string[] = [];
    const observer = createPiPhysiologyObserver({
      now: () => 10,
      onObservation: (observation) => {
        seen.push(observation.session.session_id);
        return observation.session.session_id === 'old' ? delayed : undefined;
      },
    });
    await observer.sessionStart(context('old'));
    await observer.sessionStart(context('new'));
    assert.deepEqual(seen, ['old', 'new']);
    release();
    await delayed;
  });

  test('counts successful and failed compactions once and invalidates context at compaction boundaries', async () => {
    const observer = createPiPhysiologyObserver({ now: () => 10 });
    const ctx = context('session');
    await observer.sessionStart(ctx);
    await observer.compactionStart({}, ctx);
    assert.equal(observer.read(ctx)?.context, undefined);
    await observer.compactionSucceeded({ compactionEntry: { id: 'compact-1' } }, ctx);
    await observer.compactionSucceeded({ compactionEntry: { id: 'compact-1' } }, ctx);
    await observer.compactionStart({}, ctx);
    await observer.compactionFailed({}, ctx);
    await observer.compactionFailed({}, ctx);
    assert.deepEqual(observer.read(ctx)?.compaction, { owner: 'pi', committed: 1, failed: 1 });
  });

  test('registers native event mappings and exposes the current observation by context', async () => {
    const { pi, handlers } = harness();
    registerPiPhysiology(pi, { now: () => 10 });
    const ctx = context('session');
    await handlers.get('session_start')?.({ reason: 'startup' }, ctx);
    await handlers.get('before_agent_start')?.({}, ctx);
    await handlers.get('tool_execution_start')?.({ toolCallId: 'call', toolName: 'file', args: {} }, ctx);
    await handlers.get('tool_execution_end')?.({ toolCallId: 'call', toolName: 'file', result: {}, isError: false }, ctx);
    await handlers.get('tool_result')?.({ toolCallId: 'call', toolName: 'file', result: {}, isError: false }, ctx);
    assert.equal(readPiPhysiology(ctx)?.tools?.observed, 1);
    const freshCtx = { ...ctx };
    assert.equal(readPiPhysiology(freshCtx)?.tools?.observed, 1);
    await handlers.get('session_shutdown')?.({ reason: 'quit' }, freshCtx);
    assert.equal(readPiPhysiology(ctx), undefined);
  });
});
