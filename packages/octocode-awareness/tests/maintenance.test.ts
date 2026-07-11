/**
 * maintenance.test.ts — Behavioural tests for maintenance functions against the current schema.
 *
 * Core tables: memories, tasks, locks.
 * Core columns: importance, run_id, tags_json, memory_refs.
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb } from '../src/db.js';
import { pruneStale, getWorkspaceStatus } from '../src/maintenance.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
/** Insert a memory using the memories table. */
function insertMem(db: DatabaseSync, opts: {
    memoryId?: string;
    importance?: number;
    label?: string;
    tags?: string[];
    failureSig?: string;
    observation?: string;
    workspacePath?: string | null;
} = {}): string {
    const memoryId = opts.memoryId ?? 'mem_' + randomUUID().replace(/-/g, '');
    const now = new Date().toISOString();
    db.prepare(`
    INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance,
      label, tags_json, workspace_path, failure_signature, created_at
    ) VALUES (?, 'agent-test', 'test context', ?, ?, ?, ?, ?, ?, ?)
  `).run(memoryId, opts.observation ?? 'test observation', opts.importance ?? 5, opts.label ?? 'OTHER', JSON.stringify(opts.tags ?? []), opts.workspacePath ?? null, opts.failureSig ?? null, now);
    return memoryId;
}
/** Insert an ACTIVE task and return its run_id. */
function insertTask(db: DatabaseSync, opts: {
    agentId?: string;
    workspacePath?: string;
    sessionId?: string | null;
    planDocRef?: string | null;
} = {}): string {
    const runId = 'task_' + randomUUID().replace(/-/g, '');
    const now = new Date().toISOString();
    db.prepare(`
    INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, context_ref, status, workspace_path, created_at, updated_at)
    VALUES (?, 'WORK', ?, 'test rationale', 'yarn test', ?, 'ACTIVE', ?, ?, ?)
  `).run(runId, opts.agentId ?? 'agent-test', opts.planDocRef ?? null, opts.workspacePath ?? '/ws', now, now);
    return runId;
}
/** Insert a lock for a task. */
function insertLock(db: DatabaseSync, opts: {
    runId: string;
    filePath?: string;
    agentId?: string;
    expiresAt?: string | null;
}): string {
    const lockId = 'lock_' + randomUUID().replace(/-/g, '');
    const now = new Date().toISOString();
    db.prepare(`
    INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
    VALUES (?, ?, ?, ?, ?)
  `).run(lockId, opts.filePath ?? '/ws/a.ts', opts.runId, now, opts.expiresAt ?? null);
    return lockId;
}

// ─── 1. pruneStale — uses locks + tasks ──────────────────────────────────────

describe('pruneStale — locks + tasks', () => {
  it('dry_run returns would_prune without deleting', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    const res = pruneStale(db, { dry_run: true });
    expect(res.dry_run).toBe(true);
    expect(res.would_prune).toBeGreaterThanOrEqual(1);
    expect(res.pruned_locks).toBe(0);

    // Nothing deleted
    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(1);
  });

  it('prunes expired locks from the locks table', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    const res = pruneStale(db, {});
    expect(res.pruned_locks).toBeGreaterThanOrEqual(1);

    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(0);
  });

  it('does not end independent work when its expired exclusive lock is pruned', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    pruneStale(db, {});

    const task = db.prepare(
      'SELECT status FROM task_runs WHERE run_id = ?'
    ).get(runId) as { status: string } | undefined;
    expect(task?.status).toBe('ACTIVE');
  });

  it('normalizes relative target_file filters against workspace', () => {
    const db = freshDb();
    const runId = insertTask(db, { workspacePath: '/repo' });
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, filePath: '/repo/src/a.ts', expiresAt: past });

    const dry = pruneStale(db, { workspace: '/repo', target_file: 'src/a.ts', dry_run: true });
    expect(dry.would_prune).toBe(1);

    const res = pruneStale(db, { workspace: '/repo', target_file: 'src/a.ts' });
    expect(res.pruned_locks).toBe(1);
    const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
    expect(task.status).toBe('ACTIVE');
  });

  it('does not prune non-expired locks', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const future = new Date(Date.now() + 10 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: future });

    const res = pruneStale(db, {});
    expect(res.pruned_locks).toBe(0);

    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(1);
  });
});

// ─── 2. getWorkspaceStatus — uses memories + tasks + locks ────────────────────

describe('getWorkspaceStatus — current schema', () => {
  it('returns active_memories count from the memories table', () => {
    const db = freshDb();
    insertMem(db);
    insertMem(db);

    const status = getWorkspaceStatus(db, {});
    expect(status.ok).toBe(true);
    expect(status.active_memories).toBeGreaterThanOrEqual(2);
  });

  it('returns pending_runs count from the tasks table', () => {
    const db = freshDb();
    const runId = insertTask(db);
    db.prepare("UPDATE task_runs SET status = 'PENDING' WHERE run_id = ?").run(runId);

    const status = getWorkspaceStatus(db, {});
    expect(status.pending_runs).toBeGreaterThanOrEqual(1);
  });

  it('returns active_runs count from the tasks table', () => {
    const db = freshDb();
    insertTask(db);

    const status = getWorkspaceStatus(db, {});
    expect(status.active_runs).toBeGreaterThanOrEqual(1);
  });

  it('returns active locks from locks table', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const future = new Date(Date.now() + 10 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: future });

    const status = getWorkspaceStatus(db, {});
    expect(status.lock_count).toBe(1);
    expect(status.locks.length).toBeGreaterThanOrEqual(1);
    expect(status.locks[0]).toHaveProperty('file_path');
    expect(status.locks[0]).toHaveProperty('agent_id');
  });

  it('filters expired locks without mutating status during a read', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    const status = getWorkspaceStatus(db, {});
    expect(status.lock_count).toBe(0);
    expect(status.locks).toHaveLength(0);
    expect(status.pending_runs).toBe(0);
    expect(status.active_runs).toBe(1);
    const lockCount = db.prepare('SELECT COUNT(*) AS count FROM locks').get() as { count: number };
    expect(lockCount.count).toBe(1);
    const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
    expect(task.status).toBe('ACTIVE');
  });
});
