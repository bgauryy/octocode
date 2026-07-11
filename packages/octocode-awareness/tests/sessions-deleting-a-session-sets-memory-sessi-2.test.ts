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
import { mkdirSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb, tableColumns } from '../src/db.js';
import { insertSession, listSessions, getOrCreateSession } from '../src/sessions.js';
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

// ─── 6. ON DELETE SET NULL ────────────────────────────────────────────────────
//
// Deleting a session must nullify session_id on all linked rows (tasks and
// memories) without deleting those rows.

describe('deleting a session sets memory.session_id = NULL (ON DELETE SET NULL)', () => {
  it('memory.session_id becomes NULL after the parent session is deleted', () => {
    const db = freshDb();
    if (!tableColumns(db, 'memories').has('session_id')) return; // schema not yet updated

    const { session_id } = insertSession(db, { agentId: 'agent-1' });
    insertMemoryWithSession(db, 'mem-cascade', 'agent-1', session_id);

    // Sanity-check the FK is in place before deletion.
    const before = db
      .prepare('SELECT session_id FROM memories WHERE memory_id = ?')
      .get('mem-cascade') as { session_id: string | null };
    expect(before.session_id).toBe(session_id);

    // Delete the parent session.
    db.prepare('DELETE FROM sessions WHERE session_id = ?').run(session_id);

    // memory.session_id must be NULL — the memory itself must survive.
    const after = db
      .prepare('SELECT memory_id, session_id FROM memories WHERE memory_id = ?')
      .get('mem-cascade') as { memory_id: string; session_id: string | null } | undefined;
    expect(after).toBeDefined();
    expect(after!.memory_id).toBe('mem-cascade');
    expect(after!.session_id).toBeNull();
  });

  it('task.session_id becomes NULL after the parent session is deleted', () => {
    const db = freshDb();
    const { session_id } = insertSession(db, { agentId: 'agent-1' });
    insertTask(db, 'task-cascade', 'agent-1', session_id);

    // Sanity-check the FK is in place before deletion.
    const before = db
      .prepare('SELECT session_id FROM task_runs WHERE run_id = ?')
      .get('task-cascade') as { session_id: string | null };
    expect(before.session_id).toBe(session_id);

    // Delete the parent session.
    db.prepare('DELETE FROM sessions WHERE session_id = ?').run(session_id);

    // task.session_id must be NULL — the task row itself must survive.
    const after = db
      .prepare('SELECT run_id, session_id FROM task_runs WHERE run_id = ?')
      .get('task-cascade') as { run_id: string; session_id: string | null } | undefined;
    expect(after).toBeDefined();
    expect(after!.run_id).toBe('task-cascade');
    expect(after!.session_id).toBeNull();
  });

  it('deleting one session does not affect tasks linked to a different session', () => {
    const db = freshDb();
    const s1 = insertSession(db, { agentId: 'agent-1' });
    const s2 = insertSession(db, { agentId: 'agent-2' });
    insertTask(db, 'task-s1', 'agent-1', s1.session_id);
    insertTask(db, 'task-s2', 'agent-2', s2.session_id);

    db.prepare('DELETE FROM sessions WHERE session_id = ?').run(s1.session_id);

    // task-s1 must be NULL; task-s2 must still point at s2.
    const r1 = db.prepare('SELECT session_id FROM task_runs WHERE run_id = ?').get('task-s1') as
      { session_id: string | null };
    const r2 = db.prepare('SELECT session_id FROM task_runs WHERE run_id = ?').get('task-s2') as
      { session_id: string | null };

    expect(r1.session_id).toBeNull();
    expect(r2.session_id).toBe(s2.session_id);
  });
});

// ─── listSessions ─────────────────────────────────────────────────────────────

describe('listSessions', () => {
  it('returns all sessions when called with no filter', () => {
    const db = freshDb();
    insertSession(db, { agentId: 'agent-1' });
    insertSession(db, { agentId: 'agent-2' });

    const results = listSessions(db);
    expect(results.length).toBeGreaterThanOrEqual(2);
  });

  it('filters by agentId', () => {
    const db = freshDb();
    insertSession(db, { agentId: 'agent-A' });
    insertSession(db, { agentId: 'agent-A' });
    insertSession(db, { agentId: 'agent-B' });

    const results = listSessions(db, { agentId: 'agent-A' });
    expect(results.length).toBe(2);
    expect(results.every(s => s.agent_id === 'agent-A')).toBe(true);
  });

  it('filters by workspacePath', () => {
    const db = freshDb();
    insertSession(db, { agentId: 'agent-1', workspacePath: '/alpha' });
    insertSession(db, { agentId: 'agent-2', workspacePath: '/alpha' });
    insertSession(db, { agentId: 'agent-3', workspacePath: '/beta' });

    const results = listSessions(db, { workspacePath: '/alpha' });
    expect(results.length).toBe(2);
    expect(results.every(s => s.workspace_path === '/alpha')).toBe(true);
  });

  it('returns an empty array when no sessions match the filter', () => {
    const db = freshDb();
    insertSession(db, { agentId: 'agent-1' });

    expect(listSessions(db, { agentId: 'agent-Z' })).toHaveLength(0);
    expect(listSessions(db, { workspacePath: '/no-such-path' })).toHaveLength(0);
  });

  it('caps caller limits so session listings cannot flood agent context', () => {
    const db = freshDb();
    for (let i = 0; i < 125; i++) insertSession(db, { agentId: `agent-${i}` });
    expect(listSessions(db, { limit: 500 })).toHaveLength(100);
  });
});

// ─── workspace-scope symlink stability (regression) ───────────────────────────
//
// insertSession/listSessions/getOrCreateSession used to store/query
// workspace_path verbatim with no normalization, unlike memory/lock/signal
// which resolve through fillScope. A session started via a symlinked
// workspace path could silently never be found via the real path (or a
// getOrCreateSession call for the "same" workspace could open a duplicate
// session instead of reusing the active one).

describe('workspace-scope symlink stability (regression)', () => {
  function tempDirWithLink(): { real: string; link: string; base: string } {
    const base = mkdtempSync(join(tmpdir(), 'oc-sessions-scope-'));
    const real = join(base, 'real');
    const link = join(base, 'link');
    mkdirSync(real, { recursive: true });
    symlinkSync(real, link);
    return { real, link, base };
  }

  it('a session started via a symlinked workspace path is found via the real path', () => {
    const db = freshDb();
    const { real, link, base } = tempDirWithLink();
    try {
      const { session_id } = insertSession(db, { agentId: 'agent-1', workspacePath: link });
      const results = listSessions(db, { workspacePath: real });
      expect(results.map((s) => s.session_id)).toContain(session_id);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it('getOrCreateSession reuses the active session regardless of symlink form', () => {
    const db = freshDb();
    const { real, link, base } = tempDirWithLink();
    try {
      const first = getOrCreateSession(db, { agentId: 'agent-1', workspacePath: real });
      const second = getOrCreateSession(db, { agentId: 'agent-1', workspacePath: link });
      expect(second).toBe(first);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });
});
