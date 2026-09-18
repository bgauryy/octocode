import type {
  AwarenessHistoryCaptureInput,
  AwarenessHost,
} from '@octocodeai/octocode-awareness/host';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createPiHistoryAdapter } from '../src/adapters/pi-history-adapter.js';

type Capture = (input: AwarenessHistoryCaptureInput) => Promise<Record<string, unknown>>;

function hostWith(captureHistory: Capture): (context: AwarenessHost['context']) => AwarenessHost {
  return context => ({ context, captureHistory });
}

describe('Pi history adapter', () => {
  afterEach(() => vi.unstubAllEnvs());

  it('does not capture mutations under the default coordination profile', async () => {
    const capture = vi.fn<Capture>();
    const adapter = createPiHistoryAdapter({ createHost: hostWith(capture), agentId: () => 'pi:test' });
    await adapter.before({ toolCallId: 'ordinary-edit', toolName: 'file', input: { queries: [{ type: 'write', path: 'a.ts' }] } }, { cwd: '/tmp/awareness-default-profile' } as never);
    expect(capture).not.toHaveBeenCalled();
    expect(adapter.pending()).toBe(0);
  });

  it('captures native file mutations before and after under one operation id', async () => {
    const calls: AwarenessHistoryCaptureInput[] = [];
    const capture: Capture = async input => {
      calls.push(input);
      return { ok: true, operation: {} };
    };
    const adapter = createPiHistoryAdapter({
      enabled: () => true, agentId: () => 'pi:test', createHost: hostWith(capture),
    });
    const ctx = { cwd: '/tmp/workspace' } as never;
    await adapter.before({ toolCallId: 'call-1', toolName: 'file', input: { queries: [
      { type: 'edit', path: 'src/a.ts' }, { type: 'write', path: 'src/b.ts' },
    ] } }, ctx);
    await adapter.after({ toolCallId: 'call-1', toolName: 'file', result: {}, isError: false }, ctx);

    expect(calls).toHaveLength(2);
    expect(calls[0]).toEqual(expect.objectContaining({ phase: 'before', file: ['src/a.ts', 'src/b.ts'] }));
    expect(calls[1]).toEqual(expect.objectContaining({ phase: 'after', outcome: 'success', session_id: 'unknown-session' }));
    expect(calls[1]?.operation_id).toBe(calls[0]?.operation_id);
    expect(adapter.pending()).toBe(0);
  });

  it('binds capture to the inherited Awareness database and canonical scope', async () => {
    vi.stubEnv('OCTOCODE_AWARENESS_DB', '/shared/awareness.sqlite3');
    let context: AwarenessHost['context'] | undefined;
    const adapter = createPiHistoryAdapter({
      enabled: () => true,
      agentId: () => 'pi:test',
      createHost: bound => {
        context = bound;
        return { context: bound, captureHistory: async () => ({ ok: true, operation: {} }) };
      },
    });

    await adapter.before({
      toolCallId: 'bound-history',
      toolName: 'file',
      input: { queries: [{ type: 'write', path: 'src/a.ts' }] },
    }, { cwd: '/tmp/worktree' } as never);

    expect(context).toEqual(expect.objectContaining({
      workspace: '/tmp/worktree',
      database: '/shared/awareness.sqlite3',
      agentId: 'pi:test',
    }));
    expect(context?.scope).toBeDefined();
  });

  it('ignores paths from read, shell, MCP, and unknown tools', async () => {
    const capture = vi.fn<Capture>();
    const adapter = createPiHistoryAdapter({ createHost: hostWith(capture), agentId: () => 'pi:test' });
    for (const toolName of ['bash', 'MCPTool', 'localFetch', 'mystery']) {
      await adapter.before({ toolCallId: toolName, toolName, input: { queries: [{ type: 'write', path: 'src/a.ts' }] } });
    }
    expect(capture).not.toHaveBeenCalled();
  });

  it('treats missing host correlation as an observational no-op', async () => {
    const capture = vi.fn<Capture>();
    const errors: Error[] = [];
    const adapter = createPiHistoryAdapter({ createHost: hostWith(capture), enabled: () => true, agentId: () => 'pi:test', onError: error => errors.push(error) });
    await adapter.before({ toolName: 'file', input: { queries: [{ type: 'write', path: 'a.ts' }] } } as never);
    expect(capture).not.toHaveBeenCalled();
    expect(errors).toEqual([]);
    expect(adapter.pending()).toBe(0);
  });

  it('records failures and does not invent an after capture when before failed', async () => {
    const calls: AwarenessHistoryCaptureInput[] = [];
    const errors: Error[] = [];
    const capture: Capture = async input => { calls.push(input); throw new Error('nope'); };
    const adapter = createPiHistoryAdapter({
      enabled: () => true, agentId: () => 'pi:test', onError: error => errors.push(error), createHost: hostWith(capture),
    });
    await adapter.before({ toolCallId: 'failed-before', toolName: 'file', input: { queries: [{ type: 'write', path: 'a.ts' }] } });
    await adapter.after({ toolCallId: 'failed-before', toolName: 'file', result: {}, isError: true });
    expect(calls).toHaveLength(1);
    expect(errors).toHaveLength(1);
  });

  it('closes a successful before capture with a failure outcome', async () => {
    const calls: AwarenessHistoryCaptureInput[] = [];
    const capture: Capture = async input => { calls.push(input); return { ok: true, operation: {} }; };
    const adapter = createPiHistoryAdapter({
      enabled: () => true, agentId: () => 'pi:test', createHost: hostWith(capture),
    });
    await adapter.before({ toolCallId: 'failed-tool', toolName: 'file', input: { queries: [{ type: 'delete', path: 'a.ts' }] } });
    await adapter.after({ toolCallId: 'failed-tool', toolName: 'file', result: {}, isError: true });
    expect(calls[1]).toEqual(expect.objectContaining({ phase: 'after', outcome: 'failure' }));
  });

  it('is inert when disabled and deduplicates starts', async () => {
    const capture = vi.fn<Capture>(async () => ({ ok: true, operation: {} }));
    const event = { toolCallId: 'same', toolName: 'file', input: { queries: [{ type: 'write', path: 'a.ts' }] } };
    const disabled = createPiHistoryAdapter({ createHost: hostWith(capture), enabled: () => false, agentId: () => 'pi:test' });
    await disabled.before(event);
    expect(capture).not.toHaveBeenCalled();

    const enabled = createPiHistoryAdapter({ createHost: hostWith(capture), enabled: () => true, agentId: () => 'pi:test' });
    await enabled.before(event);
    await enabled.before(event);
    expect(capture).toHaveBeenCalledTimes(1);
  });

  it('keeps a pending completion when after capture fails so it can retry', async () => {
    let attempts = 0;
    const capture: Capture = async () => {
      attempts += 1;
      if (attempts === 2) throw new Error('retry');
      return { ok: true, operation: {} };
    };
    const adapter = createPiHistoryAdapter({
      enabled: () => true, agentId: () => 'pi:test', onError: () => undefined, createHost: hostWith(capture),
    });
    const before = { toolCallId: 'retry', toolName: 'file', input: { queries: [{ type: 'edit', path: 'a.ts' }] } };
    const after = { toolCallId: 'retry', toolName: 'file', result: {}, isError: false };
    await adapter.before(before);
    await adapter.after(after);
    expect(adapter.pending()).toBe(1);
    await adapter.after(after);
    expect(adapter.pending()).toBe(0);
  });
});
