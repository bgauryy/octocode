import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';

let workspace: string;
let aw: AwarenessStore;

function createPlan(title = 'canonical plan') {
  return aw.createPlan({ agentId: 'lead', title, goal: `Complete ${title} through durable task runs.` });
}

function createTask(planId: string, title = 'canonical task', paths = ['src/index.ts']) {
  return aw.addTask({
    planId, title, paths, agentId: 'lead',
    reasoning: `The ${title} contract is explicit.`,
    acceptance: `${title} is verified.`,
  });
}

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-lite-'));
  aw = openAwarenessStore({ workspace, scope: 'repo' });
});

afterEach(async () => {
  aw.close();
  await rm(workspace, { recursive: true, force: true });
});

describe('AwarenessStore canonical coordination', () => {
  it('reports canonical storage and empty scoped state', () => {
    const status = aw.status();
    expect(status.workspace).toMatch(/aw-lite-/);
    expect(status.dbPath).toBe(join(status.workspace, '.octocode', 'awareness.sqlite3'));
    expect(status).toMatchObject({ plans: 0, tasks: 0, readyTasks: 0, inProgressTasks: 0, pendingChecks: 0, locks: 0, work: 0 });
  });

  it('creates, claims, submits, verifies, and completes a canonical plan', () => {
    const plan = createPlan('ship canonical lifecycle');
    const task = createTask(plan.planId, 'implement canonical lifecycle');
    const claimed = aw.claimTask({ taskId: task.taskId, agentId: 'worker' });
    expect(claimed).toMatchObject({ status: 'IN_PROGRESS', agentId: 'worker', runId: expect.stringMatching(/^run_/) });
    expect(aw.listReadyTasks({ planId: plan.planId })).toEqual([]);

    const submitted = aw.doneTask({ taskId: task.taskId, runId: claimed.runId!, agentId: 'worker' });
    expect(submitted).toMatchObject({ status: 'VERIFY', runId: claimed.runId });
    expect(aw.auditChecks()).toMatchObject({ ok: false, pendingCount: 1 });
    const verified = aw.markCheck({
      taskId: task.taskId, runId: claimed.runId!, doneAt: submitted.updatedAt,
      agentId: 'worker', message: 'focused test passed', status: 'SUCCESS',
    });
    expect(verified).toMatchObject({ status: 'DONE', runId: claimed.runId, verifiedAt: expect.any(String) });
    expect(aw.donePlan({ planId: plan.planId, agentId: 'lead' })).toMatchObject({ status: 'COMPLETED' });
  });

  it('fences heartbeat and submission by the canonical run ID', () => {
    const plan = createPlan();
    const task = createTask(plan.planId);
    const claim = aw.claimTask({ taskId: task.taskId, agentId: 'worker' });
    expect(() => aw.heartbeatTask({ taskId: task.taskId, runId: 'run_stale', agentId: 'worker' })).toThrow(/active task claim/i);
    expect(() => aw.doneTask({ taskId: task.taskId, runId: 'run_stale', agentId: 'worker' })).toThrow(/active claimant/i);
    expect(aw.heartbeatTask({ taskId: task.taskId, runId: claim.runId!, agentId: 'worker' })).toMatchObject({ runId: claim.runId });
  });

  it('keeps expired claims durable while reads project their task as ready', () => {
    const plan = createPlan();
    const task = createTask(plan.planId);
    const claim = aw.claimTask({ taskId: task.taskId, agentId: 'worker' });
    const db = new DatabaseSync(aw.dbPath);
    try {
      db.prepare("UPDATE task_claims SET expires_at = '2000-01-01T00:00:00Z' WHERE task_id = ?").run(task.taskId);
      expect(aw.getTask(task.taskId)).toMatchObject({ status: 'OPEN', agentId: null });
      expect(aw.listReadyTasks({ planId: plan.planId }).map((row) => row.taskId)).toEqual([task.taskId]);
      expect(db.prepare('SELECT run_id FROM task_claims WHERE task_id = ?').get(task.taskId)).toEqual({ run_id: claim.runId });
    } finally { db.close(); }
  });

  it('uses run-owned locks and work presence with bounded leases', async () => {
    await mkdir(join(workspace, 'src'));
    const plan = createPlan();
    const task = createTask(plan.planId);
    const claim = aw.claimTask({ taskId: task.taskId, agentId: 'worker' });
    const lock = aw.acquireLock({ filePath: 'src/index.ts', runId: claim.runId!, agentId: 'worker', reason: 'edit', ttlSeconds: 60 });
    expect(lock).toMatchObject({ runId: claim.runId, agentId: 'worker', filePath: join(aw.status().workspace, 'src/index.ts') });
    expect(() => aw.releaseLock({ filePath: 'src/index.ts', runId: 'run_stale', agentId: 'worker' })).toThrow(/run/i);
    expect(aw.releaseLock({ filePath: 'src/index.ts', runId: claim.runId!, agentId: 'worker' })).toEqual({ released: true });

    const work = aw.startWork({ filePath: 'src/index.ts', runId: claim.runId!, agentId: 'worker', reason: 'edit', testPlan: 'focused tests', ttlSeconds: 60 });
    expect(work).toMatchObject({ runId: claim.runId, agentId: 'worker', reason: 'edit' });
    expect(aw.releaseTask({ taskId: task.taskId, runId: claim.runId!, agentId: 'worker' })).toMatchObject({ status: 'OPEN' });
    expect(aw.listWork()).toEqual([]);
  });

  it('tracks scoped agents, signals, and message reads through canonical tables', () => {
    aw.joinAgent({ agentId: 'agent-a', name: 'Alice' });
    aw.joinAgent({ agentId: 'agent-b', name: 'Bob' });
    const message = aw.sendMessage({ fromAgentId: 'agent-a', toAgentId: 'agent-b', text: 'please review' });
    expect(message.messageId).toMatch(/^ntf_/);
    expect(aw.listMessages({ agentId: 'agent-b' }).map((row) => row.messageId)).toEqual([message.messageId]);
    expect(aw.markMessageRead({ messageId: message.messageId, agentId: 'agent-b' }).readAt).toBeTruthy();
    expect(aw.listMessages({ agentId: 'agent-b' })).toEqual([]);

    const db = new DatabaseSync(aw.dbPath);
    try {
      db.prepare("UPDATE awareness_agents SET last_seen_at = '2000-01-01T00:00:00Z' WHERE workspace_path = ? AND agent_id = ?")
        .run(aw.status().workspace, 'agent-b');
    } finally { db.close(); }
    expect(aw.listAgents({ staleAfterMs: 30_000 }).map((agent) => agent.agentId)).toEqual(['agent-b']);
  });

  it('stores and prunes memory without altering the coordination schema contract', () => {
    const memory = aw.storeMemory({ label: 'GOTCHA', text: 'Use node sqlite with the supported runtime.', tags: 'sqlite,node' });
    expect(aw.recallMemory({ query: 'sqlite' })).toHaveLength(1);
    expect(aw.forgetMemory({ memoryId: memory.memoryId })).toEqual({ forgotten: true });
    expect(aw.schema().entities.memory).toContain('memoryId');
    expect(aw.schema().entities.message).toContain('messageId');
  });
});
