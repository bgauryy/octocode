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
import {
  AWARENESS_APPLICATION_ID,
  AWARENESS_SCHEMA_VERSION,
  getDeliveryFingerprint,
  hasFts,
  initDb,
  setDeliveryFingerprint,
  tableColumns,
} from '../src/db.js';

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
    'plans',
    'plan_members',
    'plan_docs',
    'tasks',
    'task_paths',
    'task_dependencies',
    'task_claims',
    'task_events',
    'task_runs',
    'run_files',
    'locks',
    'delivery_state',
    'run_log',
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
      'plans',
      'plan_members',
      'plan_docs',
      'tasks',
      'task_paths',
      'task_dependencies',
      'task_claims',
      'task_events',
      'task_runs',
      'run_files',
      'locks',
      'delivery_state',
      'run_log',
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
      'acceptance_criteria',
      'completed_at',
      'created_at',
      'created_by',
      'plan_id',
      'priority',
      'reasoning',
      'status',
      'task_id',
      'title',
      'updated_at',
    ]);
  });
});

describe('task_runs table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'task_runs')].sort();

  it('keeps execution attempts separate from durable tasks', () => {
    expect(cols).toEqual([
      'agent_id',
      'artifact',
      'context_ref',
      'created_at',
      'origin',
      'rationale',
      'run_id',
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
      'expires_at',
      'file_path',
      'lock_id',
      'run_id',
    ]);
  });
});

describe('run_files table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'run_files')].sort();

  it('normalizes advisory file presence separately from runs and locks', () => {
    expect(cols).toEqual([
      'ended_at',
      'expires_at',
      'file_path',
      'heartbeat_at',
      'reason_override',
      'run_id',
      'source',
      'started_at',
    ]);
  });
});

describe('delivery_state table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'delivery_state')].sort();

  it('stores compact-output fingerprints without duplicating awareness payloads', () => {
    expect(cols).toEqual([
      'channel',
      'consumer_id',
      'delivered_at',
      'fingerprint',
      'scope_key',
    ]);
  });
});

describe('canonical schema identity', () => {
  it('is the sole OCT1 schema version 1', () => {
    const db = freshDb();
    expect(db.prepare('PRAGMA application_id').get())
      .toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(db.prepare('PRAGMA user_version').get())
      .toEqual({ user_version: AWARENESS_SCHEMA_VERSION });
    expect(AWARENESS_SCHEMA_VERSION).toBe(1);
  });

  it('rejects an extra application relation on the canonical fast path', () => {
    const db = freshDb();
    db.exec('CREATE TABLE unexpected_state(value TEXT)');
    expect(() => initDb(db)).toThrow(/canonical v1 relation contract mismatch.*unexpected_state/i);
  });

  it('rejects canonical-header structural drift', () => {
    const db = freshDb();
    db.exec(`
      PRAGMA foreign_keys = OFF;
      DROP TABLE task_paths;
      CREATE TABLE task_paths(task_id TEXT NOT NULL, path TEXT NOT NULL, ordinal INTEGER NOT NULL DEFAULT 0);
    `);
    expect(() => initDb(db)).toThrow(/canonical v1 schema fingerprint mismatch/i);
  });

  it('rejects a missing canonical index', () => {
    const db = freshDb();
    db.exec('DROP INDEX idx_sessions_agent');
    expect(() => initDb(db)).toThrow(/canonical v1 schema fingerprint mismatch/i);
  });

  it('rejects an unexpected trigger', () => {
    const db = freshDb();
    db.exec(`CREATE TRIGGER destructive_memory_trigger AFTER INSERT ON memories
      BEGIN DELETE FROM memories WHERE memory_id = NEW.memory_id; END`);
    expect(() => initDb(db)).toThrow(/canonical v1 schema fingerprint mismatch/i);
  });
});

describe('delivery fingerprints', () => {
  it('upserts one fingerprint per consumer, channel, and scope', () => {
    const db = freshDb();
    const key = { consumerId: 'agent-a', channel: 'briefing', scopeKey: '/repo|-|session-a' };
    expect(getDeliveryFingerprint(db, key)).toBeNull();
    setDeliveryFingerprint(db, { ...key, fingerprint: 'v1' });
    expect(getDeliveryFingerprint(db, key)).toBe('v1');
    setDeliveryFingerprint(db, { ...key, fingerprint: 'v2' });
    expect(getDeliveryFingerprint(db, key)).toBe('v2');
    expect(db.prepare('SELECT COUNT(*) AS count FROM delivery_state').get()).toEqual({ count: 1 });
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
