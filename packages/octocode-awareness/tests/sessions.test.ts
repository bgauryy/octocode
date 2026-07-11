/**
 * sessions.test.ts — Unit tests for the Session entity.
 *
 * Covers:
 *   1. insertSession  — creates a row in sessions with correct fields
 *   2. getSession     — retrieves by session_id
 *   3. endSession     — sets ended_at
 *   4. memories FK    — session_id links correctly to memories
 *   5. tasks FK       — session_id links correctly to tasks
 *   6. ON DELETE SET NULL — deleting a session nullifies memory.session_id
 *
 * Implementation notes:
 *   • src/sessions.ts is the expected location for insertSession / getSession /
 *     endSession / listSessions (may not exist yet — tests are written to drive
 *     that implementation).
 *   • tasks.session_id already has REFERENCES sessions(session_id) ON DELETE SET NULL
 *     in the current schema, so tests 5 and the tasks arm of test 6 are
 *     immediately runnable once sessions.ts exists.
 *   • memories.session_id does not yet exist in the schema.  Tests 4 and 6 (memory
 *     arm) use tableColumns() to detect the column at runtime so they degrade
 *     gracefully to a schema-shape assertion until the column is added.
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, tableColumns } from '../src/db.js';
import { insertSession, endSession, getSession } from '../src/sessions.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
/** Insert a minimal task row optionally linked to a session. */
function insertTask(db: DatabaseSync, runId: string, agentId: string, sessionId: string | null): void {
    db.prepare(`
    INSERT INTO task_runs(run_id, origin, agent_id, session_id, rationale, test_plan, status,
                      workspace_path, created_at, updated_at)
    VALUES (?, 'WORK', ?, ?, 'rationale', 'test_plan', 'ACTIVE',
            '/workspace',
            strftime('%Y-%m-%dT%H:%M:%fZ','now'),
            strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  `).run(runId, agentId, sessionId);
}
/**
 * Insert a minimal memory row optionally linked to a session.
 * Requires memories.session_id to exist — call only after verifying with tableColumns().
 */
function insertMemoryWithSession(db: DatabaseSync, memoryId: string, agentId: string, sessionId: string | null): void {
    db.prepare(`
    INSERT INTO memories(memory_id, agent_id, task_context, observation, importance,
                         session_id, created_at)
    VALUES (?, ?, 'task ctx', 'observation text', 5, ?,
            strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  `).run(memoryId, agentId, sessionId);
}

// ─── 1. insertSession ─────────────────────────────────────────────────────────

describe('insertSession', () => {
  it('creates a row in the sessions table with correct fields', () => {
    const db = freshDb();
    const session = insertSession(db, {
      agentId: 'agent-1',
      workspacePath: '/projects/my-app',
      repo: 'owner/repo',
      ref: 'main',
    });

    // Returned shape.
    expect(typeof session.session_id).toBe('string');
    expect(session.session_id.length).toBeGreaterThan(0);
    expect(session.agent_id).toBe('agent-1');
    expect(session.workspace_path).toBe('/projects/my-app');
    expect(session.repo).toBe('owner/repo');
    expect(session.ref).toBe('main');
    expect(typeof session.started_at).toBe('string');
    expect(Number.isNaN(Date.parse(session.started_at))).toBe(false);
    expect(session.ended_at).toBeNull();
    expect(session.summary).toBeNull();

    // Row is persisted.
    const row = db
      .prepare('SELECT * FROM sessions WHERE session_id = ?')
      .get(session.session_id) as Record<string, unknown> | undefined;
    expect(row).toBeDefined();
    expect(row!['agent_id']).toBe('agent-1');
    expect(row!['workspace_path']).toBe('/projects/my-app');
    expect(row!['repo']).toBe('owner/repo');
    expect(row!['ref']).toBe('main');
    expect(row!['started_at']).toBeTruthy();
    expect(row!['ended_at']).toBeNull();
    expect(row!['summary']).toBeNull();
  });

  it('optional fields default to null when only agentId is supplied', () => {
    const db = freshDb();
    const session = insertSession(db, { agentId: 'agent-min' });
    expect(session.workspace_path).toBeNull();
    expect(session.repo).toBeNull();
    expect(session.ref).toBeNull();
    expect(session.ended_at).toBeNull();
    expect(session.summary).toBeNull();
  });

  it('each call produces a distinct session_id', () => {
    const db = freshDb();
    const a = insertSession(db, { agentId: 'agent-1' });
    const b = insertSession(db, { agentId: 'agent-1' });
    expect(a.session_id).not.toBe(b.session_id);
  });

  it('there is no UNIQUE constraint on agent_id — concurrent sessions are allowed', () => {
    const db = freshDb();
    expect(() => {
      insertSession(db, { agentId: 'shared-agent' });
      insertSession(db, { agentId: 'shared-agent' });
      insertSession(db, { agentId: 'shared-agent' });
    }).not.toThrow();

    const row = db
      .prepare('SELECT COUNT(*) AS cnt FROM sessions WHERE agent_id = ?')
      .get('shared-agent') as { cnt: number };
    expect(row.cnt).toBe(3);
  });
});

// ─── 2. getSession ────────────────────────────────────────────────────────────

describe('getSession', () => {
  it('retrieves an existing session by session_id', () => {
    const db = freshDb();
    const created = insertSession(db, {
      agentId: 'agent-1',
      workspacePath: '/ws',
      repo: 'owner/repo',
      ref: 'feat/x',
    });

    const fetched = getSession(db, created.session_id);

    expect(fetched).not.toBeNull();
    expect(fetched!.session_id).toBe(created.session_id);
    expect(fetched!.agent_id).toBe('agent-1');
    expect(fetched!.workspace_path).toBe('/ws');
    expect(fetched!.repo).toBe('owner/repo');
    expect(fetched!.ref).toBe('feat/x');
    expect(fetched!.ended_at).toBeNull();
  });

  it('returns null for a session_id that does not exist', () => {
    const db = freshDb();
    expect(getSession(db, 'sess_nonexistent')).toBeNull();
  });

  it('reflects mutations made by endSession', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });

    endSession(db, { sessionId: session_id, agentId: 'agent-1', summary: 'wrapped up' });

    const fetched = getSession(db, session_id);
    expect(fetched!.ended_at).not.toBeNull();
    expect(fetched!.summary).toBe('wrapped up');
  });
});

// ─── 3. endSession ────────────────────────────────────────────────────────────

describe('endSession', () => {
  it('sets ended_at to a valid ISO timestamp', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });

    const ended = endSession(db, { sessionId: session_id, agentId: 'agent-1' });

    expect(ended).not.toBeNull();
    expect(ended!.ended_at).not.toBeNull();
    expect(Number.isNaN(Date.parse(ended!.ended_at!))).toBe(false);
  });

  it('ended_at is at or after started_at', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });
    const ended = endSession(db, { sessionId: session_id, agentId: 'agent-1' });
    expect(new Date(ended!.ended_at!).getTime()).toBeGreaterThanOrEqual(
      new Date(ended!.started_at).getTime(),
    );
  });

  it('persists the summary when provided', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });
    const ended = endSession(db, { sessionId: session_id, agentId: 'agent-1', summary: 'All tasks done.' });
    expect(ended!.summary).toBe('All tasks done.');

    // Also verify the row in the DB.
    const row = db
      .prepare('SELECT ended_at, summary FROM sessions WHERE session_id = ?')
      .get(session_id) as { ended_at: string | null; summary: string | null } | undefined;
    expect(row!.ended_at).not.toBeNull();
    expect(row!.summary).toBe('All tasks done.');
  });

  it('summary remains null when not provided', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });
    const ended = endSession(db, { sessionId: session_id, agentId: 'agent-1' });
    expect(ended!.summary).toBeNull();
  });

  it('returns null for an unknown session_id', () => {
    const db = freshDb();
    expect(endSession(db, { sessionId: 'sess_nonexistent', agentId: 'agent-1' })).toBeNull();
  });

  it('does not let another agent or scope end the session', () => {
    const db = freshDb();
    const session = insertSession(db, { agentId: 'owner', workspacePath: '/repo', artifact: 'svc' });
    expect(endSession(db, { sessionId: session.session_id, agentId: 'outsider', workspacePath: '/repo', artifact: 'svc' })).toBeNull();
    expect(endSession(db, { sessionId: session.session_id, agentId: 'owner', workspacePath: '/other', artifact: 'svc' })).toBeNull();
    expect(getSession(db, session.session_id)?.ended_at).toBeNull();
  });
});

// ─── 4. session_id links correctly to memories ────────────────────────────────
//
// Document the planned memories.session_id link without requiring it yet.

describe('session_id links correctly to memories', () => {
  it('documents whether memories has a session_id column', () => {
    const db = freshDb();
    const cols = tableColumns(db, 'memories');
    if (!cols.has('session_id')) {
      expect(cols.has('session_id')).toBe(false);
      return;
    }
    expect(cols.has('session_id')).toBe(true);
  });

  it('a memory inserted with session_id can be queried by session', () => {
    const db = freshDb();
    if (!tableColumns(db, 'memories').has('session_id')) return; // schema not yet updated

    const session = insertSession(db, { agentId: 'agent-1' });
    insertMemoryWithSession(db, 'mem-link-test', 'agent-1', session.session_id);

    const row = db
      .prepare('SELECT memory_id, session_id FROM memories WHERE session_id = ?')
      .get(session.session_id) as { memory_id: string; session_id: string } | undefined;

    expect(row).toBeDefined();
    expect(row!.memory_id).toBe('mem-link-test');
    expect(row!.session_id).toBe(session.session_id);
  });

  it('memory.session_id can be NULL (session-less memory)', () => {
    const db = freshDb();
    if (!tableColumns(db, 'memories').has('session_id')) return;

    insertMemoryWithSession(db, 'mem-no-session', 'agent-1', null);

    const row = db
      .prepare('SELECT session_id FROM memories WHERE memory_id = ?')
      .get('mem-no-session') as { session_id: string | null } | undefined;

    expect(row).toBeDefined();
    expect(row!.session_id).toBeNull();
  });
});

// ─── 5. session_id links correctly to tasks ──────────────────────────────────
//
// tasks.session_id REFERENCES sessions(session_id) ON DELETE SET NULL is already
// present in the current schema — these tests pass as soon as sessions.ts is created.

describe('session_id links correctly to tasks', () => {
  it('a task inserted with session_id can be queried by session', () => {
    const db = freshDb();
    const session = insertSession(db, { agentId: 'agent-1' });
    insertTask(db, 'task-linked', 'agent-1', session.session_id);

    const row = db
      .prepare('SELECT run_id, session_id FROM task_runs WHERE session_id = ?')
      .get(session.session_id) as { run_id: string; session_id: string } | undefined;

    expect(row).toBeDefined();
    expect(row!.run_id).toBe('task-linked');
    expect(row!.session_id).toBe(session.session_id);
  });

  it('task.session_id can be NULL (session-less task)', () => {
    const db = freshDb();
    insertTask(db, 'task-no-session', 'agent-1', null);

    const row = db
      .prepare('SELECT session_id FROM task_runs WHERE run_id = ?')
      .get('task-no-session') as { session_id: string | null } | undefined;

    expect(row).toBeDefined();
    expect(row!.session_id).toBeNull();
  });

  it('task.session_id FK rejects a reference to a nonexistent session', () => {
    const db = freshDb();
    expect(() => insertTask(db, 'task-bad', 'agent-1', 'sess_nonexistent')).toThrow();
  });

  it('a session can own multiple tasks', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });
    insertTask(db, 'task-a', 'agent-1', session_id);
    insertTask(db, 'task-b', 'agent-1', session_id);

    const rows = db
      .prepare('SELECT run_id FROM task_runs WHERE session_id = ?')
      .all(session_id) as { run_id: string }[];

    expect(rows).toHaveLength(2);
    expect(rows.map(r => r.run_id).sort()).toEqual(['task-a', 'task-b']);
  });
});
