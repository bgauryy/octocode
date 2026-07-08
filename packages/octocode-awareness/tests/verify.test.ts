import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { auditUnverified, markVerified } from '../src/verify.js';

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
  releaseFileLock(db, { agentId, taskId: claim.task.task_id, status: 'PENDING' });
  return claim.task.task_id;
}

describe('auditUnverified', () => {
  it('returns empty on a fresh DB', () => {
    const db = freshDb();
    const result = auditUnverified(db);
    expect(result.ok).toBe(true);
    expect(result.count).toBe(0);
    expect(result.unverified).toEqual([]);
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

  it('returns PENDING tasks with task_id, status, and test_plan', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a', 'run vitest + lint');
    const result = auditUnverified(db);
    expect(result.count).toBe(1);
    expect(result.unverified[0]).toMatchObject({
      task_id: taskId,
      status: 'PENDING',
      test_plan: 'run vitest + lint',
    });
  });

  it('filters by agentId — only returns that agent\'s PENDING tasks', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { agentId: 'agent-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.task_id).toBe(aId);
  });

  it('filters by workspacePath — only returns that workspace\'s PENDING tasks', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { workspacePath: '/tmp/ws-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.task_id).toBe(aId);
  });

  it('filters by both agentId and workspacePath', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { agentId: 'agent-a', workspacePath: '/tmp/ws-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.task_id).toBe(aId);
  });

  it('returns all PENDING when no filter given', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a');
    const bId = makePending(db, 'agent-b', '/tmp/ws-b');
    const result = auditUnverified(db);
    expect(result.count).toBe(2);
    expect(result.unverified.map(u => u.task_id).sort()).toEqual([aId, bId].sort());
  });
});

describe('markVerified', () => {
  it('transitions a PENDING task to SUCCESS and clears it from auditUnverified', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    expect(auditUnverified(db).count).toBe(1);

    const result = markVerified(db, { taskId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.task_id).toBe(taskId);
      expect(result.status).toBe('SUCCESS');
    }
    expect(auditUnverified(db).count).toBe(0);
  });

  it('transitions a PENDING task to FAILED', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { taskId, agentId: 'agent-a', status: 'FAILED' });
    expect(result.ok).toBe(true);
    const row = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId);
    expect((row as { status: string }).status).toBe('FAILED');
  });

  it('defaults to SUCCESS when status is omitted', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { taskId, agentId: 'agent-a' });
    expect(result.ok).toBe(true);
    const row = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId);
    expect((row as { status: string }).status).toBe('SUCCESS');
  });

  it('returns ok=false for an unknown task_id — not silent ok', () => {
    const db = freshDb();
    const result = markVerified(db, {
      taskId: 'task_does-not-exist',
      agentId: 'agent-a',
      status: 'SUCCESS',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeTruthy();
  });

  it('returns ok=false when the intent belongs to a different agent', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { taskId, agentId: 'agent-b', status: 'SUCCESS' });
    expect(result.ok).toBe(false);
  });

  it('returns ok=false for an invalid status value', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    // PENDING is not a valid verify status (can't "verify" into PENDING)
    const result = markVerified(db, {
      taskId,
      agentId: 'agent-a',
      status: 'PENDING' as 'SUCCESS',
    });
    expect(result.ok).toBe(false);
  });

  it('rejects invalid allPending status before mutating pending tasks', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, {
      agentId: 'agent-a',
      allPending: true,
      workspacePath: '/tmp/ws-a',
      status: 'PENDING' as 'SUCCESS',
    });
    expect(result.ok).toBe(false);
    const task = db.prepare('SELECT status FROM tasks WHERE task_id = ?').get(taskId) as { status: string };
    expect(task.status).toBe('PENDING');
  });

  it('returns ok=false when verifying an already-SUCCESS intent — not PENDING', () => {
    const db = freshDb();
    const taskId = makePending(db, 'agent-a', '/tmp/ws-a');
    const first = markVerified(db, { taskId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(first.ok).toBe(true);
    // Second verify attempt: intent is now SUCCESS, not PENDING
    const second = markVerified(db, { taskId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(second.ok).toBe(false);
  });
});

describe('workspace-scope symlink stability (regression)', () => {
  // auditUnverified/markVerified used to filter on the raw workspacePath with
  // no normalization, unlike preFlightIntent/releaseFileLock which resolve
  // through fillScope. A task released (and thus stored) via a symlinked
  // workspace path could silently never show up in an audit filtered by the
  // real path (or vice versa) — same bug as the memory subdirectory-recall
  // regression, but for the verify-gate.
  function tempDirWithLink(): { real: string; link: string; base: string } {
    const base = mkdtempSync(join(tmpdir(), 'oc-verify-scope-'));
    const real = join(base, 'real');
    const link = join(base, 'link');
    mkdirSync(real, { recursive: true });
    symlinkSync(real, link);
    return { real, link, base };
  }

  it('a task released via a symlinked workspace path is audited via the real path', () => {
    const db = freshDb();
    const { real, link, base } = tempDirWithLink();
    try {
      const taskId = makePending(db, 'agent-a', link, 'verify-symlink-fix');
      const result = auditUnverified(db, { workspacePath: real });
      expect(result.count).toBe(1);
      expect(result.unverified[0]?.task_id).toBe(taskId);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it('markVerified --workspace via the real path clears a task released via the symlink', () => {
    const db = freshDb();
    const { real, link, base } = tempDirWithLink();
    try {
      makePending(db, 'agent-a', link, 'verify-symlink-mark');
      const result = markVerified(db, { agentId: 'agent-a', allPending: true, workspacePath: real });
      expect(result.ok).toBe(true);
      if (result.ok) expect(result.count).toBe(1);
      expect(auditUnverified(db, { workspacePath: real }).count).toBe(0);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });
});
