import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { canonicalizePath } from '../src/git.js';
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

describe('auditUnverified', () => {
  it('returns empty on a fresh DB', () => {
    const db = freshDb();
    const result = auditUnverified(db);
    expect(result.ok).toBe(true);
    expect(result.count).toBe(0);
    expect(result.unverified).toEqual([]);
  });

  it('minAgeMs grace excludes just-created runs but includes them once past the window', () => {
    const db = freshDb();
    makePending(db, 'agent-grace', '/tmp/ws-grace');
    // A large grace window hides the fresh run…
    expect(auditUnverified(db, { minAgeMs: 60_000 }).count).toBe(0);
    // …while no/zero grace still reports it (gate behavior is unchanged).
    expect(auditUnverified(db, { minAgeMs: 0 }).count).toBe(1);
    expect(auditUnverified(db).count).toBe(1);
  });

  it('rejects a negative minAgeMs', () => {
    const db = freshDb();
    expect(() => auditUnverified(db, { minAgeMs: -1 })).toThrow(/minAgeMs/);
  });

  it('ignores ACTIVE tasks — only PENDING is unverified', () => {
    const db = freshDb();
    // Claim a lock but do NOT release it -> task stays ACTIVE
    preFlightIntent(db, {
      agentId: 'agent-a',
      workspacePath: '/tmp/ws-a',
      targetFiles: ['/tmp/active.txt'],
    });
    const result = auditUnverified(db);
    expect(result.count).toBe(0);
  });

  it('returns PENDING tasks with run_id, status, and test_plan', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a', 'run vitest + lint');
    const result = auditUnverified(db);
    expect(result.count).toBe(1);
    expect(result.unverified[0]).toMatchObject({
      run_id: runId,
      status: 'PENDING',
      test_plan: 'run vitest + lint',
      target_files: [canonicalizePath('/tmp/agent-a-target.txt')],
    });
  });

  it('verification terminally closes any remaining presence and lock rows', () => {
    const db = freshDb();
    const runId = makePending(db, 'agent-a', '/tmp/ws-a');
    db.prepare(`UPDATE run_files
      SET ended_at = NULL, expires_at = '2099-01-01T00:00:00Z'
      WHERE run_id = ?`).run(runId);
    db.prepare(`INSERT INTO awareness_locks(lock_id, file_path, run_id, acquired_at, expires_at)
      VALUES ('lock_late', '/tmp/agent-a-target.txt', ?, '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')`)
      .run(runId);

    expect(markVerified(db, { runId, agentId: 'agent-a', status: 'SUCCESS', message: 'verified late presence cleanup' }).ok).toBe(true);
    expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_locks WHERE run_id = ?').get(runId))
      .toEqual({ count: 0 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL').get(runId))
      .toEqual({ count: 0 });
  });

  it('filters by agentId — only returns that agent\'s PENDING tasks', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { agentId: 'agent-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.run_id).toBe(aId);
  });

  it('filters by workspacePath — only returns that workspace\'s PENDING tasks', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { workspacePath: '/tmp/ws-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.run_id).toBe(aId);
  });

  it('filters by both agentId and workspacePath', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { agentId: 'agent-a', workspacePath: '/tmp/ws-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.run_id).toBe(aId);
  });

  it('returns all PENDING when no filter given', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a');
    const bId = makePending(db, 'agent-b', '/tmp/ws-b');
    const result = auditUnverified(db);
    expect(result.count).toBe(2);
    expect(result.unverified.map(u => u.run_id).sort()).toEqual([aId, bId].sort());
  });

  it('can inspect only age-qualified debt without mutating it', () => {
    const db = freshDb();
    const oldId = makePending(db, 'agent-a', '/tmp/ws-a');
    const freshId = makePending(db, 'agent-a', '/tmp/ws-a');
    db.prepare('UPDATE task_runs SET updated_at = ? WHERE run_id = ?')
      .run('2020-01-01T00:00:00Z', oldId);
    const result = auditUnverified(db, {
      agentId: 'agent-a', workspacePath: '/tmp/ws-a', olderThanDays: 1,
    });
    expect(result.unverified.map((run) => run.run_id)).toEqual([oldId]);
    expect(result.unverified.map((run) => run.run_id)).not.toContain(freshId);
  });

  it('filters legacy HOOK debt by origin and creation cutoff without touching WORK', () => {
    const db = freshDb();
    const hookId = makePending(db, 'legacy-agent', '/tmp/ws-a');
    const workId = makePending(db, 'legacy-agent', '/tmp/ws-a');
    db.prepare("UPDATE task_runs SET origin = 'HOOK', created_at = '2020-01-01T00:00:00Z' WHERE run_id = ?")
      .run(hookId);
    db.prepare("UPDATE task_runs SET origin = 'WORK', created_at = '2020-01-01T00:00:00Z' WHERE run_id = ?")
      .run(workId);

    const migrated = auditUnverified(db, {
      workspacePath: '/tmp/ws-a', origins: ['HOOK'], before: '2021-01-01T00:00:00Z',
    });
    expect(migrated.unverified.map((run) => run.run_id)).toEqual([hookId]);
    expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(hookId)).toEqual({ status: 'PENDING' });
    expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(workId)).toEqual({ status: 'PENDING' });
  });
});
