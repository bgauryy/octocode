/**
 * edit-log.test.ts — Tests for the edit_log table via insertEditLog / queryEditLog.
 *
 * All tests use an in-memory SQLite database (fresh per test) so there is no
 * shared state between cases.
 */

import { describe, it, expect, beforeEach } from 'vitest';
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
function seedTask(db: DatabaseSync, taskId: string, agentId = 'agent-test'): void {
  db.prepare(`
    INSERT INTO tasks(task_id, agent_id, rationale, test_plan, status, created_at, updated_at)
    VALUES (?, ?, 'test rationale', 'test plan', 'ACTIVE', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
  `).run(taskId, agentId);
}

// ─── 1. insertEditLog creates a row with correct fields (required + optional) ──

describe('insertEditLog — creates a row with correct fields', () => {
  let db: DatabaseSync;
  beforeEach(() => { db = freshDb(); });

  it('stores all required fields correctly', () => {
    const result = insertEditLog(db, {
      agentId: 'agent-a',
      filePath: '/workspace/src/index.ts',
      operation: 'create',
    });

    expect(result.editId).toBeTruthy();

    const row = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as unknown as EditLogRow;
    expect(row).toBeDefined();
    expect(row.agent_id).toBe('agent-a');
    expect(row.file_path).toBe('/workspace/src/index.ts');
    expect(row.operation).toBe('create');
    expect(row.created_at).toBeTruthy();
  });

  it('stores all optional fields when provided', () => {
    seedSession(db, 'sess-1');
    seedTask(db, 'task-1');

    const result = insertEditLog(db, {
      agentId: 'agent-a',
      sessionId: 'sess-1',
      taskId: 'task-1',
      filePath: '/workspace/src/db.ts',
      operation: 'update',
      linesAdded: 42,
      linesRemoved: 10,
      contentHash: 'abc123def456',
      workspacePath: '/workspace',
    });

    const row = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as unknown as EditLogRow;
    expect(row.session_id).toBe('sess-1');
    expect(row.task_id).toBe('task-1');
    expect(row.lines_added).toBe(42);
    expect(row.lines_removed).toBe(10);
    expect(row.content_hash).toBe('abc123def456');
    expect(row.workspace_path).toBe('/workspace');
    expect(row.old_file_path).toBeNull();
  });

  it('stores null for absent optional fields', () => {
    const result = insertEditLog(db, {
      agentId: 'agent-b',
      filePath: '/workspace/foo.ts',
      operation: 'delete',
    });

    const row = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as unknown as EditLogRow;
    expect(row.session_id).toBeNull();
    expect(row.task_id).toBeNull();
    expect(row.old_file_path).toBeNull();
    expect(row.lines_added).toBeNull();
    expect(row.lines_removed).toBeNull();
    expect(row.content_hash).toBeNull();
    expect(row.workspace_path).toBeNull();
  });
});

// ─── 2. insertEditLog with operation='move' stores old_file_path ──────────────

describe('insertEditLog — operation move stores old_file_path', () => {
  it('stores old_file_path for move operation', () => {
    const db = freshDb();

    const result = insertEditLog(db, {
      agentId: 'agent-a',
      filePath: '/workspace/src/utils/helpers.ts',
      operation: 'move',
      oldFilePath: '/workspace/src/helpers.ts',
    });

    const row = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as unknown as EditLogRow;
    expect(row.operation).toBe('move');
    expect(row.old_file_path).toBe('/workspace/src/helpers.ts');
    expect(row.file_path).toBe('/workspace/src/utils/helpers.ts');
  });

  it('stores old_file_path for rename operation', () => {
    const db = freshDb();

    const result = insertEditLog(db, {
      agentId: 'agent-a',
      filePath: '/workspace/src/new-name.ts',
      operation: 'rename',
      oldFilePath: '/workspace/src/old-name.ts',
    });

    const row = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as unknown as EditLogRow;
    expect(row.operation).toBe('rename');
    expect(row.old_file_path).toBe('/workspace/src/old-name.ts');
  });
});

// ─── 3. queryEditLog by session_id ────────────────────────────────────────────

describe('queryEditLog — filter by session_id', () => {
  it('returns only rows matching the given session_id', () => {
    const db = freshDb();
    seedSession(db, 'sess-A');
    seedSession(db, 'sess-B');

    insertEditLog(db, { agentId: 'agent-x', sessionId: 'sess-A', filePath: '/a.ts', operation: 'create' });
    insertEditLog(db, { agentId: 'agent-x', sessionId: 'sess-A', filePath: '/b.ts', operation: 'update' });
    insertEditLog(db, { agentId: 'agent-x', sessionId: 'sess-B', filePath: '/c.ts', operation: 'delete' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/d.ts', operation: 'create' });

    const rows = queryEditLog(db, { sessionId: 'sess-A' });
    expect(rows).toHaveLength(2);
    expect(rows.every(r => r.session_id === 'sess-A')).toBe(true);
    const filePaths = rows.map(r => r.file_path);
    expect(filePaths).toContain('/a.ts');
    expect(filePaths).toContain('/b.ts');
  });

  it('returns empty array when no rows match the session_id', () => {
    const db = freshDb();
    insertEditLog(db, { agentId: 'agent-x', filePath: '/a.ts', operation: 'create' });

    const rows = queryEditLog(db, { sessionId: 'nonexistent-session' });
    expect(rows).toHaveLength(0);
  });
});

// ─── 4. queryEditLog by file_path ─────────────────────────────────────────────

describe('queryEditLog — filter by file_path', () => {
  it('returns only rows matching the given file_path', () => {
    const db = freshDb();

    insertEditLog(db, { agentId: 'agent-x', filePath: '/repo/src/index.ts', operation: 'create' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/repo/src/index.ts', operation: 'update' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/repo/src/other.ts', operation: 'update' });

    const rows = queryEditLog(db, { filePath: '/repo/src/index.ts' });
    expect(rows).toHaveLength(2);
    expect(rows.every(r => r.file_path === '/repo/src/index.ts')).toBe(true);
  });

  it('returns empty array when file_path does not exist in log', () => {
    const db = freshDb();
    insertEditLog(db, { agentId: 'agent-x', filePath: '/repo/src/foo.ts', operation: 'create' });

    const rows = queryEditLog(db, { filePath: '/repo/src/bar.ts' });
    expect(rows).toHaveLength(0);
  });
});

// ─── 5. queryEditLog by agent_id ──────────────────────────────────────────────

describe('queryEditLog — filter by agent_id', () => {
  it('returns only rows for the given agent_id', () => {
    const db = freshDb();

    insertEditLog(db, { agentId: 'agent-alpha', filePath: '/a.ts', operation: 'create' });
    insertEditLog(db, { agentId: 'agent-alpha', filePath: '/b.ts', operation: 'update' });
    insertEditLog(db, { agentId: 'agent-beta', filePath: '/c.ts', operation: 'create' });

    const rows = queryEditLog(db, { agentId: 'agent-alpha' });
    expect(rows).toHaveLength(2);
    expect(rows.every(r => r.agent_id === 'agent-alpha')).toBe(true);
  });

  it('returns empty array when no rows belong to the given agent_id', () => {
    const db = freshDb();
    insertEditLog(db, { agentId: 'agent-alpha', filePath: '/a.ts', operation: 'create' });

    const rows = queryEditLog(db, { agentId: 'agent-gamma' });
    expect(rows).toHaveLength(0);
  });
});

// ─── 6. queryEditLog by workspacePath ────────────────────────────────────────

describe('queryEditLog — filter by workspacePath', () => {
  it('returns only rows matching the given workspacePath', () => {
    const db = freshDb();

    insertEditLog(db, { agentId: 'agent-x', filePath: '/ws-a/file1.ts', operation: 'create', workspacePath: '/ws-a' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/ws-a/file2.ts', operation: 'update', workspacePath: '/ws-a' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/ws-b/file3.ts', operation: 'create', workspacePath: '/ws-b' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/no-ws.ts', operation: 'create' });

    const rows = queryEditLog(db, { workspacePath: '/ws-a' });
    expect(rows).toHaveLength(2);
    expect(rows.every(r => r.workspace_path === '/ws-a')).toBe(true);
  });

  it('returns empty array when no rows match the workspacePath', () => {
    const db = freshDb();
    insertEditLog(db, { agentId: 'agent-x', filePath: '/ws-a/file.ts', operation: 'create', workspacePath: '/ws-a' });

    const rows = queryEditLog(db, { workspacePath: '/ws-nonexistent' });
    expect(rows).toHaveLength(0);
  });
});

// ─── 7. queryEditLog with since= filters by created_at ───────────────────────

describe('queryEditLog — filter by since (created_at)', () => {
  it('returns only rows with created_at >= since', () => {
    const db = freshDb();

    // Insert rows with explicit timestamps by bypassing the DEFAULT
    db.prepare(`
      INSERT INTO edit_log(edit_id, agent_id, file_path, operation, created_at)
      VALUES ('edit-old-1', 'agent-x', '/old1.ts', 'create', '2026-01-01T00:00:00Z')
    `).run();
    db.prepare(`
      INSERT INTO edit_log(edit_id, agent_id, file_path, operation, created_at)
      VALUES ('edit-old-2', 'agent-x', '/old2.ts', 'update', '2026-03-01T00:00:00Z')
    `).run();
    db.prepare(`
      INSERT INTO edit_log(edit_id, agent_id, file_path, operation, created_at)
      VALUES ('edit-new-1', 'agent-x', '/new1.ts', 'create', '2026-06-01T00:00:00Z')
    `).run();
    db.prepare(`
      INSERT INTO edit_log(edit_id, agent_id, file_path, operation, created_at)
      VALUES ('edit-new-2', 'agent-x', '/new2.ts', 'delete', '2026-07-01T00:00:00Z')
    `).run();

    const rows = queryEditLog(db, { since: '2026-06-01T00:00:00Z' });
    expect(rows).toHaveLength(2);
    const ids = rows.map(r => r.edit_id);
    expect(ids).toContain('edit-new-1');
    expect(ids).toContain('edit-new-2');
    expect(ids).not.toContain('edit-old-1');
    expect(ids).not.toContain('edit-old-2');
  });

  it('returns all rows when since is far in the past', () => {
    const db = freshDb();

    insertEditLog(db, { agentId: 'agent-x', filePath: '/a.ts', operation: 'create' });
    insertEditLog(db, { agentId: 'agent-x', filePath: '/b.ts', operation: 'update' });

    const rows = queryEditLog(db, { since: '2000-01-01T00:00:00Z' });
    expect(rows).toHaveLength(2);
  });

  it('returns empty array when since is in the future', () => {
    const db = freshDb();
    insertEditLog(db, { agentId: 'agent-x', filePath: '/a.ts', operation: 'create' });

    const rows = queryEditLog(db, { since: '2099-01-01T00:00:00Z' });
    expect(rows).toHaveLength(0);
  });
});

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

// ─── 9. edit_log.task_id → tasks CASCADE SET NULL ────────────────────────────

describe('edit_log FK — task_id becomes NULL when task is deleted', () => {
  it('sets task_id to NULL when the referenced task is deleted', () => {
    const db = freshDb();
    seedTask(db, 'task-to-delete');

    const result = insertEditLog(db, {
      agentId: 'agent-x',
      taskId: 'task-to-delete',
      filePath: '/workspace/component.ts',
      operation: 'update',
    });

    // Verify FK is set before deletion
    const before = db.prepare('SELECT task_id FROM edit_log WHERE edit_id = ?').get(result.editId) as { task_id: string | null };
    expect(before.task_id).toBe('task-to-delete');

    // Delete the task
    db.prepare('DELETE FROM tasks WHERE task_id = ?').run('task-to-delete');

    // The edit_log row must still exist with task_id = NULL
    const after = db.prepare('SELECT * FROM edit_log WHERE edit_id = ?').get(result.editId) as EditLogRow | undefined;
    expect(after).toBeDefined();
    expect(after!.task_id).toBeNull();
    expect(after!.file_path).toBe('/workspace/component.ts');
    expect(after!.agent_id).toBe('agent-x');
  });

  it('edit_log row survives task deletion and remains queryable by file_path', () => {
    const db = freshDb();
    seedTask(db, 'task-ephemeral');

    const result = insertEditLog(db, {
      agentId: 'agent-y',
      taskId: 'task-ephemeral',
      filePath: '/module/api.ts',
      operation: 'create',
    });

    db.prepare('DELETE FROM tasks WHERE task_id = ?').run('task-ephemeral');

    const rows = queryEditLog(db, { filePath: '/module/api.ts' });
    expect(rows).toHaveLength(1);
    expect(rows[0]!.edit_id).toBe(result.editId);
    expect(rows[0]!.task_id).toBeNull();
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
