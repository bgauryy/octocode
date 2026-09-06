import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { connectDb } from '../src/db-runtime.js';
import { insertMemory } from '../src/memory-write.js';
import { writeWorkspacePolicy } from '../src/workspace-policy.js';
import { withEnabledAwarenessConfig } from './helpers/enabled-awareness-config.js';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIST_DIR = resolve(PACKAGE_ROOT, 'out');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const SKILL_ROOT = resolve(PACKAGE_ROOT, 'skills/octocode-awareness');
const HOOKS_DIR = resolve(SKILL_ROOT, 'scripts/hooks');
const NODE = process.execPath;
let restoreLocalProcesses: (() => void) | undefined;
beforeEach(() => { restoreLocalProcesses = allowLocalFixtureProcesses(); });
afterEach(() => { restoreLocalProcesses?.(); });
function runScript(script: string, args: string[], payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
    const payloadWorkspace = (payload as { workspace?: unknown })?.workspace;
    if (typeof payloadWorkspace === 'string' && payloadWorkspace.startsWith(tmpdir())) {
        writeWorkspacePolicy(payloadWorkspace, {
            version: 1,
            storage: { repository: 'repo', memory: 'repo' },
            hooks: { profile: 'full' },
        });
    }
    return spawnSync(NODE, [script, ...args], {
        input: JSON.stringify(payload),
        encoding: 'utf8',
        timeout: 5000,
        cwd: cwd ?? (typeof payloadWorkspace === 'string' ? payloadWorkspace : undefined),
        env: { ...process.env, OCTOCODE_HOOK_PROFILE: 'full', ...withEnabledAwarenessConfig(env) },
    });
}
function runHookWrapper(
    name: string,
    payload: unknown,
    env: Record<string, string | undefined> = {},
    cwd?: string,
    hooksDir = HOOKS_DIR,
) {
    const payloadWorkspace = (payload as { workspace?: unknown })?.workspace;
    if (typeof payloadWorkspace === 'string' && payloadWorkspace.startsWith(tmpdir())) {
        writeWorkspacePolicy(payloadWorkspace, {
            version: 1,
            storage: { repository: 'repo', memory: 'repo' },
            hooks: { profile: 'full' },
        });
    }
    return spawnSync('/bin/sh', [resolve(hooksDir, name)], {
        input: JSON.stringify(payload),
        encoding: 'utf8',
        timeout: 5000,
        cwd: cwd ?? (typeof payloadWorkspace === 'string' ? payloadWorkspace : undefined),
        env: { ...process.env, ...withEnabledAwarenessConfig(env) },
    });
}

describe('hook-runner', () => {
it('signals periodic maintenance pressure without embedding details or mutating state', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-digest-preview-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const dbPath = join(workspace, '.octocode', 'awareness.sqlite3');
      const database = connectDb(dbPath);
      const inserted = insertMemory(database, {
        agentId: 'old-agent',
        taskContext: 'old superseded row',
        observation: 'must survive prompt-time preview',
        importance: 5,
        workspacePath: workspace,
      });
      database.prepare("UPDATE awareness_memories SET state = 'SUPERSEDED', updated_at = '2000-01-01T00:00:00Z' WHERE memory_id = ?")
        .run(inserted.memoryId);
      database.close();

      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_AGENT_ID: 'preview-agent',
        OCTOCODE_NOTIFY_RUN_DIGEST: '1',
        OCTOCODE_DIGEST_INTERVAL_HOURS: '4',
      };
      const first = runScript(HOOK_RUNNER, ['notify-deliver'], { workspace }, env);
      expect(first.status).toBe(0);
      expect(first.stdout).toContain('Awareness: maintenance 1.');
      expect(first.stdout).not.toContain('Maintenance pressure');
      expect(first.stdout).not.toContain('maintenance digest --dry-run');
      expect(Buffer.byteLength(first.stdout, 'utf8')).toBeLessThanOrEqual(256);

      const check = connectDb(dbPath);
      expect(check.prepare('SELECT state FROM awareness_memories WHERE memory_id = ?').get(inserted.memoryId))
        .toEqual({ state: 'SUPERSEDED' });
      check.close();

      const second = runScript(HOOK_RUNNER, ['notify-deliver'], { workspace }, env);
      expect(second.status).toBe(0);
      expect(second.stdout).not.toContain('Maintenance pressure');
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
it('prefers explicit payload agent ids over shared session ids', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-agent-id-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_NO_DIGEST: '1',
        OCTOCODE_AGENT_ID: '',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'shared-session', agent_id: 'subagent-a', workspace, file_path: 'src/sub.ts' },
        env,
      );
      expect(result.status).toBe(0);

      const db = new DatabaseSync(join(workspace, '.octocode', 'awareness.sqlite3'));
      expect(db.prepare(`SELECT rf.file_path, tr.agent_id
        FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id`).get()).toMatchObject({
        file_path: resolve(realpathSync(workspace), 'src/sub.ts'),
        agent_id: 'subagent-a',
      });
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
it('declares flat file_path hook payloads even when toolName is absent', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-flat-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_AGENT_ID: 'flat-hook-agent',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'flat-session', workspace, file_path: 'src/cursor.ts' },
        env,
      );
      expect(result.status).toBe(0);

      const db = new DatabaseSync(join(workspace, '.octocode', 'awareness.sqlite3'));
      expect(db.prepare(`SELECT rf.file_path, tr.agent_id
        FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id`).get()).toMatchObject({
        file_path: resolve(realpathSync(workspace), 'src/cursor.ts'),
        agent_id: 'flat-hook-agent',
      });
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
it('declares mixed root file_path payloads even when input contains unrelated metadata', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-mixed-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_AGENT_ID: 'mixed-hook-agent',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'mixed-session', workspace, file_path: 'src/mixed.ts', input: { eventId: 'evt-1' } },
        env,
      );
      expect(result.status).toBe(0);

      const db = new DatabaseSync(join(workspace, '.octocode', 'awareness.sqlite3'));
      expect(db.prepare(`SELECT rf.file_path, tr.agent_id
        FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id`).get()).toMatchObject({
        file_path: resolve(realpathSync(workspace), 'src/mixed.ts'),
        agent_id: 'mixed-hook-agent',
      });
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
it('post-edit keeps one correlated same-agent aggregate until Stop finalizes it', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-overlap-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_AGENT_ID: 'overlap-hook-agent',
      };
      const first = { sessionId: 'overlap-session', workspace, eventId: 'tool-1', file_path: 'src/shared.ts' };
      const second = { sessionId: 'overlap-session', workspace, eventId: 'tool-2', file_path: 'src/shared.ts' };

      expect(runScript(HOOK_RUNNER, ['pre-edit'], first, env).status).toBe(0);
      expect(runScript(HOOK_RUNNER, ['pre-edit'], second, env).status).toBe(0);

      const db = new DatabaseSync(join(workspace, '.octocode', 'awareness.sqlite3'));
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(1);
      expect((db.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get() as { count: number }).count).toBe(1);

      expect(runScript(HOOK_RUNNER, ['post-edit'], first, env).status).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(1);

      expect(runScript(HOOK_RUNNER, ['post-edit'], second, env).status).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(1);
      expect(runScript(HOOK_RUNNER, ['stop-verify'], { sessionId: 'overlap-session', workspace }, env).status).toBe(2);
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(0);
      expect((db.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE status = 'PENDING'").get() as { count: number }).count).toBe(1);
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });
it('stores shell hook run correlation in per-key files', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-state-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_AGENT_ID: 'state-hook-agent',
      };
      const first = { sessionId: 'state-session', workspace, eventId: 'tool-1', file_path: 'src/a.ts' };
      const second = { sessionId: 'state-session', workspace, eventId: 'tool-2', file_path: 'src/b.ts' };

      expect(runScript(HOOK_RUNNER, ['pre-edit'], first, env).status).toBe(0);
      expect(runScript(HOOK_RUNNER, ['pre-edit'], second, env).status).toBe(0);

      const stateDir = join(workspace, '.octocode', 'hook-state', 'runs');
      const stateFiles = readdirSync(stateDir).filter((file) => file.endsWith('.json'));
      expect(stateFiles).toHaveLength(2);
      expect(existsSync(join(workspace, '.octocode', 'hook-state', 'shell-hook-tasks.json'))).toBe(false);

      expect(runScript(HOOK_RUNNER, ['post-edit'], first, env).status).toBe(0);
      expect(readdirSync(stateDir).filter((file) => file.endsWith('.json'))).toHaveLength(1);
      expect(runScript(HOOK_RUNNER, ['post-edit'], second, env).status).toBe(0);
      expect(readdirSync(stateDir).filter((file) => file.endsWith('.json'))).toHaveLength(0);
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

});

describe('hook wrapper scripts', () => {
  it('pre-edit.sh and post-edit.sh dispatch through hook-runner.mjs', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-wrapper-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_AGENT_DIR: memoryHome,
        OCTOCODE_AGENT_ID: 'wrapper-agent',
      };
      const payload = { sessionId: 'wrapper-session', workspace, file_path: 'src/wrapped.ts' };

      const pre = runHookWrapper('pre-edit.sh', payload, env, workspace);
      expect(pre.status, pre.stderr).toBe(0);

      const db = new DatabaseSync(join(workspace, '.octocode', 'awareness.sqlite3'));
      expect(db.prepare(`SELECT rf.file_path, tr.agent_id
        FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id WHERE rf.ended_at IS NULL`).get()).toMatchObject({
        file_path: resolve(realpathSync(workspace), 'src/wrapped.ts'),
        agent_id: 'wrapper-agent',
      });

      const post = runHookWrapper('post-edit.sh', payload, env, workspace);
      expect(post.status, post.stderr).toBe(0);

      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(1);
      expect(db.prepare('SELECT origin, status FROM task_runs').get()).toMatchObject({ origin: 'HOOK', status: 'ACTIVE' });
      const stop = runHookWrapper('stop-verify.sh', { sessionId: 'wrapper-session', workspace }, env, workspace);
      expect(stop.status).toBe(2);
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(0);
      expect(db.prepare('SELECT origin, status FROM task_runs').get()).toMatchObject({ origin: 'HOOK', status: 'PENDING' });
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('pre-edit.sh guards before presence without a second host hook', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-wrapper-guard-first-'));
    const guardedSkillRoot = join(memoryHome, 'octocode-awareness');
    const guardedScripts = join(guardedSkillRoot, 'scripts');
    const guardedHooks = join(guardedScripts, 'hooks');
    mkdirSync(guardedHooks, { recursive: true });
    cpSync(join(SKILL_ROOT, 'scripts', 'hook-runner.mjs'), join(guardedScripts, 'hook-runner.mjs'));
    cpSync(join(HOOKS_DIR, 'pre-edit.sh'), join(guardedHooks, 'pre-edit.sh'));
    try {
      const result = runHookWrapper(
        'pre-edit.sh',
        { tool_name: 'Edit', workspace: guardedSkillRoot, tool_input: { file_path: 'SKILL.md' } },
        {
          OCTOCODE_AGENT_DIR: memoryHome,
          OCTOCODE_AGENT_ID: 'guarded-wrapper-agent',
          OCTOCODE_SKILL_ROOT: undefined,
          OCTOCODE_ALLOW_HARNESS_APPLY: undefined,
        },
        guardedSkillRoot,
        guardedHooks,
      );
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('editing the skill itself is gated');
      const inspect = new DatabaseSync(join(guardedSkillRoot, '.octocode', 'awareness.sqlite3'));
      expect((inspect.prepare('SELECT COUNT(*) AS count FROM task_runs').get() as { count: number }).count).toBe(0);
      expect(inspect.prepare('SELECT event, status FROM hook_receipts').get()).toMatchObject({ event: 'pre-edit', status: 'success' });
      inspect.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('hook wrappers warn when hook-runner.mjs is missing', () => {
    const tempRoot = mkdtempSync(join(tmpdir(), 'octocode-missing-runner-'));
    const tempHooks = join(tempRoot, 'scripts', 'hooks');
    mkdirSync(tempHooks, { recursive: true });
    try {
      cpSync(resolve(HOOKS_DIR, 'pre-edit.sh'), join(tempHooks, 'pre-edit.sh'));
      const result = spawnSync('/bin/sh', [join(tempHooks, 'pre-edit.sh')], {
        input: JSON.stringify({ file_path: 'src/missing-runner.ts' }),
        encoding: 'utf8',
        timeout: 5000,
      });
      expect(result.status).toBe(0);
      expect(result.stderr).toContain('missing hook runner');
      expect(result.stderr).toContain('pre-edit hook skipped');
    } finally {
      rmSync(tempRoot, { recursive: true, force: true });
    }
  });
});
