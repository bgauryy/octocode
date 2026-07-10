import { describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db.js';
import { createPiAwarenessBridge, evaluateHarnessGuard, extractPiWriteTargetPaths, wirePiAwarenessHooks } from '../src/pi-hooks.js';
import { insertNotification } from '../src/notifications.js';
import { insertMemory } from '../src/memory.js';
import { createPlan } from '../src/plans.js';
import { claimTask, createTask as createTaskBase } from '../src/tasks.js';
import type { CreateTaskParams } from '../src/tasks.js';
import { startWork } from '../src/work.js';

function tempDb() {
  const dir = mkdtempSync(join(tmpdir(), 'oc-pi-hooks-'));
  return { dir, dbPath: join(dir, 'awareness.sqlite3'), cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

type TestTaskParams = Omit<CreateTaskParams, 'acceptanceCriteria'> & { acceptanceCriteria?: string };
function createTask(db: Parameters<typeof createTaskBase>[0], params: TestTaskParams) {
  return createTaskBase(db, { acceptanceCriteria: 'affected behavior is verified', ...params });
}

function gitRepoOnBranch(branch: string) {
  const dir = mkdtempSync(join(tmpdir(), 'oc-guard-repo-'));
  const git = (...args: string[]) => spawnSync('git', ['-C', dir, ...args], { encoding: 'utf8' });
  git('init', '-q');
  git('config', 'user.email', 't@t');
  git('config', 'user.name', 't');
  git('commit', '-q', '--allow-empty', '-m', 'init');
  git('branch', '-M', branch);
  return { dir, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

// evaluateHarnessGuard is the single source of truth shared by the Pi bridge and
// the shell hook runner (bin/hook-runner.ts), so both vendors gate identically.
describe('evaluateHarnessGuard', () => {
  const base = { env: {} as NodeJS.ProcessEnv };

  it('is a no-op when skillRoot is unset', () => {
    expect(evaluateHarnessGuard({ targetFiles: ['a.ts'], skillRoot: null, cwd: '/tmp', ...base })).toBeNull();
  });

  it('is a no-op for a target resolving outside the skill root', () => {
    const repo = gitRepoOnBranch('feature-x');
    try {
      expect(evaluateHarnessGuard({ targetFiles: ['/tmp/elsewhere.ts'], skillRoot: repo.dir, cwd: '/tmp', ...base })).toBeNull();
    } finally { repo.cleanup(); }
  });

  it('blocks an in-skill edit without OCTOCODE_ALLOW_HARNESS_APPLY', () => {
    const repo = gitRepoOnBranch('feature-x');
    try {
      const reason = evaluateHarnessGuard({ targetFiles: [join(repo.dir, 'SKILL.md')], skillRoot: repo.dir, cwd: repo.dir, env: {} });
      expect(reason).toContain('editing the skill itself is gated');
    } finally { repo.cleanup(); }
  });

  it('allows an approved in-skill edit on a dedicated branch', () => {
    const repo = gitRepoOnBranch('feature-x');
    try {
      const reason = evaluateHarnessGuard({ targetFiles: [join(repo.dir, 'SKILL.md')], skillRoot: repo.dir, cwd: repo.dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1' } });
      expect(reason).toBeNull();
    } finally { repo.cleanup(); }
  });

  it('blocks even when approved if the skill root is on main/master', () => {
    const repo = gitRepoOnBranch('main');
    try {
      const reason = evaluateHarnessGuard({ targetFiles: [join(repo.dir, 'SKILL.md')], skillRoot: repo.dir, cwd: repo.dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1' } });
      expect(reason).toContain('never allowed on main');
    } finally { repo.cleanup(); }
  });

  it('requires OCTOCODE_HARNESS_BRANCH_OK for a non-repo skill root', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-guard-norepo-'));
    try {
      const blocked = evaluateHarnessGuard({ targetFiles: [join(dir, 'SKILL.md')], skillRoot: dir, cwd: dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1' } });
      expect(blocked).toContain('cannot confirm a dedicated git branch');
      const allowed = evaluateHarnessGuard({ targetFiles: [join(dir, 'SKILL.md')], skillRoot: dir, cwd: dir, env: { OCTOCODE_ALLOW_HARNESS_APPLY: '1', OCTOCODE_HARNESS_BRANCH_OK: '1' } });
      expect(allowed).toBeNull();
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});

describe('extractPiWriteTargetPaths', () => {
  it('extracts Pi write/edit tool input shapes', () => {
    expect(extractPiWriteTargetPaths('write', { path: 'src/a.ts' })).toEqual(['src/a.ts']);
    expect(extractPiWriteTargetPaths('edit', { file_path: 'src/b.ts', filePaths: ['src/c.ts', 'src/b.ts'] })).toEqual([
      'src/b.ts',
      'src/c.ts',
    ]);
    expect(extractPiWriteTargetPaths('edit', {
      queries: [
        { path: 'src/d.ts' },
        { file_path: 'src/e.ts', filePaths: ['src/f.ts', 'src/d.ts'] },
      ],
    })).toEqual(['src/d.ts', 'src/e.ts', 'src/f.ts']);
  });

  it('extracts apply_patch file paths from command payloads', () => {
    expect(extractPiWriteTargetPaths('bash', {
      command: ['*** Begin Patch', '*** Update File: src/a.ts', '*** Move to: src/b.ts', '*** End Patch'].join('\n'),
    })).toEqual(['src/a.ts', 'src/b.ts']);
  });
});

describe('createPiAwarenessBridge', () => {
  it('keeps Pi fallback active until session shutdown finalizes it once', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };

      for (let index = 0; index < 5; index += 1) {
        const toolCallId = `shutdown-${index}`;
        await bridge.handleToolCall({ toolName: 'write', toolCallId, input: { path: `src/${index}.ts` } }, ctx);
        expect(bridge.pendingToolRuns.get(toolCallId)).toMatch(/^run_/);
        await bridge.handleToolResult({ toolCallId }, ctx);
      }
      expect(bridge.pendingToolRuns.size).toBe(0);
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status='ACTIVE'").get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE ended_at IS NULL').get() as { c: number }).c).toBe(5);
      await bridge.handleSessionShutdown({ reason: 'quit' }, ctx);
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status='PENDING'").get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c).toBe(0);
      expect(db.prepare('SELECT origin FROM task_runs').get()).toMatchObject({ origin: 'HOOK' });
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE ended_at IS NOT NULL').get() as { c: number }).c).toBe(5);
      expect((db.prepare('SELECT COUNT(*) AS c FROM edit_log').get() as { c: number }).c).toBe(5);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('blocks a Pi write when the host omits a stable toolCallId', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const notices: string[] = [];
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = {
        cwd: tmp.dir,
        sessionManager: { getSessionFile: () => join(tmp.dir, 'missing-id.jsonl') },
        ui: { notify: (message: string) => notices.push(message) },
      };

      const blocked = await bridge.handleToolCall({ toolName: 'write', input: { path: 'src/no-id.ts' } }, ctx);
      expect(blocked).toMatchObject({ block: true, reason: expect.stringContaining('stable toolCallId') });
      expect(bridge.pendingToolRuns.size).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS c FROM task_runs').get() as { c: number }).c).toBe(0);

      await bridge.handleToolResult({ toolName: 'write', input: { path: 'src/no-id.ts' } }, ctx);
      expect(notices.some((message) => message.includes('missing stable toolCallId'))).toBe(true);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('coalesces parallel same-file Pi events while preserving tool correlation', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'parallel.jsonl') } };

      await Promise.all([
        bridge.handleToolCall({ toolName: 'write', toolCallId: 'parallel-1', input: { path: 'src/shared.ts' } }, ctx),
        bridge.handleToolCall({ toolName: 'write', toolCallId: 'parallel-2', input: { path: 'src/shared.ts' } }, ctx),
      ]);

      expect(bridge.pendingToolRuns.size).toBe(2);
      expect(new Set(bridge.pendingToolRuns.values()).size).toBe(1);
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'").get() as { c: number }).c).toBe(1);

      await Promise.all([
        bridge.handleToolResult({ toolCallId: 'parallel-2' }, ctx),
        bridge.handleToolResult({ toolCallId: 'parallel-1' }, ctx),
      ]);

      expect(bridge.pendingToolRuns.size).toBe(0);
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'").get() as { c: number }).c).toBe(1);
      await bridge.handleSessionShutdown({ reason: 'quit' }, ctx);
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING'").get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM edit_log').get() as { c: number }).c).toBe(2);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('blocks Pi writes when another agent holds the file', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      startWork(db, {
        agentId: 'other',
        targetFiles: ['src/conflict.ts'],
        workspacePath: tmp.dir,
        rationale: 'sensitive migration',
        testPlan: 'verify migration',
        origin: 'WORK',
        source: 'EXPLICIT',
        exclusive: true,
      });
      const bridge = createPiAwarenessBridge({ getDb: () => db });

      const result = await bridge.handleToolCall(
        { toolName: 'edit', toolCallId: 'tool-2', input: { path: 'src/conflict.ts' } },
        { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'mine.jsonl') } },
      );

      expect(result).toMatchObject({ block: true });
      expect(result?.reason).toContain('other');
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('allows two Pi agents to work on the same ordinary file and emits one peer delta', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const first = createPiAwarenessBridge({ getDb: () => db });
      const second = createPiAwarenessBridge({ getDb: () => db });
      const firstCtx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'first.jsonl') } };
      const secondCtx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'second.jsonl') } };

      expect(await first.handleToolCall({ toolName: 'write', toolCallId: 'tool-1', input: { path: 'src/a.ts' } }, firstCtx)).toBeUndefined();
      const peerDelta = await second.handleToolCall({ toolName: 'write', toolCallId: 'tool-2', input: { path: 'src/a.ts' } }, secondCtx);
      expect(peerDelta).toMatchObject({ additionalContext: expect.stringContaining('AWARE') });
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE ended_at IS NULL').get() as { c: number }).c).toBe(2);
      expect((db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c).toBe(0);

      const unchanged = await second.handleToolCall({ toolName: 'write', toolCallId: 'tool-3', input: { path: 'src/a.ts' } }, secondCtx);
      expect(unchanged).toBeUndefined();
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('attaches repeated Pi edits to exactly one claimed TASK run', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const agentId = 'pi:task-session';
      const plan = createPlan(db, {
        name: 'Pi task hooks',
        objective: 'Keep hook edits on one task run',
        leadAgentId: 'lead',
        workspacePath: tmp.dir,
      }).plan;
      const task = createTask(db, {
        planId: plan.plan_id,
        title: 'Edit files',
        reasoning: 'Exercise task attachment',
        paths: ['src/a.ts'],
        createdBy: 'lead',
      }).task;
      const claim = claimTask(db, { taskId: task.task_id, agentId });
      expect(claim.ok).toBe(true);
      if (!claim.ok) throw new Error(claim.error);

      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'task-session.jsonl') } };
      for (const [index, file] of ['src/a.ts', 'src/b.ts'].entries()) {
        await bridge.handleToolCall({ toolName: 'write', toolCallId: `task-${index}`, input: { path: file } }, ctx);
        await bridge.handleToolResult({ toolCallId: `task-${index}` }, ctx);
      }
      await bridge.handleSessionShutdown({ reason: 'quit' }, ctx);

      expect((db.prepare('SELECT COUNT(*) AS c FROM task_claims').get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM task_runs').get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE run_id = ? AND ended_at IS NULL').get(claim.run.run_id) as { c: number }).c).toBe(2);
      expect(db.prepare('SELECT origin, status FROM task_runs WHERE run_id = ?').get(claim.run.run_id)).toMatchObject({ origin: 'TASK', status: 'ACTIVE' });
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('keeps an explicit WORK run active across repeated Pi edits', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const agentId = 'pi:work-session';
      const explicit = startWork(db, {
        agentId,
        workspacePath: tmp.dir,
        targetFiles: ['src/a.ts'],
        rationale: 'explicit focused work',
        testPlan: 'focused test',
        origin: 'WORK',
        source: 'EXPLICIT',
      });
      if (!explicit.ok) throw new Error('unexpected conflict');
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'work-session.jsonl') } };

      for (let index = 0; index < 2; index += 1) {
        await bridge.handleToolCall({ toolName: 'write', toolCallId: `work-${index}`, input: { path: 'src/a.ts' } }, ctx);
        await bridge.handleToolResult({ toolCallId: `work-${index}` }, ctx);
      }
      await bridge.handleSessionShutdown({ reason: 'quit' }, ctx);

      expect(db.prepare('SELECT origin, status FROM task_runs WHERE run_id = ?').get(explicit.run.run_id)).toMatchObject({ origin: 'WORK', status: 'ACTIVE' });
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE run_id = ? AND ended_at IS NULL').get(explicit.run.run_id) as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM edit_log WHERE run_id = ?').get(explicit.run.run_id) as { c: number }).c).toBe(2);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('guards Pi harness self-edits with the same approval env as shell hooks', async () => {
    const tmp = tempDb();
    const previousAllow = process.env.OCTOCODE_ALLOW_HARNESS_APPLY;
    const previousBranchOk = process.env.OCTOCODE_HARNESS_BRANCH_OK;
    try {
      delete process.env.OCTOCODE_ALLOW_HARNESS_APPLY;
      delete process.env.OCTOCODE_HARNESS_BRANCH_OK;
      const skillRoot = join(tmp.dir, 'skills', 'octocode-awareness');
      mkdirSync(skillRoot, { recursive: true });
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db, skillRoot });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };

      const blocked = await bridge.handleToolCall(
        { toolName: 'write', toolCallId: 'guard-1', input: { path: join(skillRoot, 'SKILL.md') } },
        ctx,
      );
      expect(blocked).toMatchObject({ block: true });
      expect(blocked?.reason).toContain('editing the skill itself is gated');
      expect((db.prepare('SELECT COUNT(*) AS c FROM task_runs').get() as { c: number }).c).toBe(0);

      process.env.OCTOCODE_ALLOW_HARNESS_APPLY = '1';
      process.env.OCTOCODE_HARNESS_BRANCH_OK = '1';

      const allowed = await bridge.handleToolCall(
        { toolName: 'write', toolCallId: 'guard-2', input: { path: join(skillRoot, 'README.md') } },
        ctx,
      );
      expect(allowed).toBeUndefined();
      expect((db.prepare('SELECT COUNT(*) AS c FROM task_runs').get() as { c: number }).c).toBe(1);
      db.close();
    } finally {
      if (previousAllow === undefined) delete process.env.OCTOCODE_ALLOW_HARNESS_APPLY;
      else process.env.OCTOCODE_ALLOW_HARNESS_APPLY = previousAllow;
      if (previousBranchOk === undefined) delete process.env.OCTOCODE_HARNESS_BRANCH_OK;
      else process.env.OCTOCODE_HARNESS_BRANCH_OK = previousBranchOk;
      tmp.cleanup();
    }
  });
});

describe('wirePiAwarenessHooks', () => {
  it('registers Pi lifecycle equivalents for awareness hooks', () => {
    const events: string[] = [];
    const pi = { on: (eventName: string) => { events.push(eventName); } };

    const bridge = wirePiAwarenessHooks(pi);

    expect(bridge).toBeTruthy();
    expect(events).toEqual([
      'tool_call',
      'tool_result',
      'tool_execution_start',
      'tool_execution_end',
      'input',
      'before_agent_start',
      'agent_end',
      'session_before_compact',
      'session_shutdown',
    ]);
  });


  it('delivers unread notifications through before_agent_start context', async () => {
    const tmp = tempDb();
    const previousAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'agent-b';
    try {
      const db = connectDb(tmp.dbPath);
      insertNotification(db, {
        agentId: 'agent-a',
        toAgent: 'agent-b',
        kind: 'handoff',
        subject: 'hook handoff works',
        body: 'check the notification path',
        workspacePath: tmp.dir,
      });
      const bridge = wirePiAwarenessHooks({ on: () => undefined }, { getDb: () => db })!;
      const result = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });

      expect(result?.message?.customType).toBe('octocode-awareness-briefing');
      expect(String(result?.message?.content)).toContain('hook handoff works');
      expect(String(result?.message?.content)).toContain('agent-a');

      const second = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });
      expect(second).toBeUndefined();
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('keeps the latest Pi prompt transient and injects only its relevant memory lead', async () => {
    const tmp = tempDb();
    const previousAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'pi-prompt-agent';
    try {
      const db = connectDb(tmp.dbPath);
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: 'release screenshots',
        observation: 'rotate the screenshot archive before publishing',
        importance: 10,
        label: 'GOTCHA',
        workspacePath: tmp.dir,
      });
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: 'deployment credentials',
        observation: 'token expiry requires refreshing credentials before deploy',
        importance: 7,
        label: 'DECISION',
        workspacePath: tmp.dir,
      });
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      await bridge.handleInput({ text: 'fix token expiry during deployment' }, { cwd: tmp.dir });
      const result = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });

      expect(String(result?.message?.content)).toContain('token expiry');
      expect(String(result?.message?.content)).not.toContain('screenshot archive');

      const next = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });
      expect(next).toBeUndefined();
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('sends a verify-gate follow-up message when pending runs remain', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const handlers = new Map<string, (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown>();
      const sent: Array<{ message: Record<string, unknown>; options?: Record<string, unknown> }> = [];
      const pi = {
        on: (eventName: string, handler: (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown) => {
          handlers.set(eventName, handler);
        },
        sendMessage: (message: Record<string, unknown>, options?: Record<string, unknown>) => {
          sent.push({ message, options });
        },
      };
      wirePiAwarenessHooks(pi, { getDb: () => db });

      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      for (let index = 0; index < 5; index += 1) {
        await bridge.handleToolCall({ toolName: 'write', toolCallId: `tool-verify-${index}`, input: { path: `src/${index}.ts` } }, ctx);
        await bridge.handleToolResult({ toolCallId: `tool-verify-${index}` }, ctx);
      }

      await handlers.get('agent_end')?.({}, ctx);
      await handlers.get('agent_end')?.({}, ctx);

      expect(sent).toHaveLength(1);
      expect(sent[0]?.message.customType).toBe('octocode-awareness-verify-gate');
      expect(String(sent[0]?.message.content)).toContain('unverified edits');
      expect((String(sent[0]?.message.content).match(/PENDING:run_/g) ?? [])).toHaveLength(1);
      expect(String(sent[0]?.message.content)).not.toContain('omitted');
      expect(sent[0]?.options).toEqual({ deliverAs: 'followUp', triggerTurn: true });
      const aggregate = db.prepare("SELECT run_id, test_plan, status FROM task_runs WHERE origin = 'HOOK'").get() as {
        run_id: string;
        test_plan: string;
        status: string;
      };
      expect(aggregate.status).toBe('PENDING');
      expect(aggregate.test_plan).toContain('smallest relevant test/typecheck');
      expect(aggregate.test_plan).toContain('+2 more');
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE run_id = ?').get(aggregate.run_id) as { c: number }).c).toBe(5);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('runs session_shutdown through the Pi hook without sending verify messages', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const handlers = new Map<string, (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown>();
      const sent: unknown[] = [];
      const pi = {
        on: (eventName: string, handler: (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown) => {
          handlers.set(eventName, handler);
        },
        sendMessage: (message: Record<string, unknown>) => sent.push(message),
      };
      wirePiAwarenessHooks(pi, { getDb: () => db });

      await expect(handlers.get('session_shutdown')?.({ reason: 'quit' }, { cwd: tmp.dir })).resolves.toBeUndefined();
      expect(sent).toHaveLength(0);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });
});
