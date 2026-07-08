/**
 * schema.test.ts — Structural tests for the clean db.ts schema.
 *
 * Verifies:
 *  - current table names are created
 *  - no extra application tables are created
 *  - column names match the current schema
 *  - FTS5 virtual table is created and functional
 *  - initDb is idempotent
 */

import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, tableColumns, hasFts } from '../src/db.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}


// ─── 1. Current tables are created ────────────────────────────────────────────

describe('initDb creates all required tables', () => {
  const db = freshDb();

  const requiredTables = [
    'memories',
    'memories_fts',
    'memory_refs',
    'tasks',
    'locks',
    'task_log',
    'signals',
    'signal_reads',
    'agents',
    'sessions',
    'refinements',
    'edit_log',
    'harness_log',
  ] as const;

  for (const table of requiredTables) {
    it(`creates table "${table}"`, () => {
      // memories_fts is a virtual table — check via sqlite_master directly
      const row = db.prepare(
        "SELECT name FROM sqlite_master WHERE name = ?"
      ).get(table) as { name: string } | undefined;
      expect(row, `expected "${table}" in sqlite_master`).toBeDefined();
    });
  }
});

// ─── 2. Table set stays constrained ──────────────────────────────────────────

describe('initDb table set', () => {
  const db = freshDb();

  it('creates only known application tables plus FTS internals', () => {
    const allowed = new Set([
      'sessions',
      'memories',
      'tasks',
      'locks',
      'task_log',
      'refinements',
      'signals',
      'signal_reads',
      'memory_refs',
      'agents',
      'edit_log',
      'harness_log',
    ]);
    const rows = db.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ).all() as Array<{ name: string }>;
    const unexpected = rows
      .map(r => r.name)
      .filter(name => !allowed.has(name) && !name.startsWith('memories_fts'));
    expect(unexpected).toEqual([]);
  });
});

// ─── 3. memories table columns ────────────────────────────────────────────────

describe('memories table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'memories')].sort();

  it('matches the current memory column set', () => {
    expect(cols).toEqual([
      'access_count',
      'agent_id',
      'artifact',
      'created_at',
      'decay_half_life_days',
      'embedding',
      'embedding_model',
      'expired_at',
      'failure_signature',
      'file_tree_fingerprint',
      'importance',
      'label',
      'last_accessed_at',
      'memory_id',
      'novelty_score',
      'observation',
      'ref',
      'repo',
      'state',
      'superseded_by',
      'tags_json',
      'task_context',
      'updated_at',
      'valid_from',
      'valid_to',
      'workspace_path',
    ]);
  });
});

// ─── 4. tasks table ──────────────────────────────────────────────────────────

describe('tasks table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'tasks')].sort();

  it('matches the current task column set', () => {
    expect(cols).toEqual([
      'agent_id',
      'artifact',
      'created_at',
      'files_json',
      'plan_doc_ref',
      'rationale',
      'session_id',
      'status',
      'task_id',
      'test_plan',
      'updated_at',
      'workspace_path',
    ]);
  });
});

// ─── 5. locks table ──────────────────────────────────────────────────────────

describe('locks table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'locks')].sort();

  it('matches the current lock column set', () => {
    expect(cols).toEqual([
      'acquired_at',
      'agent_id',
      'expires_at',
      'file_path',
      'lock_id',
      'lock_type',
      'session_id',
      'task_id',
    ]);
  });
});

// ─── 6. signals table ────────────────────────────────────────────────────────

describe('signals table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'signals')].sort();

  it('matches the current signal column set', () => {
    expect(cols).toEqual([
      'artifact',
      'body',
      'created_at',
      'files_json',
      'from_agent',
      'importance',
      'kind',
      'ref',
      'refs_json',
      'reply_to',
      'repo',
      'resolved_at',
      'signal_id',
      'status',
      'subject',
      'thread_id',
      'to_agent',
      'workspace_path',
    ]);
  });
});

// ─── 7. sessions table ───────────────────────────────────────────────────────

describe('sessions table column names', () => {
  const db = freshDb();
  const cols = tableColumns(db, 'sessions');

  it('has "session_id" as primary key column', () => {
    expect(cols.has('session_id')).toBe(true);
  });

  it('has expected session columns', () => {
    for (const col of ['agent_id', 'workspace_path', 'repo', 'ref', 'started_at', 'ended_at', 'summary']) {
      expect(cols.has(col), `missing column: ${col}`).toBe(true);
    }
  });
});

// ─── 8. idempotency ──────────────────────────────────────────────────────────

describe('initDb idempotency', () => {
  it('calling initDb twice on the same db does not throw', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    expect(() => {
      initDb(db);
      initDb(db);
    }).not.toThrow();
  });

  it('calling initDb three times preserves existing rows', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    // Insert a row after first init
    db.prepare(
      `INSERT INTO agents(agent_id, registered_at, last_seen_at)
       VALUES ('agent-1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`
    ).run();
    // Second and third init must not wipe data
    initDb(db);
    initDb(db);
    const row = db.prepare('SELECT * FROM agents WHERE agent_id = ?').get('agent-1') as { agent_id: string } | undefined;
    expect(row?.agent_id).toBe('agent-1');
  });
});

// ─── 9. memories_fts virtual table ───────────────────────────────────────────

describe('memories_fts virtual table', () => {
  it('hasFts returns true after initDb', () => {
    const db = freshDb();
    expect(hasFts(db)).toBe(true);
  });

  it('insert and search via FTS5 returns matching row', () => {
    const db = freshDb();
    if (!hasFts(db)) return; // skip if fts5 unavailable in this build

    // Insert a memory row first so FK / consistency holds
    db.prepare(`
      INSERT INTO memories(memory_id, agent_id, task_context, observation, importance, created_at)
      VALUES ('mem_fts_test', 'agent-1', 'authentication flow', 'JWT must be validated on every request', 7, '2026-01-01T00:00:00.000Z')
    `).run();

    // Insert into FTS shadow table
    db.prepare(
      'INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
    ).run('mem_fts_test', 'authentication flow', 'JWT must be validated on every request', 'security auth');

    // Full-text search should find the row
    const rows = db.prepare(
      "SELECT memory_id FROM memories_fts WHERE memories_fts MATCH 'JWT' ORDER BY rank"
    ).all() as { memory_id: string }[];

    expect(rows.length).toBeGreaterThanOrEqual(1);
    expect(rows.some(r => r.memory_id === 'mem_fts_test')).toBe(true);
  });

  it('FTS search returns no results for unrelated term', () => {
    const db = freshDb();
    if (!hasFts(db)) return;

    const rows = db.prepare(
      "SELECT memory_id FROM memories_fts WHERE memories_fts MATCH 'xyzzy_nonexistent_term_abc'"
    ).all() as { memory_id: string }[];

    expect(rows.length).toBe(0);
  });

  it('FTS searches across task_context and observation columns', () => {
    const db = freshDb();
    if (!hasFts(db)) return;

    db.prepare(`
      INSERT INTO memories(memory_id, agent_id, task_context, observation, importance, created_at)
      VALUES ('mem_ctx', 'agent-2', 'database indexing strategy', 'use partial indexes for sparse columns', 6, '2026-01-02T00:00:00.000Z')
    `).run();

    db.prepare(
      'INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
    ).run('mem_ctx', 'database indexing strategy', 'use partial indexes for sparse columns', 'db perf');

    // Match from task_context
    const ctx = db.prepare(
      "SELECT memory_id FROM memories_fts WHERE memories_fts MATCH 'indexing'"
    ).all() as { memory_id: string }[];
    expect(ctx.some(r => r.memory_id === 'mem_ctx')).toBe(true);

    // Match from observation
    const obs = db.prepare(
      "SELECT memory_id FROM memories_fts WHERE memories_fts MATCH 'partial'"
    ).all() as { memory_id: string }[];
    expect(obs.some(r => r.memory_id === 'mem_ctx')).toBe(true);
  });
});
