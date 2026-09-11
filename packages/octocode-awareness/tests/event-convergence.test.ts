import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { initializeFreshDb } from '../src/db-init.js';
import { insertEditLog, insertHarnessLog, queryEditLog, queryHarnessLog } from '../src/audit.js';
import { appendRunVerificationEvent, appendTaskEvent, listOutboxEvents } from '../src/event-outbox.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initializeFreshDb(db);
  return db;
}

function seedTaskAndRun(db: DatabaseSync): void {
  db.prepare(`INSERT INTO awareness_plans(
    plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at
  ) VALUES ('plan-1', 'Plan', 'Prove convergence', 'agent-1', 'ACTIVE', '/repo', '.octocode', ?, ?)`)
    .run('2026-09-11T00:00:00Z', '2026-09-11T00:00:00Z');
  db.prepare(`INSERT INTO awareness_tasks(
    task_id, plan_id, title, reasoning, acceptance_criteria, status, created_by, created_at, updated_at
  ) VALUES ('task-1', 'plan-1', 'Task', 'Reason', 'Done', 'IN_PROGRESS', 'agent-1', ?, ?)`)
    .run('2026-09-11T00:00:00Z', '2026-09-11T00:00:00Z');
  db.prepare(`INSERT INTO task_runs(
    run_id, task_id, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
  ) VALUES ('run-1', 'task-1', 'agent-1', 'Reason', 'Test', 'PENDING', '/repo', ?, ?)`)
    .run('2026-09-11T00:00:00Z', '2026-09-11T00:00:00Z');
}

describe('canonical domain event convergence', () => {
  it('projects edit and harness APIs from one audit stream without dual-writing legacy logs', () => {
    const db = freshDb();
    const edit = insertEditLog(db, {
      agentId: 'agent-1', workspacePath: '/repo', filePath: '/repo/a.ts', operation: 'update',
      linesAdded: 3, linesRemoved: 1, contentHash: 'legacy-hash-is-not-canonical',
    });
    const harness = insertHarnessLog(db, {
      agentId: 'agent-1', workspacePath: '/repo', eventType: 'validate', payload: { passed: true },
    });

    expect(queryEditLog(db, {})).toEqual([expect.objectContaining({
      edit_id: edit.editId,
      operation: 'update',
      file_path: '/repo/a.ts',
      content_hash: null,
    })]);
    expect(queryHarnessLog(db, {})).toEqual([expect.objectContaining({
      harness_id: harness,
      event_type: 'validate',
      payload_json: JSON.stringify({ passed: true }),
    })]);
    expect(db.prepare('SELECT COUNT(*) AS count FROM edit_log').get()).toEqual({ count: 0 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM harness_log').get()).toEqual({ count: 0 });
    expect(db.prepare(`SELECT event_type, retention_class FROM event_outbox ORDER BY sequence`).all())
      .toEqual([
        { event_type: 'workspace.edit.update', retention_class: 'audit' },
        { event_type: 'harness.validate', retention_class: 'audit' },
      ]);
    db.close();
  });

  it('preserves exact task/run order and bounded replay with no legacy rows', () => {
    const db = freshDb();
    seedTaskAndRun(db);
    appendTaskEvent(db, {
      taskId: 'task-1', runId: 'run-1', agentId: 'agent-1', eventType: 'SUBMITTED',
      message: 'ready', createdAt: '2026-09-11T00:00:01Z',
    });
    appendRunVerificationEvent(db, {
      runId: 'run-1', agentId: 'agent-2', message: 'tests passed', createdAt: '2026-09-11T00:00:02Z',
    });
    appendTaskEvent(db, {
      taskId: 'task-1', runId: 'run-1', agentId: 'agent-1', eventType: 'VERIFIED',
      message: 'done', createdAt: '2026-09-11T00:00:03Z',
    });

    const first = listOutboxEvents(db, { workspace: '/repo', limit: 2 });
    const second = listOutboxEvents(db, { workspace: '/repo', limit: 2, ...first.next! });
    expect([...first.events, ...second.events].map((event) => event.type)).toEqual([
      'task.submitted', 'run.verified', 'task.verified',
    ]);
    expect(first.next).toEqual({ afterSequence: first.events[1]!.sequence });
    expect(second.next).toBeNull();
    expect(db.prepare('SELECT COUNT(*) AS count FROM task_events').get()).toEqual({ count: 0 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM run_log').get()).toEqual({ count: 0 });
    db.close();
  });

  it('rolls a task mutation and its event back as one unit', () => {
    const db = freshDb();
    seedTaskAndRun(db);
    db.exec('BEGIN IMMEDIATE');
    db.prepare("UPDATE awareness_tasks SET status = 'VERIFY' WHERE task_id = 'task-1'").run();
    appendTaskEvent(db, {
      taskId: 'task-1', runId: 'run-1', agentId: 'agent-1', eventType: 'SUBMITTED',
      message: 'ready', createdAt: '2026-09-11T00:00:01Z',
    });
    db.exec('ROLLBACK');

    expect(db.prepare("SELECT status FROM awareness_tasks WHERE task_id = 'task-1'").get())
      .toEqual({ status: 'IN_PROGRESS' });
    expect(db.prepare('SELECT COUNT(*) AS count FROM event_outbox').get()).toEqual({ count: 0 });
    db.close();
  });
});
