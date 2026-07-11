import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb, tableColumns } from '../src/db.js';
import { createPlan, getPlan, joinPlan, listPlans, registerPlanDocument, updatePlanStatus } from '../src/plans.js';
import { activeTaskClaimForAgent, addTaskDependency, claimTask, createTask as createTaskBase, heartbeatTaskClaim, listTasks, releaseTaskClaim, submitTask } from '../src/tasks.js';
import type { CreateTaskParams } from '../src/tasks.js';
import { preFlightIntent } from '../src/intents.js';
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
      expect(created.plan.status).toBe('ACTIVE');
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
      expect(() => createTaskBase(db, {
        planId: plan.plan_id,
        title: 'Missing acceptance',
        reasoning: 'The task must declare how completion is checked.',
        acceptanceCriteria: ' ',
        paths: ['src/a.ts'],
        createdBy: 'lead',
      })).toThrow(/acceptance/i);
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
      expect(listPlans(db, { workspacePath: workspace, artifact: 'pkg', status: 'ACTIVE' }))
        .toHaveLength(1);
      expect(listPlans(db, { status: 'DRAFT' })).toEqual([]);
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
      expect(() => heartbeatTaskClaim(db, {
        taskId: first.task_id, runId: claim.run.run_id, agentId: 'worker', leaseMs: 6_000,
      })).toThrow(/active task claim not found/);
      expect(listTasks(db, { planId: plan.plan_id }).find((task) => task.task_id === first.task_id)?.status)
        .toBe('OPEN');
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ status: 'FAILED' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM locks WHERE run_id = ?').get(claim.run.run_id))
        .toEqual({ count: 0 });

      expect(() => updatePlanStatus(db, {
        planId: plan.plan_id, status: 'COMPLETED', agentId: 'lead',
      })).toThrow(/unfinished task/);
      updatePlanStatus(db, { planId: plan.plan_id, status: 'CANCELLED', agentId: 'lead' });
      expect(() => createTask(db, {
        planId: plan.plan_id, title: 'Late', reasoning: 'Too late.', paths: ['late.ts'], createdBy: 'lead',
      })).toThrow(/cancelled plan/);

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

});
