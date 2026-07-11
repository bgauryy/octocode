import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db.js';
import { createPiAwarenessBridge } from '../src/pi-hooks.js';
import { createPlan } from '../src/plans.js';
import { claimTask, createTask as createTaskBase } from '../src/tasks.js';
import type { CreateTaskParams } from '../src/tasks.js';
import { startWork } from '../src/work.js';
function tempDb() {
    const dir = mkdtempSync(join(tmpdir(), 'oc-pi-hooks-'));
    return { dir, dbPath: join(dir, 'awareness.sqlite3'), cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}
type TestTaskParams = Omit<CreateTaskParams, 'acceptanceCriteria'> & {
    acceptanceCriteria?: string;
};
function createTask(db: Parameters<typeof createTaskBase>[0], params: TestTaskParams) {
    return createTaskBase(db, { acceptanceCriteria: 'affected behavior is verified', ...params });
}

describe('createPiAwarenessBridge', () => {
  // Pi resolves agent id from OCTOCODE_AGENT_ID before per-session identity (see
  // getPiAwarenessAgentId), so an ambient value would collapse the distinct
  // per-session/per-claim identities these tests exercise into one agent.
  let previousAgentId: string | undefined;
  beforeEach(() => {
    previousAgentId = process.env.OCTOCODE_AGENT_ID;
    delete process.env.OCTOCODE_AGENT_ID;
  });
  afterEach(() => {
    if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
    else process.env.OCTOCODE_AGENT_ID = previousAgentId;
  });

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
      expect((db.prepare('SELECT COUNT(*) AS c FROM sessions WHERE ended_at IS NOT NULL').get() as { c: number }).c).toBe(1);
      expect(db.prepare('SELECT origin FROM task_runs').get()).toMatchObject({ origin: 'HOOK' });
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE ended_at IS NOT NULL').get() as { c: number }).c).toBe(5);
      expect((db.prepare('SELECT COUNT(*) AS c FROM edit_log').get() as { c: number }).c).toBe(5);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('rolls back a failed Pi write without edit log or verification debt', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'failed.jsonl') } };

      await bridge.handleToolCall({ toolName: 'write', toolCallId: 'failed-write', input: { path: 'src/failed.ts' } }, ctx);
      await bridge.handleToolResult({ toolCallId: 'failed-write', isError: true }, ctx);

      expect((db.prepare('SELECT COUNT(*) AS c FROM edit_log').get() as { c: number }).c).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files').get() as { c: number }).c).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS c FROM task_runs').get() as { c: number }).c).toBe(0);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('finalizes before compact without ending the reusable Pi session', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'compact.jsonl') } };

      await bridge.handleToolCall({ toolName: 'write', toolCallId: 'before-compact', input: { path: 'src/before.ts' } }, ctx);
      await bridge.handleToolResult({ toolCallId: 'before-compact' }, ctx);
      await bridge.handleSessionCompact({ reason: 'compact:auto' }, ctx);

      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status='PENDING'").get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM sessions WHERE ended_at IS NULL').get() as { c: number }).c).toBe(1);
      expect(await bridge.handleToolCall({ toolName: 'write', toolCallId: 'after-compact', input: { path: 'src/after.ts' } }, ctx)).toBeUndefined();
      expect((db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status='ACTIVE'").get() as { c: number }).c).toBe(1);
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
