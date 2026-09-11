import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import { connectDb, resolveDbPath } from '../src/db-runtime.js';
import { hookReceipts } from '../src/hook-receipts.js';
import { runHookCommand } from '../src/hooks/runner.js';
import { writeWorkspacePolicy } from '../src/workspace-policy.js';

vi.mock('../src/sessions.js', async importOriginal => {
  const actual = await importOriginal<typeof import('../src/sessions.js')>();
  return {
    ...actual,
    endSession: vi.fn(() => { throw new Error('injected session finalization failure'); }),
  };
});

const roots: string[] = [];
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

it('fails open for the host while recording degraded lifecycle telemetry', async () => {
  const root = mkdtempSync(join(tmpdir(), 'awareness-hook-fail-open-'));
  roots.push(root);
  const workspace = join(root, 'repo');
  mkdirSync(workspace);
  vi.stubEnv('OCTOCODE_HOME', root);
  vi.stubEnv('OCTOCODE_AGENT_ID', 'hook-fail-open-agent');
  vi.stubEnv('OCTOCODE_HOOK_PROFILE', 'coordination');
  writeWorkspacePolicy(workspace, {
    version: 1,
    storage: { repository: 'repo', memory: 'repo' },
    hooks: { profile: 'coordination' },
  });
  const stderr = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);

  await expect(runHookCommand('session-end', JSON.stringify({
    cwd: workspace,
    session_id: 'broken-session',
    hook_event_name: 'SessionEnd',
  }), { host: 'codex' })).resolves.toBe(0);

  const database = connectDb(resolveDbPath(null, { workspace, scope: 'repo' }));
  expect(hookReceipts(database, workspace, 'codex')).toContainEqual(expect.objectContaining({
    event: 'SessionEnd',
    status: 'degraded',
  }));
  expect(stderr.mock.calls.map(([message]) => String(message)).join(''))
    .toContain('session-end warning (continuing): injected session finalization failure');
  database.close();
});
