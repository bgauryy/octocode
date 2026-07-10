import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { initDb } from '../src/db.js';
import { createPlan } from '../src/plans.js';
import { addTaskDependency, claimTask, createTask } from '../src/tasks.js';
import { agentSignal } from '../src/notifications.js';
import { startWork } from '../src/work.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('full-loop coordination correctness fixes', () => {
  it('uses the stored run workspace for relative files and rejects session scope drift', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-coordination-work-'));
    const otherWorkspace = mkdtempSync(join(tmpdir(), 'oc-coordination-other-'));
    try {
      const started = startWork(db, {
        agentId: 'agent-a',
        sessionId: 'session-shared',
        workspacePath: workspace,
        artifact: 'pkg-a',
        rationale: 'exercise stored scope',
        testPlan: 'targeted test',
        targetFiles: ['src/first.ts'],
      });
      if (!started.ok) throw new Error('unexpected conflict');

      const attached = startWork(db, {
        agentId: 'agent-a',
        sessionId: 'session-shared',
        runId: started.run.run_id,
        targetFiles: ['src/second.ts'],
      });
      if (!attached.ok) throw new Error('unexpected conflict');
      expect(attached.files.map((file) => file.file_path))
        .toEqual([resolve(started.run.workspace_path!, 'src/second.ts')]);

      expect(() => startWork(db, {
        agentId: 'agent-b', sessionId: 'session-shared', workspacePath: workspace, artifact: 'pkg-a',
        rationale: 'wrong owner', testPlan: 'must fail', targetFiles: ['src/b.ts'],
      })).toThrow(/belongs to agent agent-a/);
      expect(() => startWork(db, {
        agentId: 'agent-a', sessionId: 'session-shared', workspacePath: otherWorkspace, artifact: 'pkg-a',
        rationale: 'wrong workspace', testPlan: 'must fail', targetFiles: ['src/c.ts'],
      })).toThrow(/belongs to workspace/);
      expect(() => startWork(db, {
        agentId: 'agent-a', sessionId: 'session-shared', workspacePath: workspace, artifact: 'pkg-b',
        rationale: 'wrong artifact', testPlan: 'must fail', targetFiles: ['src/d.ts'],
      })).toThrow(/belongs to artifact pkg-a/);

      const plan = createPlan(db, {
        name: 'Session claim', objective: 'Reject claim session drift.', leadAgentId: 'lead',
        workspacePath: workspace, artifact: 'pkg-a',
      }).plan;
      const task = createTask(db, {
        planId: plan.plan_id, title: 'Claim', reasoning: 'Exercise ownership.',
        acceptanceCriteria: 'claim is scoped', paths: ['src/task.ts'], createdBy: 'lead',
      }).task;
      expect(() => claimTask(db, {
        taskId: task.task_id, agentId: 'agent-b', sessionId: 'session-shared',
      })).toThrow(/belongs to agent agent-a/);
      expect(db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(task.task_id))
        .toEqual({ status: 'OPEN' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM task_claims WHERE task_id = ?').get(task.task_id))
        .toEqual({ count: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
      rmSync(otherWorkspace, { recursive: true, force: true });
    }
  });

  it('adds dependencies atomically and idempotently while guarding terminal state', () => {
    const db = freshDb();
    const workspace = mkdtempSync(join(tmpdir(), 'oc-coordination-task-'));
    try {
      const plan = createPlan(db, {
        name: 'Dependency guards', objective: 'Keep graph changes valid.',
        leadAgentId: 'lead', workspacePath: workspace,
      }).plan;
      const first = createTask(db, {
        planId: plan.plan_id, title: 'First', reasoning: 'Provide the base.',
        acceptanceCriteria: 'base exists', paths: ['src/a.ts'], createdBy: 'lead',
      }).task;
      const second = createTask(db, {
        planId: plan.plan_id, title: 'Second', reasoning: 'Consume the base.',
        acceptanceCriteria: 'dependency exists', paths: ['src/b.ts'], createdBy: 'lead',
      }).task;

      addTaskDependency(db, { taskId: second.task_id, dependsOnTaskId: first.task_id, agentId: 'lead' });
      addTaskDependency(db, { taskId: second.task_id, dependsOnTaskId: first.task_id, agentId: 'lead' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM task_dependencies WHERE task_id = ?')
        .get(second.task_id)).toEqual({ count: 1 });
      expect(db.prepare("SELECT COUNT(*) AS count FROM task_events WHERE task_id = ? AND event_type = 'DEPENDENCY_ADDED'")
        .get(second.task_id)).toEqual({ count: 1 });
      expect(() => addTaskDependency(db, {
        taskId: first.task_id, dependsOnTaskId: second.task_id, agentId: 'lead',
      })).toThrow(/cycle/);

      db.prepare("UPDATE tasks SET status = 'DONE' WHERE task_id = ?").run(second.task_id);
      expect(() => addTaskDependency(db, {
        taskId: second.task_id, dependsOnTaskId: first.task_id, agentId: 'lead',
      })).toThrow(/status DONE/);
      db.prepare("UPDATE tasks SET status = 'OPEN' WHERE task_id = ?").run(second.task_id);
      db.prepare("UPDATE plans SET status = 'CANCELLED' WHERE plan_id = ?").run(plan.plan_id);
      expect(() => addTaskDependency(db, {
        taskId: second.task_id, dependsOnTaskId: first.task_id, agentId: 'lead',
      })).toThrow(/cancelled plan/);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('targets inferred reply participants and authorizes broadcast resolution by participation', () => {
    const db = freshDb();
    const published = agentSignal(db, {
      action: 'publish', agentId: 'agent-a', toAgents: ['agent-b'], kind: 'question',
      subject: 'review?', workspacePath: '/repo',
    });
    if (published.action !== 'publish') throw new Error('publish failed');
    const reply = agentSignal(db, {
      action: 'reply', agentId: 'agent-b', inReplyTo: published.signal_id,
      subject: 'reviewed', workspacePath: '/repo',
    });
    if (reply.action !== 'reply') throw new Error('reply failed');
    expect(db.prepare('SELECT to_agent FROM signals WHERE signal_id = ?').get(reply.signal_id))
      .toEqual({ to_agent: 'agent-a' });
    const outsider = agentSignal(db, {
      action: 'resolve', agentId: 'agent-c', threadId: published.thread_id, workspacePath: '/repo',
    });
    if (outsider.action !== 'resolve') throw new Error('resolve failed');
    expect(outsider.resolved).toBe(0);
    const author = agentSignal(db, {
      action: 'resolve', agentId: 'agent-a', threadId: published.thread_id, workspacePath: '/repo',
    });
    if (author.action !== 'resolve') throw new Error('resolve failed');
    expect(author.resolved).toBe(2);

    const broadcast = agentSignal(db, {
      action: 'publish', agentId: 'agent-a', kind: 'blocker',
      subject: 'broadcast', workspacePath: '/repo',
    });
    if (broadcast.action !== 'publish') throw new Error('publish failed');
    const unobserved = agentSignal(db, {
      action: 'resolve', agentId: 'agent-c', threadId: broadcast.thread_id, workspacePath: '/repo',
    });
    if (unobserved.action !== 'resolve') throw new Error('resolve failed');
    expect(unobserved.resolved).toBe(0);
    const ack = agentSignal(db, {
      action: 'ack', agentId: 'agent-b', signalIds: [broadcast.signal_id], workspacePath: '/repo',
    });
    if (ack.action !== 'ack') throw new Error('ack failed');
    expect(ack.acknowledged).toBe(1);
    const observed = agentSignal(db, {
      action: 'resolve', agentId: 'agent-b', threadId: broadcast.thread_id, workspacePath: '/repo',
    });
    if (observed.action !== 'resolve') throw new Error('resolve failed');
    expect(observed.resolved).toBe(1);
  });
});
