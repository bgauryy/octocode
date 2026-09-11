import { afterEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { writeWorkspacePolicy } from '../../octocode-awareness/src/workspace-policy.js';
import { createAwarenessMutationGate } from '../src/tools/awareness-mutation-gate.js';
import { createPiHistoryAdapter } from '../src/adapters/pi-history-adapter.js';
import { inspectWorkerAwarenessAutomatically } from '../src/tools/awareness-worker-audit.js';

// These tests exercise default subscriber selection with real workspace policy.
// Shell parsing, persistent store creation, and history execution are separate boundaries.
vi.mock('../src/tools/bash-tool.js', () => ({ extractBashWriteTargets: vi.fn(() => { throw new Error('not a shell fixture'); }) }));
vi.mock('../src/tools/storage-policy.js', () => ({ openPersistentAwareness: vi.fn(() => { throw new Error('unexpected store open'); }) }));
vi.mock('@octocodeai/octocode-awareness/host', async () => ({
  ...await import('../../octocode-awareness/src/workspace-policy.js'),
  ...await import('../../octocode-awareness/src/history-tool-effects.js'),
}));
const dirs: string[] = [];
afterEach(() => { vi.unstubAllEnvs(); for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true }); });
function workspace() { const dir = mkdtempSync(join(tmpdir(), 'pi-awareness-defaults-')); dirs.push(dir); return dir; }

describe('lean native Awareness subscribers', () => {
  it('enforces existing locks without creating per-edit records', () => {
    const startWork = vi.fn(); const endWork = vi.fn();
    const queryTarget = vi.fn(() => ({ blocked: false }));
    const gate = createAwarenessMutationGate({ enabled: () => true, trackWork: () => false,
      storeExists: () => true, queryTarget, startWork, endWork });
    const event = { toolName: 'file', input: { queries: [{ type: 'write', path: 'a.ts' }] } };
    expect(gate.preflight(event, '/repo', 'owner')).toBeUndefined();
    gate.complete(event, '/repo', 'owner', true);
    gate.cleanup();
    expect(queryTarget).toHaveBeenCalledOnce();
    expect(startWork).not.toHaveBeenCalled(); expect(endWork).not.toHaveBeenCalled();
    queryTarget.mockReturnValue({ blocked: true });
    expect(gate.preflight(event, '/repo', 'owner')?.block).toBe(true);
  });

  it('skips default history and automatic worker audits, while full profile captures paired history', async () => {
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'disk');
    const cwd = workspace();
    const captureHistory = vi.fn(async (
      _input: import('@octocodeai/octocode-awareness/host').AwarenessHistoryCaptureInput,
    ) => ({ ok: true, operation: {} }));
    const adapter = createPiHistoryAdapter({
      createHost: context => ({ context, captureHistory }),
      agentId: () => 'owner',
    });
    const event = { toolCallId: 'edit', toolName: 'file', input: { queries: [{ type: 'write', path: 'a.ts' }] } };
    const ctx = { cwd } as never;
    await adapter.before(event, ctx);
    expect(captureHistory).not.toHaveBeenCalled();
    expect(inspectWorkerAwarenessAutomatically({ awarenessAgentId: 'worker', awarenessWorkspace: cwd })).toBeUndefined();
    writeWorkspacePolicy(cwd, { version: 1, storage: { repository: 'global', memory: 'global' }, hooks: { profile: 'full' } });
    await adapter.before(event, ctx);
    await adapter.after({ toolCallId: 'edit', toolName: 'file', result: {}, isError: false }, ctx);
    expect(captureHistory).toHaveBeenCalledTimes(2);
    expect(captureHistory.mock.calls[0]?.[0]).toEqual(expect.objectContaining({ phase: 'before' }));
    expect(captureHistory.mock.calls[1]?.[0]).toEqual(expect.objectContaining({ phase: 'after', outcome: 'success' }));
    expect(adapter.pending()).toBe(0);
  });
});
