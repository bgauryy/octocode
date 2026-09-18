import { describe, expect, it, vi } from 'vitest';
import { createAwarenessMutationGate } from '../src/tools/awareness-mutation-gate.js';

describe('awareness mutation gate', () => {
  const cwd = '/repo';
  const event = (toolName: string, input: Record<string, unknown>) => ({ toolName, input });

  it('checks existing locks without creating work or verification debt in coordination mode', () => {
    const startWork = vi.fn(); const endWork = vi.fn();
    const queryTarget = vi.fn(() => ({ blocked: false }));
    const gate = createAwarenessMutationGate({
      storeExists: () => true, trackWork: () => false, queryTarget, startWork, endWork,
    });
    const write = event('file', { queries: [{ type: 'write', path: 'a.ts' }] });
    expect(gate.preflight(write, cwd, 'me')).toBeUndefined();
    gate.complete(write, cwd, 'me', true);
    gate.cleanup();
    expect(queryTarget).toHaveBeenCalledOnce();
    expect(startWork).not.toHaveBeenCalled();
    expect(endWork).not.toHaveBeenCalled();
    queryTarget.mockReturnValue({ blocked: true });
    expect(gate.preflight(write, cwd, 'me')?.block).toBe(true);
  });

  it.each(['write', 'edit', 'delete'])('blocks the registered file tool %s operation before presence', (type) => {
    const startWork = vi.fn(() => 'run_file');
    const queryTarget = vi.fn(() => ({ blocked: true, message: 'peer owns file' }));
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget, startWork, endWork: vi.fn() });
    expect(gate.preflight(event('file', { queries: [{ type, path: 'a.ts' }] }), cwd, 'me'))
      .toEqual({ block: true, reason: 'peer owns file' });
    expect(queryTarget).toHaveBeenCalledWith('/repo/a.ts', cwd, 'me');
    expect(startWork).not.toHaveBeenCalled();
  });

  it('tracks successful registered file batches through completion', () => {
    const startWork = vi.fn((target: string) => `run:${target}`);
    const endWork = vi.fn();
    const gate = createAwarenessMutationGate({ storeExists: () => false, queryTarget: vi.fn(), startWork, endWork });
    const file = event('file', { queries: [{ type: 'write', path: 'a.ts' }, { type: 'delete', path: 'b.ts' }] });
    gate.preflight(file, cwd, 'me');
    gate.complete(file, cwd, 'me', true);
    expect(endWork).toHaveBeenCalledTimes(2);
  });

  it('blocks structured queries before starting any presence', () => {
    const startWork = vi.fn(() => 'run_blocked');
    const gate = createAwarenessMutationGate({
      storeExists: () => true,
      queryTarget: (target) => target.endsWith('b.ts') ? { blocked: true, message: 'peer lock' } : { blocked: false },
      startWork,
      endWork: vi.fn(),
    });
    expect(gate.preflight(event('edit', { queries: [{ path: 'a.ts' }, { path: 'b.ts' }] }), cwd, 'me')).toEqual({ block: true, reason: 'peer lock' });
    expect(startWork).not.toHaveBeenCalled();
  });

  it('supports host and queries envelopes and refreshes owned work after lock success', () => {
    const startWork = vi.fn(() => 'run_refreshed');
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget: () => ({ blocked: false }), startWork, endWork: vi.fn() });
    expect(gate.preflight(event('write', { path: 'a.ts' }), cwd, 'me')).toBeUndefined();
    expect(gate.preflight(event('write', { queries: [{ path: 'a.ts' }] }), cwd, 'me')).toBeUndefined();
    expect(startWork).toHaveBeenCalledOnce();
    expect(startWork).toHaveBeenLastCalledWith('/repo/a.ts', '/repo', 'me');
  });

  it.each([
    ["sed -i 's/a/b/' src/a.ts", '/repo/src/a.ts'],
    ['printf x > src/a.ts', '/repo/src/a.ts'],
    ['printf x | tee src/a.ts', '/repo/src/a.ts'],
    ['cp tmp/a src/a.ts', '/repo/src/a.ts'],
    ['mv tmp/a src/a.ts', '/repo/src/a.ts'],
  ])('checks identifiable bash target: %s', (command, target) => {
    const queryTarget = vi.fn(() => ({ blocked: true, message: 'locked' }));
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget, startWork: vi.fn(() => 'run_bash'), endWork: vi.fn() });
    expect(gate.preflight(event('bash', { queries: [{ command }] }), cwd, 'me')).toEqual({ block: true, reason: 'locked' });
    expect(queryTarget).toHaveBeenCalledWith(target, cwd, 'me');
  });

  it.each(['npm run build', `node -e "require('fs').writeFileSync('src/a.ts','x')"`])('passes unidentifiable bash without claiming coverage: %s', (command) => {
    const queryTarget = vi.fn(); const startWork = vi.fn(() => 'run_unidentified');
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget, startWork, endWork: vi.fn() });
    expect(gate.preflight(event('bash', { command }), cwd, 'me')).toBeUndefined();
    expect(queryTarget).not.toHaveBeenCalled(); expect(startWork).not.toHaveBeenCalled();
  });

  it('fails open when the store is absent but still attempts advisory presence', () => {
    const queryTarget = vi.fn(); const startWork = vi.fn(() => 'run_absent');
    const gate = createAwarenessMutationGate({ storeExists: () => false, queryTarget, startWork, endWork: vi.fn() });
    expect(gate.preflight(event('write', { path: 'a.ts' }), cwd, 'me')).toBeUndefined();
    expect(queryTarget).not.toHaveBeenCalled(); expect(startWork).toHaveBeenCalledOnce();
  });

  it('fails closed on an existing-store query failure for identifiable targets', () => {
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget: () => { throw new Error('corrupt'); }, startWork: vi.fn(() => 'run_corrupt'), endWork: vi.fn() });
    expect(gate.preflight(event('write', { path: 'a.ts' }), cwd, 'me')).toEqual({ block: true, reason: 'Awareness store query failed: corrupt' });
  });

  it('blocks when presence admission fails after the initial lock check', () => {
    const warn = vi.fn();
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget: () => ({ blocked: false }), startWork: () => { throw new Error('busy'); }, endWork: vi.fn(), warn });
    expect(gate.preflight(event('write', { path: 'a.ts' }), cwd, 'me')).toEqual({ block: true, reason: 'Awareness presence update failed: busy' });
    expect(warn).toHaveBeenCalledWith('Awareness presence update failed: busy');
  });

  it('rolls back only the failed batch participation when a peer acquires a later target', () => {
    const endWork = vi.fn();
    const gate = createAwarenessMutationGate({
      storeExists: () => true, queryTarget: () => ({ blocked: false }),
      startWork: (target) => { if (target.endsWith('b.ts')) throw new Error('peer lock arrived'); return 'run_a'; },
      endWork,
    });
    const first = event('file', { queries: [{ type: 'write', path: 'a.ts' }] });
    gate.preflight(first, cwd, 'me');
    expect(gate.preflight(event('file', { queries: [{ type: 'write', path: 'a.ts' }, { type: 'write', path: 'b.ts' }] }), cwd, 'me'))
      .toEqual({ block: true, reason: 'Awareness presence update failed: peer lock arrived' });
    expect(endWork).not.toHaveBeenCalled();
    gate.complete(first, cwd, 'me', true);
    expect(endWork).toHaveBeenCalledOnce();
  });

  it('skips durable admission when the host disables persistence', () => {
    const storeExists = vi.fn(); const startWork = vi.fn();
    const gate = createAwarenessMutationGate({ enabled: () => false, storeExists, queryTarget: vi.fn(), startWork, endWork: vi.fn() });
    expect(gate.preflight(event('file', { queries: [{ type: 'write', path: 'a.ts' }] }), cwd, 'me')).toBeUndefined();
    expect(storeExists).not.toHaveBeenCalled(); expect(startWork).not.toHaveBeenCalled();
  });

  it('cleans only presence owned by this gate', () => {
    const endWork = vi.fn();
    const startWork = vi.fn((target: string) => `run:${target}`);
    const gate = createAwarenessMutationGate({ storeExists: () => false, queryTarget: vi.fn(), startWork, endWork });
    gate.preflight(event('write', { queries: [{ path: 'a.ts' }, { path: 'b.ts' }] }), cwd, 'me');
    gate.cleanup();
    expect(endWork.mock.calls).toEqual([
      ['/repo/a.ts', '/repo', 'me', 'run:/repo/a.ts'],
      ['/repo/b.ts', '/repo', 'me', 'run:/repo/b.ts'],
    ]);
  });

  it('does not end a pre-existing task or lock run', () => {
    const endWork = vi.fn();
    const gate = createAwarenessMutationGate({ storeExists: () => true, queryTarget: () => ({ blocked: false }), startWork: () => null, endWork });
    const file = event('file', { queries: [{ type: 'write', path: 'a.ts' }] });
    gate.preflight(file, cwd, 'me');
    gate.complete(file, cwd, 'me', true);
    gate.cleanup();
    expect(endWork).not.toHaveBeenCalled();
  });

  it('releases only presence owned by the completed call', () => {
    const endWork = vi.fn();
    const gate = createAwarenessMutationGate({
      storeExists: () => false,
      queryTarget: vi.fn(),
      startWork: vi.fn(() => 'run_success'),
      endWork,
    });
    const write = event('write', { path: 'a.ts' });

    gate.preflight(write, cwd, 'me');
    gate.complete(write, cwd, 'me', true);

    expect(endWork).toHaveBeenCalledWith('/repo/a.ts', '/repo', 'me', 'run_success');
    gate.cleanup();
    expect(endWork).toHaveBeenCalledTimes(1);
  });

  it('releases failed mutations', () => {
    const endWork = vi.fn();
    const gate = createAwarenessMutationGate({
      storeExists: () => false,
      queryTarget: vi.fn(),
      startWork: vi.fn(() => 'run_failure'),
      endWork,
    });
    const write = event('write', { path: 'a.ts' });

    gate.preflight(write, cwd, 'me');
    gate.complete(write, cwd, 'me', false);

    expect(endWork).toHaveBeenCalledWith('/repo/a.ts', '/repo', 'me', 'run_failure');
  });

  it('keeps advisory presence until concurrent mutations of the same target settle', () => {
    const endWork = vi.fn();
    const gate = createAwarenessMutationGate({
      storeExists: () => false,
      queryTarget: vi.fn(),
      startWork: vi.fn(() => 'run_concurrent'),
      endWork,
    });
    const write = event('write', { path: 'a.ts' });

    gate.preflight(write, cwd, 'me');
    gate.preflight(write, cwd, 'me');
    gate.complete(write, cwd, 'me', true);
    expect(endWork).not.toHaveBeenCalled();
    gate.complete(write, cwd, 'me', true);
    expect(endWork).toHaveBeenCalledOnce();
    expect(endWork).toHaveBeenCalledWith('/repo/a.ts', '/repo', 'me', 'run_concurrent');
  });
});
