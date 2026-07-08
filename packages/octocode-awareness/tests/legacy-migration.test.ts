import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, tableColumns } from '../src/db.js';
import { insertMemory, getMemory } from '../src/memory.js';

/**
 * Upgrade-path regression tests. This bug class has shipped twice:
 * index-created-before-migration, then migration that only backfilled
 * `artifact` so any newer column (failure_signature, valid_from, …) broke
 * every command on a pre-existing store — silently, because hooks fail open.
 * These tests open stores frozen at older schema generations and assert
 * initDb brings them fully up to the canonical schema.
 */

/** Store generation ~pre-bitemporal/pre-semantic: memories lacks state/label/
 *  failure_signature/valid_from/valid_to/embedding/etc; other tables lack
 *  artifact and session_id columns. */
function legacyDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec(`
    CREATE TABLE sessions (
      session_id TEXT PRIMARY KEY,
      agent_id   TEXT NOT NULL,
      workspace_path TEXT,
      started_at TEXT NOT NULL,
      ended_at   TEXT,
      summary    TEXT
    );
    CREATE TABLE memories (
      memory_id    TEXT PRIMARY KEY,
      agent_id     TEXT NOT NULL,
      task_context TEXT NOT NULL,
      observation  TEXT NOT NULL,
      importance   INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
      tags_json    TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE tasks (
      task_id    TEXT PRIMARY KEY,
      agent_id   TEXT NOT NULL,
      rationale  TEXT NOT NULL,
      test_plan  TEXT NOT NULL,
      status     TEXT NOT NULL DEFAULT 'ACTIVE',
      workspace_path TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE locks (
      lock_id     TEXT PRIMARY KEY,
      file_path   TEXT NOT NULL,
      task_id     TEXT NOT NULL,
      agent_id    TEXT NOT NULL,
      lock_type   TEXT NOT NULL,
      acquired_at TEXT NOT NULL,
      expires_at  TEXT,
      UNIQUE(file_path, task_id)
    );
    INSERT INTO memories (memory_id, agent_id, task_context, observation, importance)
      VALUES ('mem_legacy_1', 'agent-old', 'legacy work', 'a lesson recorded before the schema grew', 6);
  `);
  return db;
}

describe('legacy store migration', () => {
  it('initDb succeeds on a pre-bitemporal store (indexes reference migrated columns)', () => {
    const db = legacyDb();
    // Before the fix this threw: "no such column: failure_signature"
    expect(() => initDb(db)).not.toThrow();
  });

  it('backfills every canonical column on every pre-existing table', () => {
    const db = legacyDb();
    initDb(db);
    const memories = tableColumns(db, 'memories');
    for (const col of [
      'state', 'label', 'superseded_by', 'artifact', 'repo', 'ref',
      'file_tree_fingerprint', 'novelty_score', 'last_accessed_at',
      'access_count', 'decay_half_life_days', 'failure_signature',
      'valid_from', 'valid_to', 'expired_at', 'embedding', 'embedding_model',
      'updated_at',
    ]) {
      expect(memories.has(col), `memories.${col}`).toBe(true);
    }
    expect(tableColumns(db, 'tasks').has('session_id')).toBe(true);
    expect(tableColumns(db, 'tasks').has('artifact')).toBe(true);
    expect(tableColumns(db, 'tasks').has('plan_doc_ref')).toBe(true);
    expect(tableColumns(db, 'locks').has('session_id')).toBe(true);
    expect(tableColumns(db, 'sessions').has('artifact')).toBe(true);
    expect(tableColumns(db, 'sessions').has('repo')).toBe(true);
  });

  it('constant defaults apply to pre-existing rows (NOT NULL columns stay usable)', () => {
    const db = legacyDb();
    initDb(db);
    const row = db.prepare(
      'SELECT state, label, access_count FROM memories WHERE memory_id = ?'
    ).get('mem_legacy_1') as { state: string; label: string; access_count: number };
    expect(row.state).toBe('ACTIVE');
    expect(row.label).toBe('OTHER');
    expect(row.access_count).toBe(0);
  });

  it('migrated store handles the full read/write loop', () => {
    const db = legacyDb();
    initDb(db);
    insertMemory(db, {
      taskContext: 'post-upgrade work',
      observation: 'new memory written after migration',
      importance: 7,
      label: 'GOTCHA',
    });
    const { memories } = getMemory(db, { query: 'post-upgrade work migration', limit: 5 });
    expect(memories.length).toBeGreaterThan(0);
    // The pre-existing row is also readable through the current query path.
    const all = db.prepare('SELECT COUNT(*) AS cnt FROM memories').get() as { cnt: number };
    expect(all.cnt).toBe(2);
  });

  it('initDb is idempotent on an already-migrated store', () => {
    const db = legacyDb();
    initDb(db);
    expect(() => initDb(db)).not.toThrow();
  });
});
