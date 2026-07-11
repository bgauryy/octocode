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
import { hasFts, initDb } from '../src/db.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

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
