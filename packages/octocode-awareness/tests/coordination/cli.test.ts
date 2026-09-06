import { existsSync } from 'node:fs';
import { mkdtemp, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { extractHookTargetPaths, runPreEditLockGate } from '../../src/coordination/hooks.js';
import { isCliEntrypoint, runCli } from '../../src/coordination/cli.js';
import { writeWorkspacePolicy } from '../../src/workspace-policy.js';
import { openAwarenessStore } from '../../src/coordination/open.js';
import { canonicalizePath } from '../../src/git.js';

let workspace: string;
let stdout = '';
let outSpy: ReturnType<typeof vi.spyOn>;

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-lite-cli-'));
  writeWorkspacePolicy(workspace, { version: 1, storage: { repository: 'repo', memory: 'repo' }, hooks: { profile: 'full' } });
  stdout = '';
  outSpy = vi.spyOn(process.stdout, 'write').mockImplementation((chunk: string | Uint8Array) => {
    stdout += chunk.toString();
    return true;
  });
});

afterEach(async () => {
  outSpy.mockRestore();
  await rm(workspace, { recursive: true, force: true });
});

function jsonOut<T>(): T { return JSON.parse(stdout) as T; }

describe('coordination adapter commands', () => {
  it('recognizes symlinked executable paths', async () => {
    const realCli = join(workspace, 'cli.js');
    const linkedCli = join(workspace, 'octocode-awareness');
    await import('node:fs/promises').then((fs) => fs.writeFile(realCli, '#!/usr/bin/env node\n'));
    await symlink(realCli, linkedCli);
    expect(isCliEntrypoint(pathToFileURL(realCli).href, linkedCli)).toBe(true);
  });

  it('advertises only its unique host-owned routes', () => {
    expect(runCli(['help'])).toBe(0);
    expect(stdout).toContain('handoff add|list|clear');
    expect(stdout).toContain('agent touch|leave');
    expect(stdout).toContain('memory store-verified|recall-verified|evaluate|reindex|prune');
    expect(stdout).toContain('hooks pre-edit');
    expect(stdout).not.toContain('message send');
    expect(stdout).not.toContain('plan create');
  });

  it('keeps status and verified-memory operations in the adapter', () => {
    expect(runCli(['status', '--workspace', workspace])).toBe(0);
    expect(jsonOut<{ workspace: string }>().workspace).toMatch(/aw-lite-cli-/);
    stdout = '';
    expect(runCli(['memory', 'store-verified', '--workspace', workspace, '--label', 'SECURITY', '--text', 'single use receipt race', '--source-digest', 'sha256:auth', '--scope', 'project', '--verified-at', '2026-08-26T00:00:00.000Z', '--valid-until', '2027-01-01T00:00:00.000Z', '--importance', '9'])).toBe(0);
    expect(jsonOut<{ sourceDigest: string }>().sourceDigest).toBe('sha256:auth');
    stdout = '';
    expect(runCli(['memory', 'recall-verified', '--workspace', workspace, '--query', 'receipt', '--mode', 'hybrid', '--now', '2026-08-27T00:00:00.000Z'])).toBe(0);
    expect(jsonOut<Array<{ sourceDigest: string }>>()[0]?.sourceDigest).toBe('sha256:auth');
  });

  it('keeps agent presence touch and leave in the adapter', () => {
    expect(runCli(['agent', 'touch', '--workspace', workspace, '--agent-id', 'agent-a', '--status', 'IDLE'])).toBe(0);
    expect(jsonOut<{ status: string }>().status).toBe('IDLE');
    stdout = '';
    expect(runCli(['agent', 'leave', '--workspace', workspace, '--agent-id', 'agent-a'])).toBe(0);
    expect(jsonOut<{ status: string }>().status).toBe('LEFT');
  });

  it('extracts write targets and runs the JSON pre-edit gate', () => {
    expect(extractHookTargetPaths({ toolName: 'Write', input: { path: 'src/a.ts' } })).toEqual(['src/a.ts']);
    expect(extractHookTargetPaths({ tool_name: 'apply_patch', tool_input: { command: '*** Begin Patch\n*** Update File: src/b.ts\n*** Move to: src/c.ts\n*** End Patch' } })).toEqual(['src/b.ts', 'src/c.ts']);
    expect(runCli(['hooks', 'pre-edit', '--workspace', workspace, '--agent-id', 'agent-a', '--event-json', JSON.stringify({ toolName: 'Write', input: { path: 'src/a.ts' } })])).toBe(0);
    expect(jsonOut<{ blocked: boolean }>().blocked).toBe(false);
    expect(existsSync(join(workspace, '.claude', 'settings.json'))).toBe(false);
  });

  it('blocks peer edits through a symlinked workspace alias while allowing the lock owner', async () => {
    const alias = `${workspace}-alias`;
    await symlink(workspace, alias, 'dir');
    try {
      const store = openAwarenessStore({ workspace, scope: 'repo' });
      try {
        const lock = store.acquireLock({
          filePath: 'locked.ts',
          agentId: 'owner',
          reason: 'protect a non-mergeable migration',
          testPlan: 'inspect the protected file',
        });
        expect(lock.filePath).toBe(canonicalizePath(join(workspace, 'locked.ts')));
      } finally {
        store.close();
      }

      const event = { toolName: 'Write', input: { path: 'locked.ts' } };
      const blocked = runPreEditLockGate({ workspace: alias, scope: 'repo', agentId: 'peer', event });
      expect(blocked.blocked).toBe(true);
      expect(blocked.files).toEqual([canonicalizePath(join(workspace, 'locked.ts'))]);
      expect(blocked.conflicts[0]?.lock.agentId).toBe('owner');

      const owner = runPreEditLockGate({ workspace: alias, scope: 'repo', agentId: 'owner', event });
      expect(owner.blocked).toBe(false);
    } finally {
      await rm(alias, { recursive: true, force: true });
    }
  });

  it('rejects commands whose single owner is the root CLI', () => {
    for (const argv of [
      ['schema'], ['memory', 'store'], ['message', 'send'], ['agent', 'register'],
      ['plan', 'create'], ['task', 'create'], ['work', 'start'], ['lock', 'acquire'], ['check', 'mark'],
    ]) {
      expect(() => runCli([...argv, '--workspace', workspace])).toThrow(/not owned by this adapter/);
    }
  });
});
