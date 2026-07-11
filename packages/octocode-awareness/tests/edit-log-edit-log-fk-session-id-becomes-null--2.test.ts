/**
 * edit-log.test.ts — Tests for the edit_log table via insertEditLog / queryEditLog.
 *
 * All tests use an in-memory SQLite database (fresh per test) so there is no
 * shared state between cases.
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertEditLog, queryEditLog } from '../src/audit.js';
import type { EditLogRow } from '../src/types.js';
// ─── Test helpers ──────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
/** Insert a minimal session row so FK constraints are satisfied. */
function seedSession(db: DatabaseSync, sessionId: string, agentId = 'agent-test'): void {
    db.prepare(`
    INSERT INTO sessions(session_id, agent_id, started_at)
    VALUES (?, ?, '2026-01-01T00:00:00Z')
  `).run(sessionId, agentId);
}
/** Insert a minimal task row so FK constraints are satisfied. */
function seedTask(db: DatabaseSync, runId: string, agentId = 'agent-test'): void {
    db.prepare(`
    INSERT INTO task_runs(run_id, agent_id, rationale, test_plan, status, created_at, updated_at)
    VALUES (?, ?, 'test rationale', 'test plan', 'ACTIVE', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
  `).run(runId, agentId);
}

// ─── 8. edit_log.session_id → sessions CASCADE SET NULL ──────────────────────

describe('edit_log FK — session_id becomes NULL when session is deleted', () => {
  it('sets session_id to NULL when the referenced session is deleted', () => {
    const db = freshDb();
    seedSession(db, 'sess-to-delete');

    const result = insertEditLog(db, {
      agentId: 'agent-x',
      sessionId: 'sess-to-delete',
      filePath: '/workspace/file.ts',
      operation: 'update',
    });

    // Verify FK is set before deletion
    const before = db.prepare('SELECT session_id FROM edit_log WHERE edit_id = ?').get(result.editId) as { session_id: string | null };
    expect(before.session_id).toBe('sess-to-delete');

    // Delete the session
    db.prepare('DELETE FROM sessions WHERE session_id = ?').run('sess-to-delete');

    // The edit_log row must still exist with session_id = NULL
    const after = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as EditLogRow | undefined;
    expect(after).toBeDefined();
    expect(after!.session_id).toBeNull();
    expect(after!.file_path).toBe('/workspace/file.ts');
    expect(after!.agent_id).toBe('agent-x');
  });

  it('edit_log row survives session deletion and remains queryable by agent_id', () => {
    const db = freshDb();
    seedSession(db, 'sess-ephemeral');

    const result = insertEditLog(db, {
      agentId: 'agent-survivor',
      sessionId: 'sess-ephemeral',
      filePath: '/surviving.ts',
      operation: 'create',
    });

    db.prepare('DELETE FROM sessions WHERE session_id = ?').run('sess-ephemeral');

    const rows = queryEditLog(db, { agentId: 'agent-survivor' });
    expect(rows).toHaveLength(1);
    expect(rows[0]!.edit_id).toBe(result.editId);
    expect(rows[0]!.session_id).toBeNull();
  });
});

// ─── 9. edit_log.run_id → tasks CASCADE SET NULL ────────────────────────────

describe('edit_log FK — run_id becomes NULL when task is deleted', () => {
  it('sets run_id to NULL when the referenced task is deleted', () => {
    const db = freshDb();
    seedTask(db, 'task-to-delete');

    const result = insertEditLog(db, {
      agentId: 'agent-x',
      runId: 'task-to-delete',
      filePath: '/workspace/component.ts',
      operation: 'update',
    });

    // Verify FK is set before deletion
    const before = db.prepare('SELECT run_id FROM edit_log WHERE edit_id = ?').get(result.editId) as { run_id: string | null };
    expect(before.run_id).toBe('task-to-delete');

    // Delete the task
    db.prepare('DELETE FROM task_runs WHERE run_id = ?').run('task-to-delete');

    // The edit_log row must still exist with run_id = NULL
    const after = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as EditLogRow | undefined;
    expect(after).toBeDefined();
    expect(after!.run_id).toBeNull();
    expect(after!.file_path).toBe('/workspace/component.ts');
    expect(after!.agent_id).toBe('agent-x');
  });

  it('edit_log row survives task deletion and remains queryable by file_path', () => {
    const db = freshDb();
    seedTask(db, 'task-ephemeral');

    const result = insertEditLog(db, {
      agentId: 'agent-y',
      runId: 'task-ephemeral',
      filePath: '/module/api.ts',
      operation: 'create',
    });

    db.prepare('DELETE FROM task_runs WHERE run_id = ?').run('task-ephemeral');

    const rows = queryEditLog(db, { filePath: '/module/api.ts' });
    expect(rows).toHaveLength(1);
    expect(rows[0]!.edit_id).toBe(result.editId);
    expect(rows[0]!.run_id).toBeNull();
  });
});

// ─── 10. Multiple edits to same file tracked separately ──────────────────────

describe('edit_log — multiple edits to the same file are tracked separately', () => {
  it('creates distinct rows for repeated edits to the same file_path', () => {
    const db = freshDb();
    const FILE = '/workspace/src/core.ts';

    const r1 = insertEditLog(db, { agentId: 'agent-a', filePath: FILE, operation: 'create' });
    const r2 = insertEditLog(db, { agentId: 'agent-a', filePath: FILE, operation: 'update', linesAdded: 5 });
    const r3 = insertEditLog(db, { agentId: 'agent-b', filePath: FILE, operation: 'update', linesAdded: 2, linesRemoved: 1 });

    // All three must have distinct edit_ids
    expect(r1.editId).not.toBe(r2.editId);
    expect(r2.editId).not.toBe(r3.editId);
    expect(r1.editId).not.toBe(r3.editId);

    // query returns all three
    const rows = queryEditLog(db, { filePath: FILE });
    expect(rows).toHaveLength(3);

    const editIds = rows.map(r => r.edit_id);
    expect(editIds).toContain(r1.editId);
    expect(editIds).toContain(r2.editId);
    expect(editIds).toContain(r3.editId);
  });

  it('different agents editing the same file are tracked as separate rows', () => {
    const db = freshDb();
    const FILE = '/shared/config.ts';

    insertEditLog(db, { agentId: 'agent-1', filePath: FILE, operation: 'update' });
    insertEditLog(db, { agentId: 'agent-2', filePath: FILE, operation: 'update' });
    insertEditLog(db, { agentId: 'agent-3', filePath: FILE, operation: 'update' });

    const rows = queryEditLog(db, { filePath: FILE });
    expect(rows).toHaveLength(3);

    const agentIds = new Set(rows.map(r => r.agent_id));
    expect(agentIds.size).toBe(3);
  });

  it('all operations are tracked independently on the same file', () => {
    const db = freshDb();
    const FILE = '/lifecycle/tracked.ts';
    const OPERATIONS = ['create', 'update', 'update', 'delete'] as const;

    for (const operation of OPERATIONS) {
      insertEditLog(db, { agentId: 'agent-lifecycle', filePath: FILE, operation });
    }

    const rows = queryEditLog(db, { filePath: FILE });
    expect(rows).toHaveLength(OPERATIONS.length);

    const ops = rows.map(r => r.operation);
    expect(ops).toContain('create');
    expect(ops).toContain('update');
    expect(ops).toContain('delete');
    expect(ops.filter(o => o === 'update')).toHaveLength(2);
  });
});
