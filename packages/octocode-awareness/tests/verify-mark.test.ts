import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { preFlightIntent } from '../src/intents-preflight.js';
import { releaseFileLock } from '../src/intents-release.js';
import { auditUnverified } from '../src/verify-audit.js';
import { markVerified } from '../src/verify-mark.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

/** Create a PENDING task: claim then immediately release with status PENDING. */
function makePending(
  db: DatabaseSync,
  agentId: string,
  workspacePath: string,
  testPlan = 'verify edits',
): string {
  const claim = preFlightIntent(db, {
    agentId,
    workspacePath,
    targetFiles: [`/tmp/${agentId}-target.txt`],
    testPlan,
  });
  if (!claim.ok) throw new Error('claim failed');
  releaseFileLock(db, { agentId, runId: claim.run.run_id, status: 'PENDING' });
  return claim.run.run_id;
}

describe('markVerified', () => {
  it('rejects SUCCESS without a durable evidence receipt', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { runId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(result).toMatchObject({ ok: false });
    if (!result.ok) expect(result.error).toMatch(/evidence receipt/);
    expect(auditUnverified(db, { agentId: 'agent-a', workspacePath: '/tmp/ws-a' }).count).toBe(1);
  });

  it('rejects unscoped allPending before mutation', () => {
    const db = freshDb();
    makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, {
      agentId: 'agent-a', allPending: true, status: 'SUCCESS', message: 'checks passed',
    });
    expect(result).toMatchObject({ ok: false });
    expect(auditUnverified(db, { agentId: 'agent-a' }).count).toBe(1);
  });

  it('transitions a PENDING task to SUCCESS and clears it from auditUnverified', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    expect(auditUnverified(db).count).toBe(1);

    const result = markVerified(db, { runId, agentId: 'agent-a', status: 'SUCCESS', message: 'declared check passed' });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.run_id).toBe(runId);
      expect(result.status).toBe('SUCCESS');
    }
    expect(auditUnverified(db).count).toBe(0);
  });

  it('transitions a PENDING task to FAILED', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { runId, agentId: 'agent-a', status: 'FAILED' });
    expect(result.ok).toBe(true);
    const row = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId);
    expect((row as { status: string }).status).toBe('FAILED');
  });

  it('explicitly marks a stale ACTIVE run FAILED with an evidence receipt', () => {
    const db = freshDb();
    const claim = preFlightIntent(db, {
      agentId: 'agent-a', workspacePath: '/tmp/ws-a', targetFiles: ['/tmp/stale-active.ts'],
    });
    if (!claim.ok) throw new Error('claim failed');
    db.prepare('DELETE FROM awareness_locks WHERE run_id = ?').run(claim.run.run_id);
    db.prepare('UPDATE run_files SET expires_at = ? WHERE run_id = ?')
      .run('2000-01-01T00:00:00Z', claim.run.run_id);

    expect(markVerified(db, {
      runId: claim.run.run_id,
      agentId: 'agent-a',
      status: 'FAILED',
      message: 'expired presence confirmed; work was not completed',
    })).toMatchObject({ ok: true, run_id: claim.run.run_id, status: 'FAILED' });
  });

  it('never marks a live ACTIVE run FAILED or any ACTIVE run SUCCESS', () => {
    const db = freshDb();
    const claim = preFlightIntent(db, {
      agentId: 'agent-a', workspacePath: '/tmp/ws-a', targetFiles: ['/tmp/live-active.ts'],
    });
    if (!claim.ok) throw new Error('claim failed');
    expect(markVerified(db, {
      runId: claim.run.run_id, agentId: 'agent-a', status: 'FAILED', message: 'premature failure',
    })).toMatchObject({ ok: false });
    expect(markVerified(db, {
      runId: claim.run.run_id, agentId: 'agent-a', status: 'SUCCESS', message: 'premature success',
    })).toMatchObject({ ok: false });
    expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id))
      .toEqual({ status: 'ACTIVE' });
  });

  it('defaults to SUCCESS when status is omitted but still requires a receipt', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { runId, agentId: 'agent-a', message: 'default success check passed' });
    expect(result.ok).toBe(true);
    const row = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId);
    expect((row as { status: string }).status).toBe('SUCCESS');
  });

  it('returns ok=false for an unknown run_id — not silent ok', () => {
    const db = freshDb();
    const result = markVerified(db, {
      runId: 'task_does-not-exist',
      agentId: 'agent-a',
      status: 'SUCCESS',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeTruthy();
  });

  it('returns ok=false when the intent belongs to a different agent', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { runId, agentId: 'agent-b', status: 'SUCCESS', message: 'checked by peer' });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('--adopt-verification');
  });

  it('allows explicit single-run verification adoption in the same workspace', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, {
      runId,
      agentId: 'agent-b',
      workspacePath: '/tmp/ws-a',
      status: 'SUCCESS',
      message: 'peer reran declared check',
      adoptVerification: true,
    });
    expect(result).toMatchObject({ ok: true, run_id: runId, status: 'SUCCESS' });
    expect(auditUnverified(db, { workspacePath: '/tmp/ws-a' }).count).toBe(0);
    const log = db.prepare('SELECT agent_id, message FROM run_log WHERE run_id = ?').get(runId) as { agent_id: string; message: string };
    expect(log.agent_id).toBe('agent-b');
    expect(log.message).toContain('verification adopted by agent-b from agent-a');
  });

  it('rejects verification adoption outside the run workspace', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, {
      runId,
      agentId: 'agent-b',
      workspacePath: '/tmp/ws-b',
      status: 'SUCCESS',
      message: 'wrong workspace check',
      adoptVerification: true,
    });
    expect(result.ok).toBe(false);
    const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
    expect(task.status).toBe('PENDING');
  });

  it('returns ok=false for an invalid status value', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    // PENDING is not a valid verify status (can't "verify" into PENDING)
    const result = markVerified(db, {
      runId,
      agentId: 'agent-a',
      status: 'PENDING' as 'SUCCESS',
    });
    expect(result.ok).toBe(false);
  });

  it('rejects invalid allPending status before mutating pending tasks', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, {
      agentId: 'agent-a',
      allPending: true,
      workspacePath: '/tmp/ws-a',
      status: 'PENDING' as 'SUCCESS',
    });
    expect(result.ok).toBe(false);
    const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
    expect(task.status).toBe('PENDING');
  });

  it('returns ok=false when verifying an already-SUCCESS intent — not PENDING', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    const first = markVerified(db, { runId, agentId: 'agent-a', status: 'SUCCESS', message: 'first verification passed' });
    expect(first.ok).toBe(true);
    // Second verify attempt: intent is now SUCCESS, not PENDING
    const second = markVerified(db, { runId, agentId: 'agent-a', status: 'SUCCESS', message: 'duplicate verification attempt' });
    expect(second.ok).toBe(false);
  });

  it('allPending verifies two pending runs atomically', () => {
    const db = freshDb();
    const a = makePending(db, 'agent-a', '/tmp/ws-a');
    const b = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, {
      agentId: 'agent-a',
      allPending: true,
      workspacePath: '/tmp/ws-a',
      status: 'SUCCESS',
      message: 'both declared checks passed',
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.count).toBe(2);
      expect(result.run_ids?.sort()).toEqual([a, b].sort());
    }
    expect(db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'SUCCESS' AND agent_id = 'agent-a'")
      .get()).toEqual({ c: 2 });
    expect(auditUnverified(db, { agentId: 'agent-a', workspacePath: '/tmp/ws-a' }).count).toBe(0);
  });

  it('allPending rolls back when a later finish step fails', () => {
    const db = freshDb();
    const a = makePending(db, 'agent-a', '/tmp/ws-a');
    const b = makePending(db, 'agent-a', '/tmp/ws-a');
    // Link both runs to VERIFY tasks so finishLinkedTask inserts task_events.
    for (const runId of [a, b]) {
      const taskId = `task_${runId.slice(4)}`;
      db.prepare(`INSERT INTO awareness_plans(plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
        VALUES (?, 'p', 'o', 'lead', 'ACTIVE', '/tmp/ws-a', '/tmp/docs', datetime('now'), datetime('now'))`)
        .run(`plan_${runId}`);
      db.prepare(`INSERT INTO awareness_tasks(task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
        VALUES (?, ?, 't', 'r', 'a', 'VERIFY', 0, 'lead', datetime('now'), datetime('now'))`)
        .run(taskId, `plan_${runId}`);
      db.prepare('UPDATE task_runs SET task_id = ? WHERE run_id = ?').run(taskId, runId);
    }
    // Abort on the second task_events insert so the batch must roll back.
    db.exec(`CREATE TRIGGER reject_second_task_event BEFORE INSERT ON task_events
      WHEN (SELECT COUNT(*) FROM task_events) >= 1
      BEGIN SELECT RAISE(ABORT, 'forced finishLinkedTask failure'); END`);

    expect(() => markVerified(db, {
      agentId: 'agent-a',
      allPending: true,
      workspacePath: '/tmp/ws-a',
      status: 'SUCCESS',
      message: 'batch checks passed before injected failure',
    })).toThrow(/forced finishLinkedTask failure/);

    expect(db.prepare("SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING' AND agent_id = 'agent-a'")
      .get()).toEqual({ c: 2 });
    expect(db.prepare("SELECT COUNT(*) AS c FROM awareness_tasks WHERE status = 'VERIFY'").get()).toEqual({ c: 2 });
    expect(auditUnverified(db, { agentId: 'agent-a', workspacePath: '/tmp/ws-a' }).count).toBe(2);
  });

  it('allPending UPDATE is agent-scoped — tampered agent_id is not flipped', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    // Simulate TOCTOU: row still PENDING but now owned by another agent.
    db.prepare("UPDATE task_runs SET agent_id = 'agent-b' WHERE run_id = ?").run(runId);
    const result = markVerified(db, {
      agentId: 'agent-a',
      allPending: true,
      workspacePath: '/tmp/ws-a',
      status: 'SUCCESS',
      message: 'scoped batch check passed',
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.count).toBe(0);
      expect(result.run_ids ?? []).toEqual([]);
    }
    expect(db.prepare('SELECT status, agent_id FROM task_runs WHERE run_id = ?').get(runId))
      .toEqual({ status: 'PENDING', agent_id: 'agent-b' });
  });
});
