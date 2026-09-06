import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { createPlan } from '../src/plans.js';
import { claimTask, releaseTaskClaim, submitTask } from '../src/tasks-claims.js';
import { createTask as createTaskBase, listReadyTasks, listTasks } from '../src/tasks-ready.js';
import { getTask } from '../src/tasks-catalog.js';
import type { CreateTaskParams } from '@octocodeai/octocode-shared/entities';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
type TestTaskParams = Omit<CreateTaskParams, 'acceptanceCriteria'> & {
    acceptanceCriteria?: string;
};
function createTask(db: DatabaseSync, params: TestTaskParams) {
    return createTaskBase(db, { acceptanceCriteria: 'affected behavior is verified', ...params });
}

describe('plan and task collaboration', () => {
  it('projects an expired claim as ready without mutating durable task or claim rows', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Read-only expiration', objective: 'Observation must not settle a claim.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Observe expiry', reasoning: 'The lease is stale.',
        paths: ['src/observe.ts'], createdBy: 'lead',
      });
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'worker' });
      if (!claim.ok) throw new Error(claim.error);
      db.prepare("UPDATE task_claims SET expires_at = '2000-01-01T00:00:00Z' WHERE task_id = ?")
        .run(task.task_id);

      expect(getTask(db, task.task_id)).toMatchObject({ status: 'OPEN', claim: null });
      expect(listTasks(db, { planId: plan.plan_id })).toMatchObject([{ status: 'OPEN', claim: null }]);
      expect(listReadyTasks(db, { planId: plan.plan_id }).map((row) => row.task_id)).toEqual([task.task_id]);
      expect(db.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'IN_PROGRESS' });
      expect(db.prepare('SELECT run_id FROM task_claims WHERE task_id = ?').get(task.task_id))
        .toEqual({ run_id: claim.run.run_id });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

it('submitTask rejects expired claim without a prior listTasks/getTask', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Expired submit', objective: 'Expired leases cannot submit.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Work', reasoning: 'Claim then expire.',
        paths: ['src/a.ts'], createdBy: 'lead',
      });
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'worker', leaseMs: 5_000 });
      if (!claim.ok) throw new Error(claim.error);
      db.prepare("UPDATE task_claims SET expires_at = '2000-01-01T00:00:00Z' WHERE task_id = ?")
        .run(task.task_id);
      expect(() => submitTask(db, {
        taskId: task.task_id, runId: claim.run.run_id, agentId: 'worker',
      })).toThrow(/active claimant/);
      expect(db.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'OPEN' });
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ status: 'FAILED' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL')
        .get(claim.run.run_id)).toEqual({ count: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('releaseTaskClaim rejects expired claim without a prior listTasks/getTask', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Expired release', objective: 'Expired leases cannot release as claimant.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Work', reasoning: 'Claim then expire.',
        paths: ['src/a.ts'], createdBy: 'lead',
      });
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'worker', leaseMs: 5_000 });
      if (!claim.ok) throw new Error(claim.error);
      db.prepare("UPDATE task_claims SET expires_at = '2000-01-01T00:00:00Z' WHERE task_id = ?")
        .run(task.task_id);
      expect(() => releaseTaskClaim(db, {
        taskId: task.task_id, runId: claim.run.run_id, agentId: 'worker',
      })).toThrow(/active claimant/);
      expect(db.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'OPEN' });
      expect(db.prepare('SELECT COUNT(*) AS c FROM task_claims WHERE task_id = ?').get(task.task_id))
        .toEqual({ c: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

});
