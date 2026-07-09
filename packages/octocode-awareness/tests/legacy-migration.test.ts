import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, tableColumns } from '../src/db.js';
import { insertMemory, getMemory } from '../src/memory.js';
import { insertRefinement } from '../src/refinements.js';
import { insertHarnessLog } from '../src/audit.js';

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
    expect(tableColumns(db, 'task_runs').has('session_id')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('artifact')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('context_ref')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('origin')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('files_json')).toBe(false);
    expect(tableColumns(db, 'tasks').has('plan_id')).toBe(true);
    expect(tableColumns(db, 'locks')).toEqual(new Set([
      'lock_id', 'file_path', 'run_id', 'acquired_at', 'expires_at',
    ]));
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

  it('moves legacy edit tasks to task_runs without inventing plan tasks', () => {
    const db = legacyDb();
    db.prepare(`INSERT INTO tasks
      (task_id, agent_id, rationale, test_plan, status, workspace_path, files_json)
      VALUES ('task_legacy', 'agent-old', 'edit a file', 'run tests', 'PENDING', '/repo', '["/repo/a.ts"]')`)
      .run();

    initDb(db);

    expect(db.prepare('SELECT run_id, task_id, origin, status FROM task_runs WHERE run_id = ?')
      .get('task_legacy')).toEqual({ run_id: 'task_legacy', task_id: null, origin: 'HOOK', status: 'PENDING' });
    expect(db.prepare('SELECT run_id, file_path, source FROM run_files WHERE run_id = ?')
      .get('task_legacy')).toEqual({ run_id: 'task_legacy', file_path: '/repo/a.ts', source: 'HOOK' });
    expect(db.prepare('SELECT COUNT(*) AS count FROM tasks').get()).toEqual({ count: 0 });
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: 3 });
  });

  it('migrates v2 run files and exclusive locks without duplicated identity columns', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA user_version = 2;
      CREATE TABLE task_runs (
        run_id TEXT PRIMARY KEY, task_id TEXT, agent_id TEXT NOT NULL, session_id TEXT,
        rationale TEXT NOT NULL, test_plan TEXT NOT NULL, context_ref TEXT,
        status TEXT NOT NULL, workspace_path TEXT, artifact TEXT,
        files_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL
      );
      CREATE TABLE locks (
        lock_id TEXT PRIMARY KEY, file_path TEXT NOT NULL, run_id TEXT NOT NULL,
        agent_id TEXT NOT NULL, session_id TEXT, lock_type TEXT NOT NULL,
        acquired_at TEXT NOT NULL, expires_at TEXT, UNIQUE(file_path, run_id)
      );
      INSERT INTO task_runs VALUES (
        'run_v2', NULL, 'agent-v2', 'session-v2', 'legacy edit', 'legacy test', NULL,
        'ACTIVE', '/repo', NULL, '["/repo/a.ts","/repo/b.ts"]',
        '2026-01-01T00:00:00Z', '2026-01-01T00:01:00Z'
      );
      INSERT INTO locks VALUES (
        'lock_v2', '/repo/a.ts', 'run_v2', 'agent-v2', 'session-v2', 'SHARED',
        '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z'
      );
    `);

    initDb(db);

    expect(db.prepare('SELECT origin FROM task_runs WHERE run_id = ?').get('run_v2'))
      .toEqual({ origin: 'HOOK' });
    expect(db.prepare('SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path').all('run_v2'))
      .toEqual([{ file_path: '/repo/a.ts' }, { file_path: '/repo/b.ts' }]);
    expect(db.prepare('SELECT * FROM locks WHERE lock_id = ?').get('lock_v2'))
      .toEqual({
        lock_id: 'lock_v2', file_path: '/repo/a.ts', run_id: 'run_v2',
        acquired_at: '2026-01-01T00:00:00Z', expires_at: '2099-01-01T00:00:00Z',
      });
    expect(tableColumns(db, 'task_runs').has('files_json')).toBe(false);
    expect(tableColumns(db, 'locks').has('lock_type')).toBe(false);
  });

  it('widens legacy refinement quality checks for instructions feedback', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE refinements (
        refinement_id  TEXT PRIMARY KEY,
        agent_id       TEXT NOT NULL,
        workspace_path TEXT NOT NULL,
        artifact       TEXT,
        repo           TEXT,
        ref            TEXT,
        files_json     TEXT NOT NULL DEFAULT '[]',
        reasoning      TEXT NOT NULL,
        remember       TEXT NOT NULL,
        quality        TEXT NOT NULL CHECK(quality IN ('good','bad','handoff')) DEFAULT 'good',
        state          TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
        created_at     TEXT NOT NULL,
        updated_at     TEXT NOT NULL
      );
      INSERT INTO refinements (
        refinement_id, agent_id, workspace_path, files_json, reasoning, remember,
        quality, state, created_at, updated_at
      )
      VALUES (
        'ref_legacy', 'agent-old', '/repo', '[]', 'old handoff', 'keep it',
        'handoff', 'open', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
      );
    `);

    initDb(db);
    const schema = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name='refinements'"
    ).get() as { sql: string };
    expect(schema.sql).toContain("'instructions'");

    const { refinement } = insertRefinement(db, {
      agentId: 'agent-new',
      workspacePath: '/repo',
      reasoning: 'instructions feedback',
      remember: 'clarify hook install flow',
      quality: 'instructions',
    });
    expect(refinement.quality).toBe('instructions');
    const count = db.prepare('SELECT COUNT(*) AS cnt FROM refinements')
      .get() as { cnt: number };
    expect(count.cnt).toBe(2);
  });

  // Generic CHECK-constraint drift repair (migrateCheckConstraints): the
  // column-only migration cannot widen a CHECK, so an old store whose enum is
  // narrower than the current DDL threw "CHECK constraint failed" on any insert
  // using a newer value. This must now be repaired for ANY table, not just the
  // hand-written refinements case.
  it('widens a legacy harness_log event_type CHECK and preserves its rows', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE harness_log (
        harness_id   TEXT PRIMARY KEY,
        session_id   TEXT,
        agent_id     TEXT NOT NULL,
        workspace_path TEXT,
        artifact     TEXT,
        event_type   TEXT NOT NULL CHECK(event_type IN ('mine','propose')),
        payload_json TEXT,
        memory_id    TEXT,
        task_id      TEXT,
        created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
      );
      INSERT INTO harness_log (harness_id, agent_id, event_type)
        VALUES ('hl_legacy', 'agent-old', 'mine');
    `);

    initDb(db);

    // The legacy row survives the table rebuild.
    const preserved = db.prepare(
      "SELECT COUNT(*) AS c FROM harness_log WHERE harness_id='hl_legacy'"
    ).get() as { c: number };
    expect(preserved.c).toBe(1);

    // Event types the narrow CHECK rejected now insert cleanly.
    for (const ev of ['reflect', 'validate', 'apply', 'capture'] as const) {
      expect(() => insertHarnessLog(db, { agentId: 'agent-new', eventType: ev }), ev).not.toThrow();
    }
  });

  it('does not rebuild a current-schema store (CHECK migration is a no-op)', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    insertHarnessLog(db, { agentId: 'a', eventType: 'reflect' });
    const before = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name='harness_log'"
    ).get() as { sql: string };

    initDb(db); // second init must not touch the table

    const after = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name='harness_log'"
    ).get() as { sql: string };
    expect(after.sql).toBe(before.sql);
    const rows = db.prepare('SELECT COUNT(*) AS c FROM harness_log').get() as { c: number };
    expect(rows.c).toBe(1);
  });
});
