import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { connectDb } from '../src/db.js';
import { insertMemory } from '../src/memory.js';
import { createPlan } from '../src/plans.js';
import { claimTask, createTask as createTaskBase } from '../src/tasks.js';
import type { CreateTaskParams } from '../src/tasks.js';
import { startWork } from '../src/work.js';
const DIST_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../out');
const HOOK_RUNNER = resolve(DIST_DIR, 'hook-runner.js');
const AWARENESS = resolve(DIST_DIR, 'octocode-awareness.js');
const NODE = process.execPath;
type TestTaskParams = Omit<CreateTaskParams, 'acceptanceCriteria'> & {
    acceptanceCriteria?: string;
};
function createTask(db: DatabaseSync, params: TestTaskParams) {
    return createTaskBase(db, { acceptanceCriteria: 'affected behavior is verified', ...params });
}
function runScript(script: string, args: string[], payload: unknown, env: Record<string, string | undefined> = {}, cwd?: string) {
    return spawnSync(NODE, [script, ...args], {
        input: JSON.stringify(payload),
        encoding: 'utf8',
        timeout: 5000,
        cwd,
        env: { ...process.env, ...env },
    });
}

describe('hook-runner', () => {
it('allows two agents to declare ordinary work on the same file without locks', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-presence-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const payload = { workspace, file_path: 'src/shared.ts', hook_event_name: 'PreToolUse' };
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
      expect(db.prepare('SELECT host, event, status FROM hook_receipts').all()).toEqual([
        { host: 'claude', event: 'PreToolUse', status: 'success' },
      ]);
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
      const inspect = new DatabaseSync(join(memoryHome, 'awareness.sqlite3'));
      expect((inspect.prepare('SELECT COUNT(*) AS count FROM task_runs').get() as { count: number }).count).toBe(0);
      expect(inspect.prepare('SELECT event, status FROM hook_receipts').get()).toMatchObject({ event: 'pre-edit', status: 'success' });
      inspect.close();
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
it('uses the submitted prompt to inject only a relevant memory lead', () => {
    const memoryHome = mkdtempSync(join(tmpdir(), 'octocode-hook-selective-memory-'));
    const workspace = resolve(memoryHome, 'repo');
    mkdirSync(workspace, { recursive: true });
    try {
      const dbPath = join(memoryHome, 'awareness.sqlite3');
      const database = connectDb(dbPath);
      insertMemory(database, {
        agentId: 'memory-agent',
        taskContext: 'release screenshots',
        observation: 'rotate the screenshot archive before publishing',
        importance: 10,
        label: 'GOTCHA',
        workspacePath: workspace,
      });
      insertMemory(database, {
        agentId: 'memory-agent',
        taskContext: 'deployment credentials',
        observation: 'token expiry requires refreshing credentials before deploy',
        importance: 7,
        label: 'DECISION',
        workspacePath: workspace,
      });
      database.close();

      const env = {
        OCTOCODE_MEMORY_HOME: memoryHome,
        OCTOCODE_AGENT_ID: 'prompt-agent',
        OCTOCODE_NO_DIGEST: '1',
      };
      const relevant = runScript(HOOK_RUNNER, ['notify-deliver'], {
        session_id: 'session-relevant',
        workspace,
        prompt: 'fix token expiry during deployment',
      }, env);
      expect(relevant.status).toBe(0);
      expect(relevant.stdout).toContain('token expiry');
      expect(relevant.stdout).not.toContain('screenshot archive');

      const unrelated = runScript(HOOK_RUNNER, ['notify-deliver'], {
        session_id: 'session-unrelated',
        workspace,
        prompt: 'format the release notes',
      }, env);
      expect(unrelated.status).toBe(0);
      expect(unrelated.stdout).toBe('');
    } finally {
      rmSync(memoryHome, { recursive: true, force: true });
    }
  });

});
