import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { evictExpiredLocks, initDb, tableColumns } from '../src/db.js';
import { createPlan, getPlan, joinPlan, listPlans, registerPlanDocument, updatePlanStatus } from '../src/plans.js';
import {
  activeTaskClaimForAgent,
  addTaskDependency,
  claimTask,
  createTask,
  heartbeatTaskClaim,
  listTasks,
  listReadyTasks,
  releaseTaskClaim,
  submitTask,
} from '../src/tasks.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('plan and task collaboration', () => {
  it('keeps durable tasks distinct from execution runs', () => {
    const db = freshDb();
    expect([...tableColumns(db, 'tasks')]).toEqual(expect.arrayContaining([
      'task_id', 'plan_id', 'title', 'reasoning', 'status',
    ]));
    expect([...tableColumns(db, 'task_runs')]).toEqual(expect.arrayContaining([
      'run_id', 'task_id', 'agent_id', 'test_plan', 'status',
    ]));
    expect(tableColumns(db, 'tasks').has('test_plan')).toBe(false);
    expect(tableColumns(db, 'task_runs').has('title')).toBe(false);
  });

  it('creates a managed plan document and registers the lead as a member', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const created = createPlan(db, {
        name: 'Awareness collaboration',
        objective: 'Let agents choose ready work from one shared plan.',
        leadAgentId: 'lead-agent',
        workspacePath: workspace,
      });

      expect(created.plan.plan_id).toMatch(/^plan_/);
      expect(created.plan.status).toBe('DRAFT');
      expect(created.plan.doc_dir).toMatch(/^\.octocode\/plan\/\d{8}-\d{6}Z-awareness-collaboration$/);
      expect(existsSync(join(workspace, created.plan.doc_dir, 'PLAN.md'))).toBe(true);
      expect(readFileSync(join(workspace, created.plan.doc_dir, 'PLAN.md'), 'utf8'))
        .toContain('task state are live in the Awareness database');
      expect(readFileSync(join(workspace, created.plan.doc_dir, 'PLAN.md'), 'utf8'))
        .not.toContain('Status:');

      joinPlan(db, { planId: created.plan.plan_id, agentId: 'worker-agent' });
      writeFileSync(join(workspace, created.plan.doc_dir, 'docs', 'DESIGN.md'), '# Design\n', 'utf8');
      registerPlanDocument(db, {
        planId: created.plan.plan_id,
        agentId: 'worker-agent',
        relativePath: 'docs/DESIGN.md',
        title: 'Design',
      });
      const plan = getPlan(db, created.plan.plan_id);
      expect(plan?.members.map((member) => member.agent_id).sort())
        .toEqual(['lead-agent', 'worker-agent']);
      expect(plan?.docs.map((doc) => doc.relative_path)).toEqual(['PLAN.md', 'docs/DESIGN.md']);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('requires reasoning and at least one path for every task', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Validation',
        objective: 'Keep task intent inspectable.',
        leadAgentId: 'lead',
        workspacePath: workspace,
      });
      expect(() => createTask(db, {
        planId: plan.plan_id,
        title: 'Missing reasoning',
        reasoning: ' ',
        paths: ['src/a.ts'],
        createdBy: 'lead',
      })).toThrow(/reasoning/i);
      expect(() => createTask(db, {
        planId: plan.plan_id,
        title: 'Missing paths',
        reasoning: 'This task has no ownership boundary.',
        paths: [],
        createdBy: 'lead',
      })).toThrow(/path/i);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('validates plan ownership, document containment, filters, and cleanup', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      expect(() => createPlan(db, {
        name: ' ', objective: 'objective', leadAgentId: 'lead', workspacePath: workspace,
      })).toThrow(/name is required/);
      expect(getPlan(db, 'plan_missing')).toBeNull();
      expect(() => joinPlan(db, { planId: 'plan_missing', agentId: 'worker' })).toThrow(/not found/);

      const { plan } = createPlan(db, {
        name: 'Governed plan', objective: 'Validate plan control-plane boundaries.',
        leadAgentId: 'lead', workspacePath: workspace, artifact: 'pkg',
      });
      expect(listPlans(db, { workspacePath: workspace, artifact: 'pkg', status: 'DRAFT' }))
        .toHaveLength(1);
      expect(listPlans(db, { status: 'ACTIVE' })).toEqual([]);
      joinPlan(db, { planId: plan.plan_id, agentId: 'worker' });
      expect(joinPlan(db, { planId: plan.plan_id, agentId: 'worker' }).role).toBe('CONTRIBUTOR');

      expect(() => registerPlanDocument(db, {
        planId: 'plan_missing', agentId: 'worker', relativePath: 'docs/x.md', title: 'x',
      })).toThrow(/not found/);
      expect(() => registerPlanDocument(db, {
        planId: plan.plan_id, agentId: 'outsider', relativePath: 'docs/x.md', title: 'x',
      })).toThrow(/must join/);
      expect(() => registerPlanDocument(db, {
        planId: plan.plan_id, agentId: 'worker', relativePath: join(workspace, 'x.md'), title: 'x',
      })).toThrow(/relative/);
      expect(() => registerPlanDocument(db, {
        planId: plan.plan_id, agentId: 'worker', relativePath: '../escape.md', title: 'x',
      })).toThrow(/inside/);
      expect(() => registerPlanDocument(db, {
        planId: plan.plan_id, agentId: 'worker', relativePath: 'docs/missing.md', title: 'x',
      })).toThrow(/does not exist/);

      expect(() => updatePlanStatus(db, {
        planId: 'plan_missing', status: 'ACTIVE', agentId: 'lead',
      })).toThrow(/not found/);
      expect(() => updatePlanStatus(db, {
        planId: plan.plan_id, status: 'ACTIVE', agentId: 'worker',
      })).toThrow(/only lead/);
      expect(updatePlanStatus(db, {
        planId: plan.plan_id, status: 'ACTIVE', agentId: 'lead',
      }).status).toBe('ACTIVE');

      const failedDb = freshDb();
      failedDb.exec(`CREATE TRIGGER reject_plan BEFORE INSERT ON plans
        BEGIN SELECT RAISE(ABORT, 'forced plan failure'); END`);
      const failedWorkspace = mkdtempSync(join(tmpdir(), 'oc-plan-fail-'));
      try {
        expect(() => createPlan(failedDb, {
          name: 'Rollback', objective: 'Remove files after DB failure.',
          leadAgentId: 'lead', workspacePath: failedWorkspace,
        })).toThrow(/forced plan failure/);
        expect(readdirSync(join(failedWorkspace, '.octocode', 'plan'))).toEqual([]);
      } finally {
        rmSync(failedWorkspace, { recursive: true, force: true });
      }
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('covers dependency guards, filtered task views, heartbeats, and lease expiry', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-plan-'));
    try {
      const { plan } = createPlan(db, {
        name: 'Task controls', objective: 'Exercise task control-plane boundaries.',
        leadAgentId: 'lead', workspacePath: workspace, artifact: 'pkg',
      });
      const otherPlan = createPlan(db, {
        name: 'Other plan', objective: 'Keep dependency graphs isolated.',
        leadAgentId: 'lead', workspacePath: workspace,
      }).plan;
      expect(() => createTask(db, {
        planId: 'plan_missing', title: 'x', reasoning: 'x', paths: ['x.ts'], createdBy: 'lead',
      })).toThrow(/plan not found/);
      const first = createTask(db, {
        planId: plan.plan_id, title: 'First', reasoning: 'Establish the base.',
        paths: ['src/a.ts', 'src/a.ts'], createdBy: 'lead', priority: 5,
      }).task;
      const second = createTask(db, {
        planId: plan.plan_id, title: 'Second', reasoning: 'Consume the base.',
        paths: ['src/b.ts'], createdBy: 'lead', dependsOn: [first.task_id],
      }).task;
      const foreign = createTask(db, {
        planId: otherPlan.plan_id, title: 'Foreign', reasoning: 'Remain in another plan.',
        paths: ['src/c.ts'], createdBy: 'lead',
      }).task;
      db.prepare("UPDATE tasks SET status = 'BLOCKED' WHERE task_id = ?").run(foreign.task_id);
      const notOpen = claimTask(db, { taskId: foreign.task_id, agentId: 'worker' });
      expect(notOpen.ok).toBe(false);
      if (!notOpen.ok) expect(notOpen.error).toMatch(/status=BLOCKED/);
      expect(first.paths).toEqual(['src/a.ts']);
      expect(() => createTask(db, {
        planId: plan.plan_id, title: 'Outside', reasoning: 'Invalid ownership.',
        paths: ['../outside.ts'], createdBy: 'lead',
      })).toThrow(/below the workspace/);
      expect(() => addTaskDependency(db, {
        taskId: first.task_id, dependsOnTaskId: first.task_id, agentId: 'lead',
      })).toThrow(/itself/);
      expect(() => addTaskDependency(db, {
        taskId: first.task_id, dependsOnTaskId: 'task_missing', agentId: 'lead',
      })).toThrow(/must exist/);
      expect(() => addTaskDependency(db, {
        taskId: first.task_id, dependsOnTaskId: foreign.task_id, agentId: 'lead',
      })).toThrow(/one plan/);
      expect(() => addTaskDependency(db, {
        taskId: first.task_id, dependsOnTaskId: second.task_id, agentId: 'lead',
      })).toThrow(/cycle/);
      expect(listTasks(db, { planId: plan.plan_id, status: 'OPEN' })).toHaveLength(2);

      const blocked = claimTask(db, { taskId: second.task_id, agentId: 'worker' });
      expect(blocked.ok).toBe(false);
      if (!blocked.ok) expect(blocked.error).toMatch(/blocked/);
      expect(claimTask(db, { taskId: 'task_missing', agentId: 'worker' }).ok).toBe(false);
      const claim = claimTask(db, {
        taskId: first.task_id, agentId: 'worker', sessionId: 'session-worker', leaseMs: 5_000,
      });
      if (!claim.ok) throw new Error(claim.error);
      expect(activeTaskClaimForAgent(db, {
        agentId: 'worker', workspacePath: workspace, artifact: 'pkg',
      })?.task_id).toBe(first.task_id);
      expect(listTasks(db, { agentId: 'worker' })).toHaveLength(1);
      expect(heartbeatTaskClaim(db, {
        taskId: first.task_id, runId: claim.run.run_id, agentId: 'worker', leaseMs: 6_000,
      }).agent_id).toBe('worker');
      expect(() => heartbeatTaskClaim(db, {
        taskId: first.task_id, runId: claim.run.run_id, agentId: 'other',
      })).toThrow(/not found/);
      expect(() => submitTask(db, {
        taskId: first.task_id, runId: claim.run.run_id, agentId: 'other',
      })).toThrow(/active claimant/);
      expect(() => releaseTaskClaim(db, {
        taskId: first.task_id, runId: claim.run.run_id, agentId: 'other',
      })).toThrow(/active claimant/);

      const locked = preFlightIntent(db, {
        runId: claim.run.run_id, agentId: 'worker', workspacePath: workspace, targetFiles: ['src/a.ts'],
      });
      expect(locked.ok).toBe(true);
      db.prepare("UPDATE task_claims SET expires_at = '2000-01-01T00:00:00Z' WHERE task_id = ?")
        .run(first.task_id);
      expect(listTasks(db, { planId: plan.plan_id }).find((task) => task.task_id === first.task_id)?.status)
        .toBe('OPEN');
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ status: 'FAILED' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM locks WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ count: 0 });

      updatePlanStatus(db, { planId: plan.plan_id, status: 'COMPLETED', agentId: 'lead' });
      expect(() => createTask(db, {
        planId: plan.plan_id, title: 'Late', reasoning: 'Too late.', paths: ['late.ts'], createdBy: 'lead',
      })).toThrow(/completed plan/);

      const failedDb = freshDb();
      const failedWorkspace = mkdtempSync(join(tmpdir(), 'oc-task-fail-'));
      try {
        const failedPlan = createPlan(failedDb, {
          name: 'Task rollback', objective: 'Rollback a failed task insert.',
          leadAgentId: 'lead', workspacePath: failedWorkspace,
        }).plan;
        failedDb.exec(`CREATE TRIGGER reject_task BEFORE INSERT ON tasks
          BEGIN SELECT RAISE(ABORT, 'forced task failure'); END`);
        expect(() => createTask(failedDb, {
          planId: failedPlan.plan_id, title: 'Rejected', reasoning: 'Exercise rollback.',
          paths: ['src/rejected.ts'], createdBy: 'lead',
        })).toThrow(/forced task failure/);
        expect(listTasks(failedDb)).toEqual([]);
      } finally {
        rmSync(failedWorkspace, { recursive: true, force: true });
      }
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

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

  it('fails the durable task when its pending run is explicitly abandoned', () => {
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

      auditUnverified(db, { agentId: 'worker', workspacePath: workspace, abandon: true });
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
      expect(db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(task.task_id))
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
      expect(db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'OPEN' });
      expect(db.prepare('SELECT COUNT(*) AS c FROM task_claims WHERE task_id = ?').get(task.task_id))
        .toEqual({ c: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
