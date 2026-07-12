import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { evictExpiredLocks, initDb } from '../src/db.js';
import { countPlans, createPlan, getPlan, listPlans, updatePlanStatus } from '../src/plans.js';
import { addTaskDependency, claimTask, countReadyTasks, countTasks, createTask as createTaskBase, listTasks, listReadyTasks, releaseTaskClaim, submitTask } from '../src/tasks.js';
import type { CreateTaskParams } from '../src/tasks.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
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
it('derives readiness from dependencies and atomically claims one task', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Dependency flow',
        objective: 'Only expose unblocked tasks.',
        leadAgentId: 'lead',
        workspacePath: workspace,
      });
      const first = createTask(db, {
        planId: plan.plan_id,
        title: 'Schema',
        reasoning: 'The storage contract must exist before consumers.',
        paths: ['src/db.ts'],
        createdBy: 'lead',
      });
      const second = createTask(db, {
        planId: plan.plan_id,
        title: 'CLI',
        reasoning: 'The CLI consumes the storage contract.',
        paths: ['bin/awareness.ts'],
        createdBy: 'lead',
      });
      addTaskDependency(db, {
        taskId: second.task.task_id,
        dependsOnTaskId: first.task.task_id,
        agentId: 'lead',
      });

      expect(listReadyTasks(db, { planId: plan.plan_id }).map((task) => task.task_id))
        .toEqual([first.task.task_id]);

      const claim = claimTask(db, { taskId: first.task.task_id, agentId: 'worker-a' });
      expect(claim.ok).toBe(true);
      if (!claim.ok) throw new Error(claim.error);
      expect(claim.run.run_id).toMatch(/^run_/);
      expect(claim.run.origin).toBe('TASK');
      expect(claim.task.status).toBe('IN_PROGRESS');

      const competing = claimTask(db, { taskId: first.task.task_id, agentId: 'worker-b' });
      expect(competing.ok).toBe(false);
      if (!competing.ok) expect(competing.error).toMatch(/claimed/i);

      const activeFile = preFlightIntent(db, {
        runId: claim.run.run_id,
        agentId: 'worker-a',
        workspacePath: workspace,
        targetFiles: ['src/db.ts'],
      });
      expect(activeFile.ok).toBe(true);

      const submitted = submitTask(db, {
        taskId: first.task.task_id,
        runId: claim.run.run_id,
        agentId: 'worker-a',
        message: 'schema tests pass',
      });
      expect(submitted.task.status).toBe('VERIFY');
      expect(db.prepare('SELECT COUNT(*) AS count FROM locks WHERE run_id = ?')
        .get(claim.run.run_id)).toEqual({ count: 0 });
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL')
        .get(claim.run.run_id)).toEqual({ count: 0 });

      const verified = markVerified(db, {
        runId: claim.run.run_id,
        agentId: 'worker-a',
        status: 'SUCCESS',
        message: 'targeted tests passed',
      });
      expect(verified.ok).toBe(true);
      expect(db.prepare('SELECT status FROM tasks WHERE task_id = ?')
        .get(first.task.task_id)).toEqual({ status: 'DONE' });
      expect(listReadyTasks(db, { planId: plan.plan_id }).map((task) => task.task_id))
        .toEqual([second.task.task_id]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('blocks ready/claim outside ACTIVE plans and cancels unclaimed tasks atomically', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-status-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Governed queue', objective: 'Make lead status authoritative.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Queued', reasoning: 'Wait for active governance.',
        acceptanceCriteria: 'tests pass', paths: ['src/a.ts'], createdBy: 'lead',
      });

      updatePlanStatus(db, { planId: plan.plan_id, status: 'PAUSED', agentId: 'lead' });
      expect(listReadyTasks(db, { planId: plan.plan_id })).toEqual([]);
      const pausedClaim = claimTask(db, { taskId: task.task_id, agentId: 'worker' });
      expect(pausedClaim.ok).toBe(false);
      if (!pausedClaim.ok) expect(pausedClaim.error).toMatch(/plan is not ACTIVE/);

      updatePlanStatus(db, { planId: plan.plan_id, status: 'CANCELLED', agentId: 'lead' });
      expect(getPlan(db, plan.plan_id)?.status).toBe('CANCELLED');
      expect(listTasks(db, { planId: plan.plan_id })[0]?.status).toBe('CANCELLED');
      expect(claimTask(db, { taskId: task.task_id, agentId: 'worker' }).ok).toBe(false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('bounds plan/task reads while counts remain exact', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-bounds-'));
    try {
      const plans = Array.from({ length: 3 }, (_, index) => createPlan(db, {
        name: `Plan ${index}`,
        objective: 'Exercise bounded reads.',
        leadAgentId: 'lead',
        workspacePath: workspace,
      }).plan);
      expect(countPlans(db, { workspacePath: workspace })).toBe(3);
      expect(listPlans(db, { workspacePath: workspace, limit: 2 })).toHaveLength(2);

      for (let index = 0; index < 3; index++) {
        createTask(db, {
          planId: plans[0]!.plan_id,
          title: `Task ${index}`,
          reasoning: 'Exercise bounded task reads.',
          paths: [`src/${index}.ts`],
          createdBy: 'lead',
        });
      }
      expect(countTasks(db, { planId: plans[0]!.plan_id })).toBe(3);
      expect(countReadyTasks(db, { planId: plans[0]!.plan_id })).toBe(3);
      expect(listTasks(db, { planId: plans[0]!.plan_id, limit: 2 })).toHaveLength(2);
      expect(listReadyTasks(db, { planId: plans[0]!.plan_id, limit: 2 })).toHaveLength(2);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('attaches repeated exclusive edits to a task run while explicit lock-only work stays isolated', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Hook grouping', objective: 'Keep one run per claimed task.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Edit two files', reasoning: 'Both edits implement one task.',
        paths: ['src/a.ts', 'src/b.ts'], createdBy: 'lead',
      });
      const claimed = claimTask(db, { taskId: task.task_id, agentId: 'worker' });
      if (!claimed.ok) throw new Error(claimed.error);

      for (const file of ['src/a.ts', 'src/b.ts']) {
        const lock = preFlightIntent(db, {
          runId: claimed.run.run_id,
          agentId: 'worker',
          workspacePath: workspace,
          targetFiles: [file],
        });
        expect(lock.ok).toBe(true);
        releaseFileLock(db, { runId: claimed.run.run_id, agentId: 'worker', status: 'ACTIVE' });
      }
      expect(db.prepare('SELECT COUNT(*) AS count FROM task_runs WHERE task_id = ?')
        .get(task.task_id)).toEqual({ count: 1 });
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ?')
        .get(claimed.run.run_id)).toEqual({ count: 2 });
      expect(() => releaseFileLock(db, {
        runId: claimed.run.run_id, agentId: 'worker', status: 'PENDING',
      })).toThrow(/task submit or task release/);

      const lockOnly = preFlightIntent(db, {
        agentId: 'sensitive-worker', workspacePath: workspace, targetFiles: ['README.md'],
      });
      if (!lockOnly.ok) throw new Error('exclusive lock failed');
      expect(lockOnly.run.task_id).toBeNull();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('does not treat a live task claim between edits as a stale lock-only run', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Long task', objective: 'Let a task outlive each short file lock.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Several edits', reasoning: 'The agent pauses between files.',
        paths: ['src/a.ts'], createdBy: 'lead',
      });
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'worker' });
      if (!claim.ok) throw new Error(claim.error);
      const locked = preFlightIntent(db, {
        runId: claim.run.run_id, agentId: 'worker', workspacePath: workspace, targetFiles: ['src/a.ts'],
      });
      expect(locked.ok).toBe(true);
      db.prepare("UPDATE locks SET expires_at = '2000-01-01T00:00:00Z' WHERE run_id = ?")
        .run(claim.run.run_id);
      expect(evictExpiredLocks(db)).toEqual({ pruned_locks: 1 });

      const audit = auditUnverified(db, { agentId: 'worker', workspacePath: workspace });
      expect(audit.stale_active).toEqual([]);
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ status: 'ACTIVE' });
      expect(db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'IN_PROGRESS' });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('clears exclusive locks when a task claim is released', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Release locks', objective: 'Return plan work without blocking the next agent.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Attempt work', reasoning: 'The first attempt may stop early.',
        paths: ['src/a.ts'], createdBy: 'lead',
      });
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'worker' });
      if (!claim.ok) throw new Error(claim.error);
      const locked = preFlightIntent(db, {
        runId: claim.run.run_id, agentId: 'worker', workspacePath: workspace, targetFiles: ['src/a.ts'],
      });
      expect(locked.ok).toBe(true);

      const released = releaseTaskClaim(db, {
        taskId: task.task_id, runId: claim.run.run_id, agentId: 'worker',
      });
      expect(released.status).toBe('OPEN');
      expect(db.prepare('SELECT COUNT(*) AS count FROM locks WHERE run_id = ?')
        .get(claim.run.run_id)).toEqual({ count: 0 });
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL')
        .get(claim.run.run_id)).toEqual({ count: 0 });
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?')
        .get(claim.run.run_id)).toEqual({ status: 'FAILED' });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('fails the durable task when its pending run is explicitly marked FAILED', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Abandon', objective: 'Keep task and run terminal states aligned.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const { task } = createTask(db, {
        planId: plan.plan_id, title: 'Failed work', reasoning: 'The implementation cannot be verified.',
        paths: ['src/fail.ts'], createdBy: 'lead',
      });
      const claim = claimTask(db, { taskId: task.task_id, agentId: 'worker' });
      if (!claim.ok) throw new Error(claim.error);
      submitTask(db, { taskId: task.task_id, runId: claim.run.run_id, agentId: 'worker' });

      expect(markVerified(db, {
        runId: claim.run.run_id,
        agentId: 'worker',
        status: 'FAILED',
        message: 'declared verification failed',
      })).toMatchObject({ ok: true, status: 'FAILED' });
      expect(db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'FAILED' });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('createTask with missing dependsOn rolls back the whole task', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Atomic deps', objective: 'No orphan tasks on bad dependsOn.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      expect(() => createTask(db, {
        planId: plan.plan_id,
        title: 'Orphan risk',
        reasoning: 'Depends on a missing task.',
        paths: ['src/x.ts'],
        createdBy: 'lead',
        dependsOn: ['task_missing'],
      })).toThrow(/must exist/);
      expect(listTasks(db, { planId: plan.plan_id })).toEqual([]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
it('createTask with partial dependsOn list rolls back', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Partial deps', objective: 'All-or-nothing dependency edges.',
        leadAgentId: 'lead', workspacePath: workspace,
      });
      const first = createTask(db, {
        planId: plan.plan_id, title: 'Base', reasoning: 'Exists.',
        paths: ['src/a.ts'], createdBy: 'lead',
      }).task;
      const before = listTasks(db, { planId: plan.plan_id }).map((t) => t.task_id);
      expect(() => createTask(db, {
        planId: plan.plan_id,
        title: 'Partial',
        reasoning: 'One valid edge then a missing one.',
        paths: ['src/b.ts'],
        createdBy: 'lead',
        dependsOn: [first.task_id, 'task_missing'],
      })).toThrow(/must exist/);
      expect(listTasks(db, { planId: plan.plan_id }).map((t) => t.task_id)).toEqual(before);
      expect(db.prepare('SELECT COUNT(*) AS c FROM task_dependencies WHERE depends_on_task_id = ?')
        .get(first.task_id)).toEqual({ c: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

});
