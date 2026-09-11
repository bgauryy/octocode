import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { runHookCommand } from '../src/hooks/runner.js';
import { captureHookHistory } from '../src/hooks/history-capture.js';
import { connectDb, resolveDbPath } from '../src/db-runtime.js';
import { agentSignal } from '../src/notifications-signals.js';
import { specsFor, awarenessHookName } from '../src/hooks-install-health.js';

// History is an optional subscriber. The default profile must never invoke it.
vi.mock('../src/hooks/history-capture.js', () => ({ captureHookHistory: vi.fn() }));
afterEach(() => { vi.unstubAllEnvs(); vi.restoreAllMocks(); });

describe('coordination defaults', () => {
  it('retains a message across a hook with no context channel', async () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-deferred-message-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_AGENT_ID', 'owner');
    vi.stubEnv('OCTOCODE_HOOK_PROFILE', 'coordination');
    vi.stubEnv('OCTOCODE_NO_NOTIFY', '0');
    const db = connectDb(resolveDbPath(null, { workspace: home }));
    const out = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const payload = { cwd: home, agent_id: 'owner', session_id: 'session' };
    try {
      agentSignal(db, { action: 'publish', agentId: 'peer', workspacePath: home, cwd: home,
        kind: 'question', toAgents: ['owner'], subject: 'Parser', body: 'Keep this for the supported event.' });
      await runHookCommand('post-edit', JSON.stringify({ ...payload, hook_event_name: 'postToolUseFailure' }), { host: 'cursor' });
      expect(out).not.toHaveBeenCalled();
      await runHookCommand('notify-deliver', JSON.stringify({ ...payload, hook_event_name: 'sessionStart' }), { host: 'cursor' });
      expect(out.mock.calls.map(([s]) => String(s)).join('')).toContain('Keep this for the supported event.');
    } finally { db.close(); rmSync(home, { recursive: true, force: true }); }
  });
  it('fails open when the workspace hook policy is malformed', async () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-invalid-hook-policy-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_AGENT_ID', 'owner');
    // A policy parse error must not escape the host hook boundary.
    mkdirSync(join(home, '.octocode'));
    writeFileSync(join(home, '.octocode', 'awareness.json'), '{ malformed', 'utf8');
    const out = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    try {
      await expect(runHookCommand('notify-deliver', JSON.stringify({ cwd: home, session_id: 'session' }), { host: 'codex' })).resolves.toBe(0);
      expect(out.mock.calls.map(([s]) => String(s)).join('')).toContain('hooks inert');
    } finally { rmSync(home, { recursive: true, force: true }); }
  });
  it.each(['claude', 'codex', 'cursor', 'copilot', 'gemini'] as const)('installs only communication and departure hooks for %s', host => {
    const specs = specsFor(host, { globalMode: false, projectDir: '/repo', hookDir: '/repo/skills/octocode-awareness/scripts/hooks' });
    const commands = new Set(specs.map(spec => awarenessHookName(spec.command)));
    expect(commands).toEqual(new Set(['notify-deliver.sh', 'session-end.sh']));
    expect(specs.every(spec => spec.matcher === undefined)).toBe(true);
  });

  it('uses installed hooks without onboarding and delivers messages without work, memory, history or finalization ceremonies', async () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-lean-hooks-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_AGENT_ID', 'owner');
    vi.stubEnv('OCTOCODE_HOOK_PROFILE', 'coordination');
    vi.stubEnv('OCTOCODE_NO_NOTIFY', '0');
    const db = connectDb(resolveDbPath(null, { workspace: home }));
    const out = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const payload = { cwd: home, agent_id: 'owner', session_id: 'lean-session', tool_name: 'Write', tool_use_id: 'edit-1', tool_input: { file_path: 'a.ts' } };
    try {
      await runHookCommand('notify-deliver', JSON.stringify({ ...payload, hook_event_name: 'SessionStart' }), { host: 'codex' });
      expect(out).not.toHaveBeenCalled();
      agentSignal(db, { action: 'publish', agentId: 'peer', workspacePath: home, cwd: home,
        kind: 'question', toAgents: ['owner'], subject: 'Ownership', body: 'Are you editing the parser?' });
      const prepare = vi.spyOn(db, 'prepare');
      await runHookCommand('pre-edit', JSON.stringify({ ...payload, hook_event_name: 'PreToolUse' }), { host: 'codex' });
      await runHookCommand('post-edit', JSON.stringify({ ...payload, hook_event_name: 'PostToolUse' }), { host: 'codex' });
      expect(out).not.toHaveBeenCalled();
      await runHookCommand('notify-deliver', JSON.stringify({ ...payload, hook_event_name: 'UserPromptSubmit' }), { host: 'codex' });
      expect(out.mock.calls.map(([s]) => String(s)).join('')).toContain('Are you editing the parser?');
      const deliveries = out.mock.calls.length;
      await runHookCommand('notify-deliver', JSON.stringify({ ...payload, hook_event_name: 'UserPromptSubmit' }), { host: 'codex' });
      expect(out).toHaveBeenCalledTimes(deliveries);
      await runHookCommand('stop-verify', JSON.stringify(payload), { host: 'codex' });
      await runHookCommand('session-compact', JSON.stringify(payload), { host: 'codex' });
      await runHookCommand('session-end', JSON.stringify(payload), { host: 'codex' });
      expect(prepare.mock.calls.some(([sql]) => /awareness_memories|task_runs|run_files|edit_log/.test(sql))).toBe(false);
      expect(captureHookHistory).not.toHaveBeenCalled();
      expect(db.prepare('SELECT count(*) AS count FROM task_runs').get()).toEqual({ count: 0 });
      expect(db.prepare("SELECT status FROM awareness_agents WHERE agent_id = 'owner'").get()).toEqual({ status: 'LEFT' });
      await runHookCommand('notify-deliver', JSON.stringify({ ...payload, session_id: 'resumed-session', hook_event_name: 'SessionStart' }), { host: 'codex' });
      expect(db.prepare("SELECT status FROM awareness_agents WHERE agent_id = 'owner'").get()).toEqual({ status: 'ACTIVE' });
    } finally { db.close(); rmSync(home, { recursive: true, force: true }); }
  });
});
