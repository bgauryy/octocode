import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import { runHookCommand } from '../src/hooks/runner.js';
import { resolveDbPath } from '../src/db-runtime.js';
import { writeWorkspacePolicy } from '../src/workspace-policy.js';

const roots: string[] = [];
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

it('rechecks due maintenance on an unchanged ledger and prompt, but respects its interval', async () => {
  const root = mkdtempSync(join(tmpdir(), 'awareness-nudge-due-'));
  roots.push(root);
  const workspace = join(root, 'repo');
  mkdirSync(workspace);
  vi.stubEnv('OCTOCODE_HOME', root);
  vi.stubEnv('OCTOCODE_AGENT_ID', 'nudge-tester');
  vi.stubEnv('OCTOCODE_NO_NOTIFY', '0');
  vi.stubEnv('OCTOCODE_NO_RETENTION_REMINDER', '0');
  vi.stubEnv('OCTOCODE_RETENTION_REMINDER_INTERVAL_HOURS', '4');
  vi.stubEnv('OCTOCODE_HOOK_PROFILE', 'full');
  writeFileSync(join(root, 'awareness.json'), JSON.stringify({ version: 1, features: {
    hooks: true, notifications: false, verificationGate: false, sessionCapture: false, maintenanceReminders: true,
  } }));
  writeWorkspacePolicy(workspace, { version: 1, storage: { repository: 'repo', memory: 'repo' }, hooks: { profile: 'full' } });
  const clock = vi.spyOn(Date, 'now').mockReturnValue(1_000_000);
  const payload = JSON.stringify({ workspace, session_id: 'nudge-session', prompt: 'same prompt' });
  const hash = createHash('sha256').update(workspace).digest('hex').slice(0, 12);
  const marker = join(dirname(resolveDbPath(null)), `.last-retention-preview-${hash}-epoch-ms`);

  expect(await runHookCommand('notify-deliver', payload)).toBe(0);
  expect(readFileSync(marker, 'utf8')).toBe('1000000');
  clock.mockReturnValue(1_000_001);
  expect(await runHookCommand('notify-deliver', payload)).toBe(0);
  expect(readFileSync(marker, 'utf8')).toBe('1000000');
  clock.mockReturnValue(1_000_000 + 4 * 3600_000);
  expect(await runHookCommand('notify-deliver', payload)).toBe(0);
  expect(readFileSync(marker, 'utf8')).toBe(String(1_000_000 + 4 * 3600_000));
  // A wall-clock rollback must not suppress reminders until the old future time.
  clock.mockReturnValue(500_000);
  expect(await runHookCommand('notify-deliver', payload)).toBe(0);
  expect(readFileSync(marker, 'utf8')).toBe('500000');
  vi.stubEnv('OCTOCODE_NO_RETENTION_REMINDER', '1');
  clock.mockReturnValue(500_000 + 4 * 3600_000);
  expect(await runHookCommand('notify-deliver', payload)).toBe(0);
  expect(readFileSync(marker, 'utf8')).toBe('500000');
});
