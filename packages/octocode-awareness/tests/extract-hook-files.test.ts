import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { connectDb } from '../src/db.js';
import { createPlan } from '../src/plans.js';
import { claimTask, createTask } from '../src/tasks.js';
import { startWork } from '../src/work.js';

const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin');
const SCRIPT = resolve(DIST_DIR, 'extract-hook-files.js');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const AWARENESS = resolve(DIST_DIR, 'awareness.js');
const SKILL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness');
const HOOKS_DIR = resolve(SKILL_ROOT, 'scripts/hooks');
const NODE = process.execPath;

function runScript(script: string, args: string[], payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
  return spawnSync(NODE, [script, ...args], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 5000,
    cwd,
    env: { ...process.env, ...env },
  });
}

function extract(payload: unknown): string[] {
  const result = runScript(SCRIPT, [], payload);
  expect(result.status).toBe(0);
  return result.stdout.trim() ? result.stdout.trim().split('\n') : [];
}

function runHookWrapper(name: string, payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
  return spawnSync(resolve(HOOKS_DIR, name), [], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 5000,
    cwd,
    env: { ...process.env, ...env },
  });
}

describe('extract-hook-files', () => {
  it('supports Claude tool_input payloads', () => {
    expect(extract({ tool_input: { file_path: 'src/a.ts', file_paths: ['src/b.ts'] } })).toEqual(['src/a.ts', 'src/b.ts']);
  });

  it('supports Pi tool event input payloads', () => {
    expect(extract({ toolName: 'write', input: { path: 'src/pi.ts' } })).toEqual(['src/pi.ts']);
  });

  it('supports Cursor flat file payloads', () => {
    expect(extract({ event_name: 'afterFileEdit', file_path: 'src/cursor.ts' })).toEqual(['src/cursor.ts']);
  });

  it('keeps Cursor flat file payloads when input contains unrelated metadata', () => {
    expect(extract({ event_name: 'afterFileEdit', file_path: 'src/mixed.ts', input: { eventId: 'evt-1' } })).toEqual(['src/mixed.ts']);
  });

  it('supports Pi args payloads and apply_patch paths', () => {
    expect(extract({ args: { command: '*** Begin Patch\n*** Add File: src/new.ts\n*** Move to: src/moved.ts\n*** End Patch' } })).toEqual([
      'src/new.ts',
      'src/moved.ts',
    ]);
  });
});

describe('hook-runner', () => {
  it('allows two agents to declare ordinary work on the same file without locks', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-presence-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const payload = { workspace, file_path: 'src/shared.ts' };
      const first = runScript(HOOK_RUNNER, ['pre-edit'], payload, {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'agent-a',
      });
      const second = runScript(HOOK_RUNNER, ['pre-edit'], payload, {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'agent-b',
      });

      expect(first.status, first.stderr).toBe(0);
      expect(second.status, second.stderr).toBe(0);
      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(2);
      expect((db.prepare('SELECT COUNT(*) AS count FROM locks').get() as { count: number }).count).toBe(0);
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('blocks shell edits when another run holds sensitive exclusive work', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-exclusive-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const db = connectDb(join(memoryHome, 'awareness.sqlite3'));
      const exclusive = startWork(db, {
        agentId: 'sensitive-agent',
        workspacePath: workspace,
        targetFiles: ['src/schema.ts'],
        rationale: 'change shared schema',
        testPlan: 'run migration tests',
        origin: 'WORK',
        source: 'EXPLICIT',
        exclusive: true,
      });
      expect(exclusive.ok).toBe(true);
      db.close();

      const blocked = runScript(HOOK_RUNNER, ['pre-edit'], { workspace, file_path: 'src/schema.ts' }, {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'other-agent',
      });
      expect(blocked.status).toBe(2);
      expect(blocked.stderr).toContain('exclusive file work');
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('attaches shell edits to exactly one claimed TASK run', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-task-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const db = connectDb(join(memoryHome, 'awareness.sqlite3'));
      const plan = createPlan(db, {
        name: 'Shell task hooks',
        objective: 'Keep hook edits on the claim',
        leadAgentId: 'lead',
        workspacePath: workspace,
      }).plan;
      const task = createTask(db, {
        planId: plan.plan_id,
        title: 'Edit files',
        reasoning: 'Exercise shell task attachment',
        paths: ['src/a.ts'],
        createdBy: 'lead',
      }).task;
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'task-agent' });
      expect(claim.ok).toBe(true);
      if (!claim.ok) throw new Error(claim.error);
      db.close();

      const env = { OCTOCODE_MEMORY_HOME: memoryHome, OCTOCODE_AGENT_ID: 'task-agent' };
      for (const [index, file] of ['src/a.ts', 'src/b.ts'].entries()) {
        const payload = { workspace, eventId: `task-${index}`, file_path: file };
        expect(runScript(HOOK_RUNNER, ['pre-edit'], payload, env).status).toBe(0);
        expect(runScript(HOOK_RUNNER, ['post-edit'], payload, env).status).toBe(0);
      }

      const inspect = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect((inspect.prepare('SELECT COUNT(*) AS count FROM task_runs').get() as { count: number }).count).toBe(1);
      expect((inspect.prepare('SELECT COUNT(*) AS count FROM task_claims').get() as { count: number }).count).toBe(1);
      expect((inspect.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL').get(claim.run.run_id) as { count: number }).count).toBe(2);
      expect(inspect.prepare('SELECT origin, status FROM task_runs WHERE run_id = ?').get(claim.run.run_id)).toMatchObject({ origin: 'TASK', status: 'ACTIVE' });
      inspect.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('keeps an explicit WORK run active across shell edits', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-work-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const db = connectDb(join(memoryHome, 'awareness.sqlite3'));
      const explicit = startWork(db, {
        agentId: 'work-agent',
        workspacePath: workspace,
        targetFiles: ['src/a.ts'],
        rationale: 'explicit work',
        testPlan: 'focused test',
        origin: 'WORK',
        source: 'EXPLICIT',
      });
      expect(explicit.ok).toBe(true);
      if (!explicit.ok) throw new Error('unexpected conflict');
      db.close();

      const env = { OCTOCODE_MEMORY_HOME: memoryHome, OCTOCODE_AGENT_ID: 'work-agent' };
      for (let index = 0; index < 2; index += 1) {
        const payload = { workspace, eventId: `work-${index}`, file_path: 'src/a.ts' };
        expect(runScript(HOOK_RUNNER, ['pre-edit'], payload, env).status).toBe(0);
        expect(runScript(HOOK_RUNNER, ['post-edit'], payload, env).status).toBe(0);
      }

      const inspect = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect(inspect.prepare('SELECT origin, status FROM task_runs WHERE run_id = ?').get(explicit.run.run_id)).toMatchObject({ origin: 'WORK', status: 'ACTIVE' });
      expect((inspect.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL').get(explicit.run.run_id) as { count: number }).count).toBe(1);
      inspect.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('runs the harness guard before declaring file work', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-guard-first-'));
    const skillRoot = resolve(memoryHome, 'skill');
    mkdirSync(skillRoot, { recursive: true });
    try {
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { workspace: skillRoot, file_path: 'SKILL.md' },
        {
          OCTOCODE_MEMORY_HOME: memoryHome,
          OCTOCODE_AGENT_ID: 'guarded-agent',
          OCTOCODE_SKILL_ROOT: skillRoot,
          OCTOCODE_ALLOW_HARNESS_APPLY: undefined,
        },
        skillRoot,
      );

      expect(result.status).toBe(2);
      expect(result.stderr).toContain('editing the skill itself is gated');
      expect(existsSync(join(memoryHome, 'awareness.sqlite3'))).toBe(false);
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('emits a compact peer delta once and stays silent while peers are unchanged', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-peer-delta-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const base = { workspace, file_path: 'src/shared.ts' };
      expect(runScript(HOOK_RUNNER, ['pre-edit'], { ...base, eventId: 'a-1' }, {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'agent-a',
      }).status).toBe(0);

      const first = runScript(HOOK_RUNNER, ['pre-edit'], { ...base, eventId: 'b-1' }, {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'agent-b',
      });
      const unchanged = runScript(HOOK_RUNNER, ['pre-edit'], { ...base, eventId: 'b-2' }, {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'agent-b',
      });

      expect(first.status).toBe(0);
      expect(`${first.stdout}${first.stderr}`).toContain('AWARE');
      expect(unchanged.status).toBe(0);
      expect(`${unchanged.stdout}${unchanged.stderr}`).toBe('');
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('caps stop verification detail at three runs and reports omissions', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-stop-cap-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'stop-agent',
      };
      for (let index = 0; index < 5; index += 1) {
        const payload = { workspace, eventId: `tool-${index}`, file_path: `src/${index}.ts` };
        expect(runScript(HOOK_RUNNER, ['pre-edit'], payload, env).status).toBe(0);
        expect(runScript(HOOK_RUNNER, ['post-edit'], payload, env).status).toBe(0);
      }

      const stop = runScript(HOOK_RUNNER, ['stop-verify'], { workspace }, env);
      expect(stop.status).toBe(2);
      expect((stop.stderr.match(/PENDING:run_/g) ?? [])).toHaveLength(3);
      expect(stop.stderr).toContain('+2 omitted');
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('owns hook dispatch logic outside the skill wrapper scripts', () => {
    const result = runScript(HOOK_RUNNER, ['notify-deliver'], { sessionId: 'agent-a', workspace: process.cwd() });
    expect(result.status).toBe(0);
    if (result.stdout.trim()) {
      expect(() => JSON.parse(result.stdout)).not.toThrow();
    }
  });

  it('registers hook agents before checking mailbox delivery', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-agent-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'hook-agent',
        OCTOCODE_AGENT_NAME: 'Hook Agent',
        OCTOCODE_AGENT_CONTEXT: 'codex-hook',
        OCTOCODE_NO_DIGEST: '1',
      };
      const result = runScript(HOOK_RUNNER, ['notify-deliver'], { sessionId: 'session-a', workspace }, env);
      expect(result.status).toBe(0);

      const listed = spawnSync(NODE, [
        AWARENESS,
        'agent',
        'list',
        '--workspace',
        workspace,
      ], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_MEMORY_HOME: memoryHome },
      });
      expect(listed.status).toBe(0);
      const parsed = JSON.parse(listed.stdout) as { agents: Array<Record<string, unknown>> };
      expect(parsed.agents).toEqual(expect.arrayContaining([
        expect.objectContaining({
          agent_id: 'hook-agent',
          agent_name: 'Hook Agent',
          workspace_path: realpathSync(workspace),
          context: 'codex-hook',
        }),
      ]));
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
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_NO_DIGEST: '1',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'shared-session', agent_id: 'subagent-a', workspace, file_path: 'src/sub.ts' },
        env,
      );
      expect(result.status).toBe(0);

      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
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
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'flat-hook-agent',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'flat-session', workspace, file_path: 'src/cursor.ts' },
        env,
      );
      expect(result.status).toBe(0);

      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
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
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'mixed-hook-agent',
      };
      const result = runScript(
        HOOK_RUNNER,
        ['pre-edit'],
        { sessionId: 'mixed-session', workspace, file_path: 'src/mixed.ts', input: { eventId: 'evt-1' } },
        env,
      );
      expect(result.status).toBe(0);

      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
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

  it('post-edit ends only the correlated same-agent HOOK presence', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-overlap-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'overlap-hook-agent',
      };
      const first = { sessionId: 'overlap-session', workspace, eventId: 'tool-1', file_path: 'src/shared.ts' };
      const second = { sessionId: 'overlap-session', workspace, eventId: 'tool-2', file_path: 'src/shared.ts' };

      expect(runScript(HOOK_RUNNER, ['pre-edit'], first, env).status).toBe(0);
      expect(runScript(HOOK_RUNNER, ['pre-edit'], second, env).status).toBe(0);

      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(2);

      expect(runScript(HOOK_RUNNER, ['post-edit'], first, env).status).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(1);

      expect(runScript(HOOK_RUNNER, ['post-edit'], second, env).status).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(0);
      expect((db.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE status = 'PENDING'").get() as { count: number }).count).toBe(2);
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
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'state-hook-agent',
      };
      const first = { sessionId: 'state-session', workspace, eventId: 'tool-1', file_path: 'src/a.ts' };
      const second = { sessionId: 'state-session', workspace, eventId: 'tool-2', file_path: 'src/b.ts' };

      expect(runScript(HOOK_RUNNER, ['pre-edit'], first, env).status).toBe(0);
      expect(runScript(HOOK_RUNNER, ['pre-edit'], second, env).status).toBe(0);

      const stateDir = join(memoryHome, 'hook-state', 'runs');
      const stateFiles = readdirSync(stateDir).filter((file) => file.endsWith('.json'));
      expect(stateFiles).toHaveLength(2);
      expect(existsSync(join(memoryHome, 'hook-state', 'shell-hook-tasks.json'))).toBe(false);

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
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'wrapper-agent',
      };
      const payload = { sessionId: 'wrapper-session', workspace, file_path: 'src/wrapped.ts' };

      const pre = runHookWrapper('pre-edit.sh', payload, env, workspace);
      expect(pre.status, pre.stderr).toBe(0);

      const db = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect(db.prepare(`SELECT rf.file_path, tr.agent_id
        FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id WHERE rf.ended_at IS NULL`).get()).toMatchObject({
        file_path: resolve(realpathSync(workspace), 'src/wrapped.ts'),
        agent_id: 'wrapper-agent',
      });

      const post = runHookWrapper('post-edit.sh', payload, env, workspace);
      expect(post.status, post.stderr).toBe(0);

      expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get() as { count: number }).count).toBe(0);
      expect(db.prepare('SELECT origin, status FROM task_runs').get()).toMatchObject({ origin: 'HOOK', status: 'PENDING' });
      db.close();
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

  it('harness-guard.sh passes OCTOCODE_SKILL_ROOT to the runner', () => {
    const result = runHookWrapper(
      'harness-guard.sh',
      { tool_name: 'Edit', tool_input: { file_path: 'SKILL.md' } },
      { OCTOCODE_ALLOW_HARNESS_APPLY: undefined },
      SKILL_ROOT,
    );
    expect(result.status).toBe(2);
    expect(result.stderr).toContain('editing the skill itself is gated');
  });

  it('pre-edit.sh guards before presence without a second host hook', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-wrapper-guard-first-'));
    try {
      const result = runHookWrapper(
        'pre-edit.sh',
        { tool_name: 'Edit', workspace: SKILL_ROOT, tool_input: { file_path: 'SKILL.md' } },
        {
          OCTOCODE_MEMORY_HOME: memoryHome,
          OCTOCODE_AGENT_ID: 'guarded-wrapper-agent',
          OCTOCODE_SKILL_ROOT: undefined,
          OCTOCODE_ALLOW_HARNESS_APPLY: undefined,
        },
        SKILL_ROOT,
      );
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('editing the skill itself is gated');
      expect(existsSync(join(memoryHome, 'awareness.sqlite3'))).toBe(false);
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
      const result = spawnSync(join(tempHooks, 'pre-edit.sh'), [], {
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

describe('hook-runner harness-guard containment', () => {
  // Exercises the harness-guard CLI dispatch end-to-end (payload extraction +
  // path resolution + containment check), which previously had no integration
  // test — hiding a relative-path traversal bypass of the self-edit gate.
  function guard(skillRoot: string | undefined, files: string[], cwd: string, extraEnv: Record<string, string | undefined> = {}) {
    return runScript(
      HOOK_RUNNER,
      ['harness-guard'],
      { tool_name: 'Edit', tool_input: { file_paths: files } },
      { OCTOCODE_SKILL_ROOT: skillRoot, OCTOCODE_ALLOW_HARNESS_APPLY: undefined, ...extraEnv },
      cwd,
    );
  }

  it('is a no-op when OCTOCODE_SKILL_ROOT is unset', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    try {
      expect(guard(undefined, ['SKILL.md'], tmp).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('allows edits resolving outside the skill root', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const project = join(tmp, 'project');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(project, { recursive: true });
    try {
      expect(guard(skillRoot, ['notes.txt'], project).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('gates absolute edits inside the skill root', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    mkdirSync(skillRoot, { recursive: true });
    try {
      const result = guard(skillRoot, [join(skillRoot, 'SKILL.md')], tmp);
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('OCTOCODE_ALLOW_HARNESS_APPLY');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('gates relative traversal edits that resolve inside the skill root', () => {
    // The bypass: cwd is outside the skill root, but a `../` path resolves back
    // in. A textual prefix check misses this; a normalized check must catch it.
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const project = join(tmp, 'project');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(project, { recursive: true });
    try {
      const result = guard(skillRoot, ['../skill/SKILL.md'], project);
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('OCTOCODE_ALLOW_HARNESS_APPLY');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('does not gate a sibling directory sharing the root name prefix', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'octocode-guard-'));
    const skillRoot = join(tmp, 'skill');
    const sibling = join(tmp, 'skill-sibling');
    mkdirSync(skillRoot, { recursive: true });
    mkdirSync(sibling, { recursive: true });
    try {
      expect(guard(skillRoot, [join(sibling, 'x.ts')], tmp).status).toBe(0);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
