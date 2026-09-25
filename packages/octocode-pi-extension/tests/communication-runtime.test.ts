import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PiContext, PiInstance } from '../src/types.js';

const state = vi.hoisted(() => ({
  enabled: true,
  matchesContext: true,
  binding: { session: 'self', workspace: '/repo' } as { session: string; workspace: string } | undefined,
  call: vi.fn(),
  register: vi.fn(),
}));
vi.mock('@octocodeai/config', async (load) => ({
  ...await load<typeof import('@octocodeai/config')>(),
  isPersistentStorageEnabledForExtension: () => state.enabled,
}));
vi.mock('node:module', async (load) => {
  const actual = await load<typeof import('node:module')>();
  return { ...actual, createRequire: (url: string) => {
    const original = actual.createRequire(url);
    return Object.assign((id: string) => id.endsWith('/pi-inbox.mjs') ? {
      registerPiInbox: (...args: unknown[]) => {
        state.register(...args);
        return { getBinding: () => state.binding, isBoundContext: () => state.matchesContext, call: state.call };
      },
    } : original(id), original);
  } };
});
import { communicationMutationTargets, registerCommunicationRuntime } from '../src/tools/communication-runtime.js';

describe('communication runtime bridge', () => {
  beforeEach(() => {
    state.enabled = true;
    state.matchesContext = true;
    state.binding = { session: 'self', workspace: '/repo' };
    state.call.mockReset().mockResolvedValue({ ok: true });
    state.register.mockReset();
  });
  function setup() {
    let gate!: (event: unknown, ctx: PiContext) => Promise<unknown>;
    const pi = { on: (event: string, handler: typeof gate) => { if (event === 'tool_call') gate = handler; } };
    registerCommunicationRuntime(pi as unknown as PiInstance);
    return (toolName: string, input: unknown) => gate({ toolName, input }, { cwd: '/repo' } as PiContext);
  }
  it('does not query shared leases for reads or memory-only sessions', async () => {
    const gate = setup();
    expect(await gate('read', { path: 'a.ts' })).toBeUndefined();
    state.enabled = false;
    expect(await gate('write', { path: 'a.ts' })).toBeUndefined();
    expect(state.call).not.toHaveBeenCalled();
  });
  it('blocks a conflicting batch, with owner intent available for negotiation', async () => {
    const gate = setup();
    state.call.mockResolvedValue({ ok: false, conflicts: [{ owner: 'peer', reasoning: 'migrating schema' }] });
    expect(await gate('file', { queries: [{ type: 'delete', path: 'a.ts' }, { type: 'write', path: 'b.ts' }] }))
      .toEqual({ block: true, reason: expect.stringContaining('migrating schema') });
    expect(state.call).toHaveBeenCalledWith('check_paths', { paths: [
      { path: '/repo/a.ts', kind: 'file' }, { path: '/repo/b.ts', kind: 'file' },
    ] });
  });
  it('allows clear paths, but does not silently allow edits when the DB or identity is unavailable', async () => {
    const gate = setup();
    expect(await gate('write', { path: 'a.ts' })).toBeUndefined();
    state.call.mockRejectedValue(new Error('DB unavailable'));
    expect(await gate('write', { path: 'a.ts' })).toEqual({ block: true, reason: expect.stringContaining('DB unavailable') });
    state.binding = undefined;
    expect(await gate('write', { path: 'a.ts' })).toEqual({ block: true, reason: expect.stringContaining('identity') });
  });
  it('blocks writes with stale session or workspace bindings while keeping reads available', async () => {
    const gate = setup();
    state.matchesContext = false;
    expect(await gate('write', { path: 'a.ts' })).toEqual({ block: true, reason: expect.stringContaining('another session/workspace') });
    expect(await gate('read', { path: 'a.ts' })).toBeUndefined();
    expect(state.call).not.toHaveBeenCalled();
  });
  it('checks shell removals and both sides of renames as trees', async () => {
    const gate = setup();
    await gate('bash', { queries: [{ command: 'mv source.txt target.txt; rm -r directory' }] });
    expect(state.call).toHaveBeenCalledWith('check_paths', { paths: expect.arrayContaining([
      { path: '/repo/source.txt', kind: 'tree' }, { path: '/repo/target.txt', kind: 'tree' }, { path: '/repo/directory', kind: 'tree' },
    ]) });
  });
  it('includes deletion and move destinations, deduplicating paths', () => {
    expect(communicationMutationTargets({ toolName: 'apply_patch', input: {
      patch: '*** Delete File: a.ts\n*** Update File: b.ts\n*** Move to: c.ts\n', path: 'a.ts',
    } }, '/repo')).toEqual(['/repo/a.ts', '/repo/b.ts', '/repo/c.ts']);
  });
});
