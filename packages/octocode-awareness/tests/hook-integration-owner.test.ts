import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { afterEach, expect, it, vi } from 'vitest';
import { resolveDbPath } from '../src/db-runtime.js';
import { runHookCommand } from '../src/hooks/runner.js';
import { claimNativeHookOwner, writeWorkspacePolicy } from '../src/workspace-policy.js';

const roots: string[] = [];
afterEach(() => {
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

it('keeps the shell runner inert before identity or database work when native owns the host', async () => {
  const root = mkdtempSync(join(tmpdir(), 'awareness-native-owner-'));
  roots.push(root);
  const workspace = join(root, 'repo');
  vi.stubEnv('OCTOCODE_HOME', root);
  writeWorkspacePolicy(workspace, {
    version: 1,
    storage: { repository: 'repo', memory: 'repo' },
    hooks: { profile: 'full' },
  });
  claimNativeHookOwner({ workspace, host: 'codex' });
  const databasePath = resolveDbPath(null, { workspace, scope: 'repo' });

  await expect(runHookCommand('session-end', JSON.stringify({
    cwd: workspace,
    hook_event_name: 'SessionEnd',
  }), { host: 'codex' })).resolves.toBe(0);

  expect(existsSync(databasePath)).toBe(false);
});

it('retains identity enforcement for the shell-owned host', async () => {
  const root = mkdtempSync(join(tmpdir(), 'awareness-shell-owner-'));
  roots.push(root);
  const workspace = join(root, 'repo');
  vi.stubEnv('OCTOCODE_HOME', root);
  vi.stubEnv('OCTOCODE_AGENT_ID', '');
  writeWorkspacePolicy(workspace, {
    version: 1,
    storage: { repository: 'repo', memory: 'repo' },
    hooks: { profile: 'full' },
  });

  await expect(runHookCommand('session-end', JSON.stringify({
    cwd: workspace,
    hook_event_name: 'SessionEnd',
  }), { host: 'codex' })).resolves.toBe(1);
});
