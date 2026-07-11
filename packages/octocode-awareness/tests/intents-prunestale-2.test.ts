import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { canonicalizePath } from '../src/git.js';
import { fileLock, preFlightIntent, releaseFileLock } from '../src/intents.js';
import { pruneStale } from '../src/maintenance.js';
import { startWork } from '../src/work.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
function tempFile(): {
    dir: string;
    path: string;
    cleanup: () => void;
} {
    const dir = mkdtempSync(join(tmpdir(), 'oc-intent-test-'));
    const path = join(dir, 'target.txt');
    writeFileSync(path, 'seed');
    return { dir, path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

describe('pruneStale', () => {
  it('deletes expired locks without ending advisory work', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 60_000 });
      if (!claim.ok) throw new Error('claim failed');
      const runId = claim.run.run_id;

      // Force-expire the lock so pruneStale will pick it up
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(past, runId);

      // Verify preconditions
      const locksBefore = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE run_id = ?').get(runId) as { c: number };
      expect(locksBefore.c).toBe(1);
      const taskBefore = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
      expect(taskBefore.status).toBe('ACTIVE');

      const result = pruneStale(db);

      // Both mutations reflected in the return value
      expect(result.pruned_locks).toBeGreaterThan(0);
      expect(result).toEqual({ pruned_locks: 1 });

      // Both mutations visible in the DB simultaneously
      const locksAfter = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE run_id = ?').get(runId) as { c: number };
      expect(locksAfter.c).toBe(0);
      const taskAfter = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
      expect(taskAfter.status).toBe('ACTIVE');
    } finally { cleanup(); }
  });

  it('rechecks stale locks at prune time so renewed locks survive', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 60_000 });
      if (!claim.ok) throw new Error('claim failed');
      const runId = claim.run.run_id;
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(past, runId);
      expect(pruneStale(db, { dry_run: true }).would_prune).toBe(1);

      const future = new Date(Date.now() + 60_000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(future, runId);
      const result = pruneStale(db);
      expect(result.pruned_locks).toBe(0);
      const lockCount = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE run_id = ?').get(runId) as { c: number };
      expect(lockCount.c).toBe(1);
    } finally { cleanup(); }
  });

  it('dry_run reports would_prune without mutating', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path], ttlMs: 60_000 });
      if (!claim.ok) throw new Error('claim failed');
      const runId = claim.run.run_id;

      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?').run(past, runId);

      const result = pruneStale(db, { dryRun: true });
      expect(result.dry_run).toBe(true);
      expect(result.would_prune).toBeGreaterThan(0);
      expect(result.pruned_locks).toBe(0);

      // Lock must still exist after dry run
      const locksAfter = db.prepare('SELECT COUNT(*) AS c FROM locks WHERE run_id = ?').get(runId) as { c: number };
      expect(locksAfter.c).toBe(1);
    } finally { cleanup(); }
  });
});

describe('fileLock', () => {
  it('rejects late renewal after expired exclusivity is reacquired by another run', () => {
    const db = freshDb();
    const { dir, cleanup } = tempFile();
    try {
      const first = fileLock(db, {
        type: 'lock', agentId: 'agent-a', workspacePath: dir, targetFiles: ['src/a.ts'],
      });
      if (!first.ok || first.type !== 'lock') throw new Error('first lock failed');
      db.prepare("UPDATE run_files SET expires_at = '2000-01-01T00:00:00Z' WHERE run_id = ?")
        .run(first.runId);
      db.prepare("UPDATE locks SET expires_at = '2000-01-01T00:00:00Z' WHERE run_id = ?")
        .run(first.runId);

      const second = startWork(db, {
        agentId: 'agent-b', workspacePath: dir, targetFiles: ['src/a.ts'], exclusive: true,
        rationale: 'replacement exclusive owner', testPlan: 'security suite',
      });
      if (!second.ok) throw new Error('second lock failed');

      expect(() => fileLock(db, { type: 'renew', agentId: 'agent-a', runId: first.runId }))
        .toThrow(/conflict/i);
      const active = fileLock(db, { type: 'status', workspacePath: dir });
      if (active.type !== 'status') throw new Error('status failed');
      expect(active.locks.map((lock) => lock.run_id)).toEqual([second.run.run_id]);
    } finally { cleanup(); }
  });

  it('closes every advisory presence when the final lock release makes a run pending', () => {
    const db = freshDb();
    const { dir, cleanup } = tempFile();
    try {
      const locked = fileLock(db, {
        type: 'lock', agentId: 'agent-a', workspacePath: dir, targetFiles: ['src/a.ts'],
      });
      if (!locked.ok || locked.type !== 'lock') throw new Error('lock failed');
      const attached = startWork(db, {
        agentId: 'agent-a', runId: locked.runId, workspacePath: dir, targetFiles: ['src/b.ts'],
      });
      if (!attached.ok) throw new Error('advisory attachment failed');

      releaseFileLock(db, { agentId: 'agent-a', runId: locked.runId, status: 'PENDING' });
      expect(db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(locked.runId))
        .toEqual({ status: 'PENDING' });
      expect(db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ? AND ended_at IS NULL')
        .get(locked.runId)).toEqual({ count: 0 });
    } finally { cleanup(); }
  });

  it('locks, reports status, renews, and releases by task id', () => {
    const db = freshDb();
    const { dir, cleanup } = tempFile();
    try {
      const locked = fileLock(db, {
        type: 'lock',
        agentId: 'agent-a',
        sessionId: 'session-a',
        workspacePath: dir,
        targetFiles: ['src/a.ts'],
        ttlMs: 60_000,
      });
      expect(locked.ok).toBe(true);
      expect(locked.type).toBe('lock');
      if (!locked.ok || locked.type !== 'lock') throw new Error('lock failed');
      expect(locked.runId).toMatch(/^run_/);
      expect(locked.files).toEqual([canonicalizePath(join(dir, 'src/a.ts'))]);
      expect(locked.expiresAt).toBeTruthy();

      const status = fileLock(db, { type: 'status', workspacePath: dir, sessionId: 'session-a' });
      expect(status.type).toBe('status');
      if (status.type !== 'status') throw new Error('status failed');
      expect(status.locks).toHaveLength(1);
      expect(status.locks[0]!.run_id).toBe(locked.runId);

      const beforeWrongRenew = db.prepare('SELECT heartbeat_at, expires_at FROM run_files WHERE run_id = ?')
        .get(locked.runId);
      expect(() => fileLock(db, { type: 'renew', agentId: 'agent-b', runId: locked.runId, ttlMs: 60 * 60_000 }))
        .toThrow(/belongs to agent-a/);
      expect(db.prepare('SELECT heartbeat_at, expires_at FROM run_files WHERE run_id = ?').get(locked.runId))
        .toEqual(beforeWrongRenew);

      const renewed = fileLock(db, { type: 'renew', agentId: 'agent-a', runId: locked.runId, ttlMs: 60 * 60_000 });
      expect(renewed.type).toBe('renew');
      if (renewed.type !== 'renew') throw new Error('renew failed');
      expect(renewed.renewed).toBe(true);
      expect(Date.parse(renewed.expiresAt!)).toBeGreaterThanOrEqual(Date.parse(locked.expiresAt!));

      const released = fileLock(db, { type: 'release', agentId: 'agent-a', runId: locked.runId, status: 'PENDING' });
      expect(released.type).toBe('release');
      if (released.type !== 'release') throw new Error('release failed');
      expect(released.released).toBe(true);
      const emptyStatus = fileLock(db, { type: 'status', workspacePath: dir });
      expect(emptyStatus.type).toBe('status');
      if (emptyStatus.type !== 'status') throw new Error('status failed');
      expect(emptyStatus.locks).toHaveLength(0);
    } finally { cleanup(); }
  });
});
