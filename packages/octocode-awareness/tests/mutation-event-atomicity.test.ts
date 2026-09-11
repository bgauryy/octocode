import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { registerAgent } from '../src/agents.js';
import { initDb } from '../src/db-init.js';
import { workspaceEventHighWater } from '../src/event-outbox.js';
import { releaseFileLock } from '../src/intents-release.js';
import { insertMemory } from '../src/memory-write.js';
import { resolveNotification } from '../src/notifications-inbox.js';
import { insertNotification } from '../src/notifications-core.js';
import { createPlan } from '../src/plans.js';
import { endSession, insertSession } from '../src/sessions.js';
import { createTask } from '../src/tasks-ready.js';
import { startWork } from '../src/work.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

function fixture(): { db: DatabaseSync; workspace: string } {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-mutation-event-')));
  roots.push(workspace);
  const db = new DatabaseSync(':memory:');
  initDb(db);
  return { db, workspace };
}

function rejectEvent(db: DatabaseSync, type: string): void {
  db.exec(`CREATE TRIGGER reject_event BEFORE INSERT ON event_outbox WHEN NEW.event_type = '${type}'
    BEGIN SELECT RAISE(ABORT, 'event rejected'); END`);
}

describe('durable mutation event atomicity', () => {
  it('records direct memory writes and rolls back the row when its event fails', async () => {
    const { db, workspace } = fixture();
    await insertMemory(db, { agentId: 'agent-a', taskContext: 'event convergence',
      observation: 'memory and event commit together', importance: 8, workspacePath: workspace });
    const highWater = workspaceEventHighWater(db, workspace);
    expect(highWater).toBeGreaterThan(0);
    rejectEvent(db, 'memory.recorded');
    await expect(insertMemory(db, { agentId: 'agent-a', taskContext: 'rollback',
      observation: 'must not survive', importance: 5, workspacePath: workspace })).rejects.toThrow('event rejected');
    expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_memories').get()).toEqual({ count: 1 });
    expect(workspaceEventHighWater(db, workspace)).toBe(highWater);
    db.close();
  });

  it('commits agent and session lifecycle mutations with scoped events', () => {
    const { db, workspace } = fixture();
    const before = workspaceEventHighWater(db, workspace);
    registerAgent(db, { agentId: 'agent-a', agentName: 'A', workspacePath: workspace });
    const registered = workspaceEventHighWater(db, workspace);
    expect(registered).toBeGreaterThan(before);
    const session = insertSession(db, { agentId: 'agent-a', workspacePath: workspace });
    const started = workspaceEventHighWater(db, workspace);
    expect(started).toBeGreaterThan(registered);
    expect(endSession(db, { sessionId: session.session_id, agentId: 'agent-a', workspacePath: workspace })).not.toBeNull();
    expect(workspaceEventHighWater(db, workspace)).toBeGreaterThan(started);
    db.close();
  });

  it('rolls back session end and message resolution when their events fail', () => {
    const { db, workspace } = fixture();
    const session = insertSession(db, { agentId: 'agent-a', workspacePath: workspace });
    const signal = insertNotification(db, { agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi',
      subject: 'atomic resolve', workspacePath: workspace });
    const highWater = workspaceEventHighWater(db, workspace);
    rejectEvent(db, 'session.ended');
    expect(() => endSession(db, { sessionId: session.session_id, agentId: 'agent-a', workspacePath: workspace }))
      .toThrow('event rejected');
    expect(db.prepare('SELECT ended_at FROM sessions WHERE session_id = ?').get(session.session_id)).toEqual({ ended_at: null });
    expect(workspaceEventHighWater(db, workspace)).toBe(highWater);
    db.exec('DROP TRIGGER reject_event');
    rejectEvent(db, 'peer.message.resolved');
    expect(() => resolveNotification(db, { notificationIds: [signal.signal_id], agentId: 'agent-b', workspacePath: workspace }))
      .toThrow('event rejected');
    expect(db.prepare('SELECT status FROM signals WHERE signal_id = ?').get(signal.signal_id)).toEqual({ status: 'open' });
    expect(workspaceEventHighWater(db, workspace)).toBe(highWater);
    db.close();
  });

  it('rolls back protection release when its operational event fails', () => {
    const { db, workspace } = fixture();
    const started = startWork(db, { agentId: 'agent-a', workspacePath: workspace, targetFiles: ['src/a.ts'],
      rationale: 'protect ownership', testPlan: 'run focused test', exclusive: true });
    if (!started.ok) throw new Error('work start failed');
    const highWater = workspaceEventHighWater(db, workspace);
    rejectEvent(db, 'work.protection-released');
    expect(() => releaseFileLock(db, { agentId: 'agent-a', runId: started.run.run_id,
      workspacePath: workspace, status: 'PENDING' })).toThrow('event rejected');
    expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_locks WHERE run_id = ?').get(started.run.run_id))
      .toEqual({ count: 1 });
    expect(workspaceEventHighWater(db, workspace)).toBe(highWater);
    db.close();
  });

  it('commits task check configuration with creation and rolls both back when its event fails', () => {
    const { db, workspace } = fixture();
    const plan = createPlan(db, { name: 'Atomic tasks', objective: 'Keep task state aligned with events',
      leadAgentId: 'agent-a', workspacePath: workspace }).plan;
    const task = createTask(db, { planId: plan.plan_id, title: 'Configured task', paths: ['src/a.ts'],
      reasoning: 'The check belongs to the task contract', acceptanceCriteria: 'The check passes',
      createdBy: 'agent-a', checkCommand: 'yarn test focused' }).task;
    expect(task.check_command).toBe('yarn test focused');
    const highWater = workspaceEventHighWater(db, workspace);
    rejectEvent(db, 'task.created');
    expect(() => createTask(db, { planId: plan.plan_id, title: 'Rejected task', paths: ['src/b.ts'],
      reasoning: 'Exercise rollback', acceptanceCriteria: 'No row survives', createdBy: 'agent-a',
      checkCommand: 'yarn test rejected' })).toThrow('event rejected');
    expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_tasks').get()).toEqual({ count: 1 });
    expect(workspaceEventHighWater(db, workspace)).toBe(highWater);
    db.close();
  });
});
