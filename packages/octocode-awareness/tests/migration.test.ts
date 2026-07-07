import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { initDb } from '../src/db.js';
import { insertMemory } from '../src/memory.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const LEGACY_MIGRATE_SCRIPT = resolve(__dirname, '../skills/octocode-awareness/scripts/legacy-migrate.mjs');

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

function tableExists(db: DatabaseSync, table: string): boolean {
  const row = db.prepare("SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?").get(table);
  return Boolean(row);
}

function legacyFileDb(): { dbPath: string; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'oc-awareness-legacy-'));
  const dbPath = join(dir, 'awareness.sqlite3');
  const db = new DatabaseSync(dbPath);
  db.exec(`
    CREATE TABLE agent_memories (
      memory_id TEXT PRIMARY KEY,
      agent_id TEXT,
      task_context TEXT NOT NULL,
      observation TEXT NOT NULL,
      importance_score INTEGER NOT NULL DEFAULT 5,
      tags_json TEXT NOT NULL DEFAULT '[]',
      created_at TEXT NOT NULL,
      updated_at TEXT,
      label TEXT NOT NULL DEFAULT 'OTHER',
      references_json TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      repo TEXT,
      ref TEXT,
      file TEXT
    );
    CREATE TABLE memory_references (
      memory_id TEXT NOT NULL,
      reference TEXT NOT NULL,
      ordinal INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE agent_intents (
      intent_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
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
      acquired_at TEXT NOT NULL,
      expires_at TEXT NOT NULL
    );
  `);
  db.prepare(`
    INSERT INTO agent_memories (
      memory_id, agent_id, task_context, observation, importance_score,
      tags_json, references_json, workspace_path, repo, ref, file, label, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    'mem_legacy_1',
    'legacy-agent',
    'legacy import',
    'old awareness memory should become current',
    9,
    '["legacy","sqlite"]',
    '["https://example.test/legacy"]',
    '/tmp/legacy-ws',
    'owner/repo',
    'main',
    '/tmp/legacy-ws/src/a.ts',
    'gotcha',
    '2026-01-02T00:00:00.000Z',
    '2026-01-03T00:00:00.000Z',
  );
  db.prepare('INSERT INTO memory_references(memory_id, reference, ordinal) VALUES (?, ?, ?)')
    .run('mem_legacy_1', 'file:/tmp/legacy-ws/docs/note.md', 0);
  db.close();
  return { dbPath, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

function runLegacyMigrate(dbPath: string, args: string[] = []): { status: number; json: Record<string, unknown>; stderr: string } {
  const result = spawnSync(process.execPath, [LEGACY_MIGRATE_SCRIPT, '--db', dbPath, ...args], {
    encoding: 'utf8',
  });
  let json: Record<string, unknown> = {};
  try {
    json = JSON.parse(result.stdout);
  } catch {
    json = { parse_error: result.stdout };
  }
  return { status: result.status ?? 1, json, stderr: result.stderr };
}

describe('initDb on legacy stores', () => {
  /**
   * Schema v2 migration strategy: initDb() creates NEW tables (memories, tasks, locks, …)
   * alongside old tables (agent_memories, file_locks, agent_intents). Old tables are left
   * intact so a rollback can still read them. Data in old tables must be migrated separately
   * (out of scope for initDb itself — it is purely additive / idempotent).
   */

  it('does not throw on a store that predates file_locks.session_id', () => {
    const db = legacySessionIdEraDb();
    expect(() => initDb(db)).not.toThrow();
  });

  it('creates new schema tables on a legacy store alongside the old tables', () => {
    const db = legacySessionIdEraDb();
    initDb(db);

    // New tables must exist
    for (const table of ['memories', 'tasks', 'locks', 'memory_refs', 'signals']) {
      const row = db.prepare("SELECT name FROM sqlite_master WHERE name = ?").get(table) as { name: string } | undefined;
      expect(row, `new table "${table}" must exist after initDb on legacy store`).toBeDefined();
    }

    // Old tables still present (not dropped by initDb — additive-only migration)
    for (const table of ['agent_memories', 'agent_intents', 'file_locks']) {
      const row = db.prepare("SELECT name FROM sqlite_master WHERE name = ?").get(table) as { name: string } | undefined;
      expect(row, `legacy table "${table}" must be preserved (initDb is additive)`).toBeDefined();
    }
  });

  it('makes new operations (preFlightIntent / releaseFileLock / insertMemory) work after migration', () => {
    const db = legacySessionIdEraDb();
    initDb(db);

    // New writes to memories table work
    const { memoryId } = insertMemory(db, {
      agentId: 'a', taskContext: 'post-migration write', observation: 'works after migration', importance: 5,
    });
    expect(memoryId).toMatch(/^mem_/);

    // Intents via new tasks + locks tables work
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

  it('does not throw on an ancient store missing every migration-added column', () => {
    const db = ancientDb();
    expect(() => initDb(db)).not.toThrow();
  });

  it('new tables are fully functional after initDb on ancient store', () => {
    const db = ancientDb();
    initDb(db);

    const { memoryId } = insertMemory(db, {
      agentId: 'a', taskContext: 'post-migration write', observation: 'works', importance: 5,
    });
    expect(memoryId).toMatch(/^mem_/);
  });

  it('is idempotent — running initDb twice on a migrated legacy store is safe', () => {
    const db = legacySessionIdEraDb();
    initDb(db);
    expect(() => initDb(db)).not.toThrow();
  });
});

describe('standalone legacy-migrate skill script', () => {
  it('previews legacy tables without creating current tables', () => {
    const fixture = legacyFileDb();
    try {
      const result = runLegacyMigrate(fixture.dbPath);
      expect(result.status, result.stderr).toBe(0);
      expect(result.json['dry_run']).toBe(true);
      expect(result.json['source_memories']).toBe(1);

      const db = new DatabaseSync(fixture.dbPath);
      expect(tableExists(db, 'agent_memories')).toBe(true);
      expect(tableExists(db, 'memories')).toBe(false);
      db.close();
    } finally {
      fixture.cleanup();
    }
  });

  it('refuses to drop legacy tables without --write', () => {
    const fixture = legacyFileDb();
    try {
      const result = runLegacyMigrate(fixture.dbPath, ['--drop-legacy']);
      expect(result.status).toBe(1);
      expect(String(result.json['error'])).toContain('--drop-legacy requires --write');
    } finally {
      fixture.cleanup();
    }
  });

  it('copies legacy memories into current tables while preserving legacy tables', () => {
    const fixture = legacyFileDb();
    try {
      const result = runLegacyMigrate(fixture.dbPath, ['--write']);
      expect(result.status, result.stderr).toBe(0);
      expect(result.json['copied_memories']).toBe(1);
      expect(result.json['copied_references']).toBe(3);

      const db = new DatabaseSync(fixture.dbPath);
      const memory = db.prepare('SELECT * FROM memories WHERE memory_id = ?').get('mem_legacy_1') as Record<string, unknown>;
      expect(memory['task_context']).toBe('legacy import');
      expect(memory['importance']).toBe(9);
      expect(memory['label']).toBe('GOTCHA');
      expect(memory['workspace_path']).toBe('/tmp/legacy-ws');
      const refs = db.prepare('SELECT reference FROM memory_refs WHERE memory_id = ? ORDER BY ordinal')
        .all('mem_legacy_1') as Array<{ reference: string }>;
      expect(refs.map(r => r.reference)).toEqual([
        'https://example.test/legacy',
        'file:/tmp/legacy-ws/docs/note.md',
        'file:/tmp/legacy-ws/src/a.ts',
      ]);
      expect(tableExists(db, 'agent_memories')).toBe(true);
      db.close();
    } finally {
      fixture.cleanup();
    }
  });

  it('drops legacy tables only after an explicit write/drop run', () => {
    const fixture = legacyFileDb();
    try {
      expect(runLegacyMigrate(fixture.dbPath, ['--write']).status).toBe(0);
      const result = runLegacyMigrate(fixture.dbPath, ['--write', '--drop-legacy']);
      expect(result.status, result.stderr).toBe(0);
      expect(result.json['skipped_existing']).toBe(1);
      expect(result.json['dropped_tables']).toEqual(expect.arrayContaining(['agent_memories', 'memory_references', 'agent_intents', 'file_locks']));

      const db = new DatabaseSync(fixture.dbPath);
      expect(tableExists(db, 'memories')).toBe(true);
      expect(tableExists(db, 'agent_memories')).toBe(false);
      expect(tableExists(db, 'memory_references')).toBe(false);
      db.close();
    } finally {
      fixture.cleanup();
    }
  });
});
