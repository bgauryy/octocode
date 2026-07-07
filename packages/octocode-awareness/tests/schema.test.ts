/**
 * schema.test.ts — Structural tests for the clean db.ts schema.
 *
 * Verifies:
 *  - new table names are created
 *  - old/removed table names are absent
 *  - column names match the new schema (importance, task_id, signal_id, etc.)
 *  - legacy columns are absent (importance_score, intent_id, tags_text, etc.)
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


// ─── 1. New tables are created ────────────────────────────────────────────────

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

// ─── 2. Old/removed table names are absent ────────────────────────────────────

describe('initDb does NOT create legacy tables', () => {
  const db = freshDb();

  const removedTables = [
    'agent_memories',
    'agent_intents',
    'file_locks',
    'notifications',
    'agent_identities',
    'awareness_meta',
  ] as const;

  for (const table of removedTables) {
    it(`does not create legacy table "${table}"`, () => {
      const row = db.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?"
      ).get(table) as { name: string } | undefined;
      expect(row, `legacy table "${table}" must not exist`).toBeUndefined();
    });
  }
});

// ─── 3. memories table columns ────────────────────────────────────────────────

describe('memories table column names', () => {
  const db = freshDb();
  const cols = tableColumns(db, 'memories');

  it('has "importance" column', () => {
    expect(cols.has('importance')).toBe(true);
  });

  it('does NOT have "importance_score"', () => {
    expect(cols.has('importance_score')).toBe(false);
  });

  it('does NOT have "tags_text"', () => {
    expect(cols.has('tags_text')).toBe(false);
  });

  it('does NOT have "references_json"', () => {
    expect(cols.has('references_json')).toBe(false);
  });

  it('does NOT have "similar_memory_ids_json"', () => {
    expect(cols.has('similar_memory_ids_json')).toBe(false);
  });

  it('has core identity and content columns', () => {
    for (const col of ['memory_id', 'agent_id', 'task_context', 'observation', 'state', 'label']) {
      expect(cols.has(col), `missing column: ${col}`).toBe(true);
    }
  });

  it('has tags_json (not tags_text)', () => {
    expect(cols.has('tags_json')).toBe(true);
  });
});

// ─── 4. tasks table: task_id not intent_id ────────────────────────────────────

describe('tasks table column names', () => {
  const db = freshDb();
  const cols = tableColumns(db, 'tasks');

  it('has "task_id" as primary key column', () => {
    expect(cols.has('task_id')).toBe(true);
  });

  it('does NOT have "intent_id"', () => {
    expect(cols.has('intent_id')).toBe(false);
  });

  it('has expected task columns', () => {
    for (const col of ['agent_id', 'session_id', 'rationale', 'test_plan', 'status', 'workspace_path', 'files_json', 'created_at', 'updated_at']) {
      expect(cols.has(col), `missing column: ${col}`).toBe(true);
    }
  });
});

// ─── 5. locks table: task_id FK not intent_id ────────────────────────────────

describe('locks table column names', () => {
  const db = freshDb();
  const cols = tableColumns(db, 'locks');

  it('has "task_id" foreign key column', () => {
    expect(cols.has('task_id')).toBe(true);
  });

  it('does NOT have "intent_id"', () => {
    expect(cols.has('intent_id')).toBe(false);
  });

  it('has expected lock columns', () => {
    for (const col of ['lock_id', 'file_path', 'agent_id', 'session_id', 'lock_type', 'acquired_at', 'expires_at']) {
      expect(cols.has(col), `missing column: ${col}`).toBe(true);
    }
  });
});

// ─── 6. signals table ────────────────────────────────────────────────────────

describe('signals table column names', () => {
  const db = freshDb();
  const cols = tableColumns(db, 'signals');

  it('has "signal_id" as primary key column', () => {
    expect(cols.has('signal_id')).toBe(true);
  });

  it('has "reply_to" column', () => {
    expect(cols.has('reply_to')).toBe(true);
  });

  it('has "resolved_at" column', () => {
    expect(cols.has('resolved_at')).toBe(true);
  });

  it('has expected signal columns', () => {
    for (const col of ['workspace_path', 'from_agent', 'to_agent', 'kind', 'subject', 'body', 'thread_id', 'status', 'created_at']) {
      expect(cols.has(col), `missing column: ${col}`).toBe(true);
    }
  });

  it('does NOT have old "notification_id" column', () => {
    expect(cols.has('notification_id')).toBe(false);
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
