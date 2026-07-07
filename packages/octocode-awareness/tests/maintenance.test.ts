/**
 * maintenance.test.ts — Behavioural tests for maintenance functions against the new schema.
 *
 * All functions must work with the new table names:
 *   memories (not agent_memories)
 *   tasks    (not agent_intents)
 *   locks    (not file_locks)
 *
 * Column renames:
 *   importance_score → importance
 *   intent_id        → task_id
 *   tags_text        → (gone; use tags_json JSON array)
 *   references_json  → (gone; use memory_refs table)
 */

import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb } from '../src/db.js';
import {
  pruneStale,
  notifyGet,
  exportHarness,
  getWorkspaceStatus,
  sessionCapture,
  digest,
} from '../src/maintenance.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

/** Insert a memory using the NEW memories table. */
function insertMem(
  db: DatabaseSync,
  opts: {
    memoryId?: string;
    importance?: number;
    label?: string;
    tags?: string[];
    failureSig?: string;
    observation?: string;
    workspacePath?: string | null;
  } = {},
): string {
  const memoryId = opts.memoryId ?? 'mem_' + randomUUID().replace(/-/g, '');
  const now = new Date().toISOString();
  db.prepare(`
    INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance,
      label, tags_json, workspace_path, failure_signature, created_at
    ) VALUES (?, 'agent-test', 'test context', ?, ?, ?, ?, ?, ?, ?)
  `).run(
    memoryId,
    opts.observation ?? 'test observation',
    opts.importance ?? 5,
    opts.label ?? 'OTHER',
    JSON.stringify(opts.tags ?? []),
    opts.workspacePath ?? null,
    opts.failureSig ?? null,
    now,
  );
  return memoryId;
}

/** Insert an ACTIVE task and return its task_id. */
function insertTask(
  db: DatabaseSync,
  opts: { agentId?: string; workspacePath?: string; sessionId?: string | null } = {},
): string {
  const taskId = 'task_' + randomUUID().replace(/-/g, '');
  const now = new Date().toISOString();
  db.prepare(`
    INSERT INTO tasks (task_id, agent_id, rationale, test_plan, status, workspace_path, files_json, created_at, updated_at)
    VALUES (?, ?, 'test rationale', 'yarn test', 'ACTIVE', ?, '[]', ?, ?)
  `).run(taskId, opts.agentId ?? 'agent-test', opts.workspacePath ?? '/ws', now, now);
  return taskId;
}

/** Insert a lock for a task. */
function insertLock(
  db: DatabaseSync,
  opts: { taskId: string; filePath?: string; agentId?: string; expiresAt?: string | null },
): string {
  const lockId = 'lock_' + randomUUID().replace(/-/g, '');
  const now = new Date().toISOString();
  db.prepare(`
    INSERT INTO locks (lock_id, file_path, task_id, agent_id, lock_type, acquired_at, expires_at)
    VALUES (?, ?, ?, ?, 'EXCLUSIVE', ?, ?)
  `).run(
    lockId,
    opts.filePath ?? '/ws/a.ts',
    opts.taskId,
    opts.agentId ?? 'agent-test',
    now,
    opts.expiresAt ?? null,
  );
  return lockId;
}

// ─── 1. pruneStale — uses locks + tasks (not file_locks / agent_intents) ──────

describe('pruneStale — new schema (locks + tasks)', () => {
  it('dry_run returns would_prune without deleting', () => {
    const db = freshDb();
    const taskId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { taskId, expiresAt: past });

    const res = pruneStale(db, { dry_run: true });
    expect(res.dry_run).toBe(true);
    expect(res.would_prune).toBeGreaterThanOrEqual(1);
    expect(res.pruned_locks).toBe(0);

    // Nothing deleted
    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(1);
  });

  it('prunes expired locks from the locks table (not file_locks)', () => {
    const db = freshDb();
    const taskId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { taskId, expiresAt: past });

    const res = pruneStale(db, {});
    expect(res.pruned_locks).toBeGreaterThanOrEqual(1);

    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(0);
  });

  it('updates task status to PENDING in the tasks table when its last lock is pruned', () => {
    const db = freshDb();
    const taskId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { taskId, expiresAt: past });

    pruneStale(db, {});

    const task = db.prepare(
      'SELECT status FROM tasks WHERE task_id = ?'
    ).get(taskId) as { status: string } | undefined;
    expect(task?.status).toBe('PENDING');
  });

  it('does not prune non-expired locks', () => {
    const db = freshDb();
    const taskId = insertTask(db);
    const future = new Date(Date.now() + 10 * 60_000).toISOString();
    insertLock(db, { taskId, expiresAt: future });

    const res = pruneStale(db, {});
    expect(res.pruned_locks).toBe(0);

    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(1);
  });
});

// ─── 2. getWorkspaceStatus — uses memories + tasks + locks ────────────────────

describe('getWorkspaceStatus — new schema', () => {
  it('returns active_memories count from the memories table', () => {
    const db = freshDb();
    insertMem(db);
    insertMem(db);

    const status = getWorkspaceStatus(db, {});
    expect(status.ok).toBe(true);
    expect(status.active_memories).toBeGreaterThanOrEqual(2);
  });

  it('returns pending_tasks count from the tasks table', () => {
    const db = freshDb();
    const taskId = insertTask(db);
    db.prepare("UPDATE tasks SET status = 'PENDING' WHERE task_id = ?").run(taskId);

    const status = getWorkspaceStatus(db, {});
    expect(status.pending_tasks).toBeGreaterThanOrEqual(1);
  });

  it('returns active_tasks count from the tasks table', () => {
    const db = freshDb();
    insertTask(db);

    const status = getWorkspaceStatus(db, {});
    expect(status.active_tasks).toBeGreaterThanOrEqual(1);
  });

  it('returns active locks from locks table (not file_locks)', () => {
    const db = freshDb();
    const taskId = insertTask(db);
    const future = new Date(Date.now() + 10 * 60_000).toISOString();
    insertLock(db, { taskId, expiresAt: future });

    const status = getWorkspaceStatus(db, {});
    expect(status.locks.length).toBeGreaterThanOrEqual(1);
    expect(status.locks[0]).toHaveProperty('file_path');
    expect(status.locks[0]).toHaveProperty('agent_id');
  });
});

// ─── 3. notifyGet — reads from memories (not agent_memories) ─────────────────

describe('notifyGet — smart briefing from memories table', () => {
  it('returns empty briefing when no memories exist', () => {
    const db = freshDb();
    const res = notifyGet(db, { agent_id: 'agent-a', workspace: '/ws' });
    expect(res.ok).toBe(true);
  });

  it('surfaces high-importance memories from memories table using importance column (not importance_score)', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      observation: 'always check token expiry',
      workspacePath: '/ws',
    });

    const res = notifyGet(db, { agent_id: 'agent-b', workspace: '/ws' }) as {
      ok: true; count: number; notifications: Array<{ kind: string; text: string }>;
    };
    expect(res.ok).toBe(true);
    // The briefing should surface the GOTCHA memory
    expect(res.count).toBeGreaterThanOrEqual(1);
    expect(res.notifications.some(n => n.text.includes('GOTCHA'))).toBe(true);
  });

  it('surfaces weakness cluster when failure_signature is present', () => {
    const db = freshDb();
    const sig = 'mechanism:test-timeout|cause:slow-io';
    insertMem(db, { failureSig: sig, importance: 6, workspacePath: '/ws' });
    insertMem(db, { failureSig: sig, importance: 6, workspacePath: '/ws' });

    const res = notifyGet(db, { agent_id: 'agent-c', workspace: '/ws' }) as {
      ok: true; count: number; notifications: Array<{ kind: string; text: string }>;
    };
    expect(res.notifications.some(n => n.kind === 'weakness')).toBe(true);
  });
});

// ─── 4. exportHarness — JSON tag matching, no tags_text column ────────────────

describe('exportHarness — new schema tag matching', () => {
  it('surfaces harness-tagged memories using tags_json (not tags_text)', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      tags: ['reflection', 'harness'],
      observation: 'run mine-weakness before export-harness',
    });

    const res = exportHarness(db, {});
    expect(res.count).toBeGreaterThanOrEqual(1);
    expect(res.memories.some(m => m.tier === 'harness')).toBe(true);
  });

  it('does not include non-harness memories in tier-1', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 9,
      label: 'DECISION',
      tags: ['architecture'],
      observation: 'use SQLite for local memory',
    });

    const res = exportHarness(db, { harness_only: true });
    // harness_only=true → only tier-1; DECISION without 'harness' tag must be absent
    const harnessCount = res.memories.filter(m => m.tier === 'harness').length;
    expect(harnessCount).toBe(0);
  });

  it('surfaces high-importance general memories in tier-2', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'DECISION',
      tags: [],
      observation: 'always validate before conclude',
    });

    const res = exportHarness(db, { min_importance: 7 });
    expect(res.memories.some(m => m.tier === 'general')).toBe(true);
  });

  it('returns empty markdown when no qualifying memories exist', () => {
    const db = freshDb();
    const res = exportHarness(db, {});
    expect(res.count).toBe(0);
    expect(res.markdown).toContain('No harness');
  });
});

// ─── 5. sessionCapture — uses tasks table (not agent_intents) ─────────────────

describe('sessionCapture — tasks table', () => {
  it('returns captured=false when no active/pending tasks exist', () => {
    const db = freshDb();
    const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    expect(res.ok).toBe(true);
    expect(res.captured).toBe(false);
    expect(res.refinement_id).toBeNull();
  });

  it('captures active tasks from the tasks table and creates a handoff refinement', () => {
    const db = freshDb();
    insertTask(db, { agentId: 'agent-cap', workspacePath: '/ws' });

    const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    expect(res.ok).toBe(true);
    expect(res.captured).toBe(true);
    expect(res.active_tasks).toBeGreaterThanOrEqual(1);
    expect(res.refinement_id).toBeTruthy();

    // Verify the refinement was written to the refinements table
    const ref = db.prepare(
      "SELECT quality, state FROM refinements WHERE refinement_id = ?"
    ).get(res.refinement_id!) as { quality: string; state: string } | undefined;
    expect(ref?.quality).toBe('handoff');
    expect(ref?.state).toBe('open');
  });
});

// ─── 6. digest — works with new schema tables ──────────────────────────────────

describe('digest — dry_run with new schema', () => {
  it('dry_run returns counts without mutating', () => {
    const db = freshDb();

    // Add a SUPERSEDED memory older than 90d
    const oldDate = new Date(Date.now() - 91 * 86400000).toISOString();
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, created_at, updated_at)
      VALUES ('mem_old', 'agent-x', 'old task', 'old observation', 3, 'SUPERSEDED', ?, ?)
    `).run(oldDate, oldDate);

    const res = digest(db, { dry_run: true });
    expect(res.ok).toBe(true);
    expect(res.dry_run).toBe(true);
    expect(typeof res.would_prune_old).toBe('number');

    // Nothing deleted in dry_run
    const row = db.prepare("SELECT state FROM memories WHERE memory_id = 'mem_old'").get() as { state: string } | undefined;
    expect(row?.state).toBe('SUPERSEDED');
  });
});
