import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertMemory, getMemory } from '../src/memory.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';

/**
 * Legacy-store migration tests.
 *
 * Regression for the production bug where initDb() created indexes on
 * migration-added columns (file_locks.session_id, agent_intents.workspace_path,
 * agent_memories scope columns) BEFORE ensure*Columns() added them, so any
 * store created by an older build failed connect with
 * "no such column: session_id" and every command died.
 *
 * The fixtures below mirror real pre-upgrade schemas captured from a
 * production ~/.octocode/memory/awareness.sqlite3.
 */

/** Schema as shipped immediately before the session_id change. */
function legacySessionIdEraDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec(`
    CREATE TABLE agent_memories (
      memory_id TEXT PRIMARY KEY,
      agent_id TEXT,
      task_context TEXT NOT NULL,
      observation TEXT NOT NULL,
      importance_score INTEGER NOT NULL DEFAULT 5,
      tags_json TEXT NOT NULL DEFAULT '[]',
      tags_text TEXT NOT NULL DEFAULT ',,',
      file_tree_fingerprint TEXT,
      created_at TEXT NOT NULL,
      state TEXT NOT NULL DEFAULT 'ACTIVE',
      label TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by TEXT,
      updated_at TEXT,
      file TEXT,
      novelty_score REAL,
      similar_memory_ids_json TEXT NOT NULL DEFAULT '[]',
      last_accessed_at TEXT,
      access_count INTEGER NOT NULL DEFAULT 0,
      decay_half_life_days REAL,
      failure_signature TEXT,
      valid_from TEXT,
      valid_to TEXT,
      expired_at TEXT,
      references_json TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      repo TEXT,
      ref TEXT,
      embedding BLOB,
      embedding_model TEXT
    );
    CREATE TABLE agent_intents (
      intent_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      plan_doc_ref TEXT,
      rationale TEXT NOT NULL,
      test_plan TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'ACTIVE',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      workspace_path TEXT,
      files_json TEXT
    );
    CREATE TABLE file_locks (
      lock_id TEXT PRIMARY KEY,
      file_path TEXT NOT NULL,
      intent_id TEXT,
      agent_id TEXT NOT NULL,
      lock_type TEXT NOT NULL DEFAULT 'EXCLUSIVE',
      acquired_at TEXT NOT NULL,
      expires_at TEXT NOT NULL
    );
    CREATE TABLE awareness_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
  `);
  return db;
}

/** Much older schema: intents predate even workspace_path/files_json. */
function ancientDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec(`
    CREATE TABLE agent_memories (
      memory_id TEXT PRIMARY KEY,
      agent_id TEXT,
      task_context TEXT NOT NULL,
      observation TEXT NOT NULL,
      importance_score INTEGER NOT NULL DEFAULT 5,
      tags_json TEXT NOT NULL DEFAULT '[]',
      tags_text TEXT NOT NULL DEFAULT ',,',
      file_tree_fingerprint TEXT,
      created_at TEXT NOT NULL
    );
    CREATE TABLE agent_intents (
      intent_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      plan_doc_ref TEXT,
      rationale TEXT NOT NULL,
      test_plan TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'ACTIVE',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );
    CREATE TABLE file_locks (
      lock_id TEXT PRIMARY KEY,
      file_path TEXT NOT NULL,
      intent_id TEXT,
      agent_id TEXT NOT NULL,
      lock_type TEXT NOT NULL DEFAULT 'EXCLUSIVE',
      acquired_at TEXT NOT NULL,
      expires_at TEXT NOT NULL
    );
  `);
  return db;
}

describe('initDb on legacy stores', () => {
  it('migrates a store that predates file_locks.session_id (production regression)', () => {
    const db = legacySessionIdEraDb();
    // Seed a pre-existing row so migration must preserve data, not just DDL.
    db.prepare(
      `INSERT INTO agent_memories (memory_id, task_context, observation, importance_score, created_at)
       VALUES ('mem_legacy1', 'legacy task', 'legacy observation about sqlite recall', 7, '2026-01-01T00:00:00Z')`
    ).run();

    expect(() => initDb(db)).not.toThrow();

    const lockCols = db.prepare('PRAGMA table_info(file_locks)').all().map((c) => (c as { name: string }).name);
    expect(lockCols).toContain('session_id');
    const intentCols = db.prepare('PRAGMA table_info(agent_intents)').all().map((c) => (c as { name: string }).name);
    expect(intentCols).toContain('session_id');

    // Existing data survives and the store is fully operational.
    const { memories } = getMemory(db, { query: 'legacy observation', limit: 5 });
    expect(memories.some((m) => m.memory_id === 'mem_legacy1')).toBe(true);

    const claim = preFlightIntent(db, {
      agentId: 'migrator',
      workspacePath: '/tmp/ws',
      rationale: 'post-migration lock',
      testPlan: 'this test',
      targetFiles: ['/tmp/ws/a.ts'],
    });
    expect(claim.ok).toBe(true);
    const release = releaseFileLock(db, {
      agentId: 'migrator',
      targetFiles: ['/tmp/ws/a.ts'],
      status: 'PENDING',
    });
    expect(release.released).toBe(true);
  });

  it('migrates an ancient store missing every migration-added column', () => {
    const db = ancientDb();
    expect(() => initDb(db)).not.toThrow();

    // All ensure*Columns additions present.
    const memCols = db.prepare('PRAGMA table_info(agent_memories)').all().map((c) => (c as { name: string }).name);
    for (const col of ['state', 'label', 'references_json', 'workspace_path', 'valid_from', 'embedding_model']) {
      expect(memCols).toContain(col);
    }

    // Store is usable end-to-end after migration.
    const { memoryId } = insertMemory(db, {
      agentId: 'a', taskContext: 'post-migration write', observation: 'works', importanceScore: 5,
    });
    expect(memoryId).toMatch(/^mem_/);
  });

  it('is idempotent — running initDb twice on a migrated legacy store is safe', () => {
    const db = legacySessionIdEraDb();
    initDb(db);
    expect(() => initDb(db)).not.toThrow();
  });
});
