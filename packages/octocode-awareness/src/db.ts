/**
 * db.ts — SQLite connection, schema init, and migration helpers.
 * Requires Node >=22 (node:sqlite built-in).
 */

import { DatabaseSync } from 'node:sqlite';
import { mkdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { homedir, platform } from 'node:os';

import { parseJsonList } from './helpers.js';
import type { TableInfoRow, MetaRow, MemoryRow } from './types.js';

const REFERENCES_INDEX_VERSION = '1';
const REFINEMENT_QUALITY_SCHEMA_VERSION = '2';

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_DB_NAME = 'awareness.sqlite3';
const MEMORY_HOME_ENV = 'OCTOCODE_MEMORY_HOME';
const FTS_INDEX_VERSION = '2';

// ─── Path resolution ──────────────────────────────────────────────────────────

/** Resolve the memory home directory from env or platform defaults. */
export function memoryHome(): string {
  const configured = process.env[MEMORY_HOME_ENV];
  if (configured?.trim()) return resolve(configured.trim());

  const h = homedir();
  const p = platform();
  if (p === 'win32') {
    const appData = process.env['APPDATA'] ?? join(h, 'AppData', 'Roaming');
    return join(appData, '.octocode', 'memory');
  }
  if (p === 'darwin') return join(h, '.octocode', 'memory');
  const xdg = process.env['XDG_CONFIG_HOME'] ?? join(h, '.config');
  return join(xdg, '.octocode', 'memory');
}

/** Resolve a DB path from an override arg or the default location. */
export function resolveDbPath(dbArg?: string | null): string {
  if (dbArg) return resolve(dbArg);
  return join(memoryHome(), DEFAULT_DB_NAME);
}

// ─── Connection ───────────────────────────────────────────────────────────────

/**
 * Open (or create) the SQLite database and initialize the schema.
 */
export function connectDb(dbPath: string): DatabaseSync {
  mkdirSync(dirname(dbPath), { recursive: true });
  const db = new DatabaseSync(dbPath);
  db.exec('PRAGMA foreign_keys = ON');
  db.exec('PRAGMA busy_timeout = 5000');
  db.exec('PRAGMA journal_mode = WAL');
  initDb(db);
  return db;
}

// ─── Schema ───────────────────────────────────────────────────────────────────

export function initDb(db: DatabaseSync): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS agent_memories (
      memory_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      task_context TEXT NOT NULL,
      observation TEXT NOT NULL,
      importance_score INTEGER NOT NULL CHECK(importance_score BETWEEN 1 AND 10),
      state TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
      label TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by TEXT,
      tags_json TEXT NOT NULL DEFAULT '[]',
      tags_text TEXT NOT NULL DEFAULT ',',
      references_json TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      repo TEXT,
      ref TEXT,
      file_tree_fingerprint TEXT,
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
      embedding BLOB,
      embedding_model TEXT,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT
    );

    CREATE TABLE IF NOT EXISTS agent_intents (
      intent_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      session_id TEXT,
      plan_doc_ref TEXT,
      rationale TEXT NOT NULL,
      test_plan TEXT NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('PENDING','ACTIVE','SUCCESS','FAILED')) DEFAULT 'ACTIVE',
      workspace_path TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS file_locks (
      lock_id TEXT PRIMARY KEY,
      file_path TEXT NOT NULL,
      intent_id TEXT NOT NULL,
      agent_id TEXT NOT NULL,
      session_id TEXT,
      lock_type TEXT NOT NULL CHECK(lock_type IN ('SHARED','EXCLUSIVE')),
      acquired_at TEXT NOT NULL,
      expires_at TEXT,
      FOREIGN KEY(intent_id) REFERENCES agent_intents(intent_id) ON DELETE CASCADE,
      UNIQUE(file_path, intent_id)
    );

    CREATE TABLE IF NOT EXISTS refinements (
      refinement_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      workspace_path TEXT NOT NULL,
      repo TEXT,
      ref TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      reasoning TEXT NOT NULL,
      remember TEXT NOT NULL,
      quality TEXT NOT NULL CHECK(quality IN ('good','bad','handoff')) DEFAULT 'good',
      state TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS notifications (
      notification_id TEXT PRIMARY KEY,
      workspace_path TEXT NOT NULL,
      repo TEXT,
      ref TEXT,
      from_agent TEXT NOT NULL,
      to_agent TEXT,
      kind TEXT NOT NULL,
      subject TEXT NOT NULL,
      body TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      refs_json TEXT NOT NULL DEFAULT '[]',
      thread_id TEXT NOT NULL,
      in_reply_to TEXT,
      importance INTEGER NOT NULL DEFAULT 5,
      status TEXT NOT NULL DEFAULT 'open',
      created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS memory_references (
      memory_id TEXT NOT NULL,
      reference TEXT NOT NULL,
      kind TEXT,
      ordinal INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (memory_id, reference),
      FOREIGN KEY(memory_id) REFERENCES agent_memories(memory_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_memory_references_ref ON memory_references(reference);
    CREATE INDEX IF NOT EXISTS idx_memory_references_kind ON memory_references(kind);

    CREATE TABLE IF NOT EXISTS intent_events (
      event_id TEXT PRIMARY KEY,
      intent_id TEXT,
      agent_id TEXT NOT NULL,
      event_type TEXT NOT NULL,
      message TEXT NOT NULL,
      created_at TEXT NOT NULL,
      FOREIGN KEY(intent_id) REFERENCES agent_intents(intent_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS awareness_meta (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS notification_reads (
      notification_id TEXT NOT NULL,
      agent_id TEXT NOT NULL,
      read_at TEXT NOT NULL,
      PRIMARY KEY (notification_id, agent_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_memories_importance ON agent_memories(importance_score);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_created_at ON agent_memories(created_at);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_state ON agent_memories(state);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_label ON agent_memories(label);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_file ON agent_memories(file);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_failure_sig ON agent_memories(failure_signature);
    CREATE INDEX IF NOT EXISTS idx_file_locks_file_path ON file_locks(file_path);
    CREATE INDEX IF NOT EXISTS idx_file_locks_agent_id ON file_locks(agent_id);
    CREATE INDEX IF NOT EXISTS idx_file_locks_acquired_at ON file_locks(acquired_at);
    CREATE INDEX IF NOT EXISTS idx_file_locks_expires_at ON file_locks(expires_at);
    CREATE INDEX IF NOT EXISTS idx_refinements_state ON refinements(state);
    CREATE INDEX IF NOT EXISTS idx_refinements_repo ON refinements(repo);
  `);

  ensureMemoryColumns(db);
  ensureIntentColumns(db);
  ensureRefinementQualitySchema(db);
  ensureMemoryReferencesVersion(db);

  try {
    db.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts
      USING fts5(memory_id UNINDEXED, task_context, observation, tags)
    `);
  } catch {
    /* fts5 unavailable or already exists */
  }

  ensureFtsVersion(db);
}

// ─── Migration helpers ────────────────────────────────────────────────────────

export function tableColumns(db: DatabaseSync, tableName: string): Set<string> {
  const rows = db.prepare(`PRAGMA table_info(${tableName})`).all() as unknown as TableInfoRow[];
  return new Set(rows.map(r => r.name));
}

export function ensureMemoryColumns(db: DatabaseSync): void {
  const cols = tableColumns(db, 'agent_memories');
  const alterations: [string, string][] = [
    ['state', "TEXT NOT NULL DEFAULT 'ACTIVE'"],
    ['label', "TEXT NOT NULL DEFAULT 'OTHER'"],
    ['superseded_by', 'TEXT'],
    ['updated_at', 'TEXT'],
    ['file', 'TEXT'],
    ['novelty_score', 'REAL'],
    ['similar_memory_ids_json', "TEXT NOT NULL DEFAULT '[]'"],
    ['last_accessed_at', 'TEXT'],
    ['access_count', 'INTEGER NOT NULL DEFAULT 0'],
    ['decay_half_life_days', 'REAL'],
    ['failure_signature', 'TEXT'],
    ['valid_from', 'TEXT'],
    ['valid_to', 'TEXT'],
    ['expired_at', 'TEXT'],
    ['references_json', "TEXT NOT NULL DEFAULT '[]'"],
    ['workspace_path', 'TEXT'],
    ['repo', 'TEXT'],
    ['ref', 'TEXT'],
    ['embedding', 'BLOB'],
    ['embedding_model', 'TEXT'],
  ];
  for (const [col, def] of alterations) {
    if (!cols.has(col)) {
      db.exec(`ALTER TABLE agent_memories ADD COLUMN ${col} ${def}`);
    }
  }
}

export function ensureIntentColumns(db: DatabaseSync): void {
  const cols = tableColumns(db, 'agent_intents');
  if (!cols.has('workspace_path')) {
    db.exec('ALTER TABLE agent_intents ADD COLUMN workspace_path TEXT');
  }
  if (!cols.has('files_json')) {
    db.exec("ALTER TABLE agent_intents ADD COLUMN files_json TEXT NOT NULL DEFAULT '[]'");
  }
  if (!cols.has('session_id')) {
    db.exec('ALTER TABLE agent_intents ADD COLUMN session_id TEXT');
  }

  const lockCols = tableColumns(db, 'file_locks');
  if (!lockCols.has('session_id')) {
    db.exec('ALTER TABLE file_locks ADD COLUMN session_id TEXT');
  }
  db.exec('CREATE INDEX IF NOT EXISTS idx_file_locks_session_id ON file_locks(session_id)');
}

function rewriteLegacyHandoffRefinements(db: DatabaseSync): void {
  db.prepare(
    "UPDATE refinements SET quality = 'handoff', updated_at = COALESCE(updated_at, created_at) WHERE quality <> 'handoff' AND remember LIKE 'Review session handoff%'"
  ).run();
}

export function ensureRefinementQualitySchema(db: DatabaseSync): void {
  const row = db.prepare(
    "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'refinements'"
  ).get() as { sql: string | null } | undefined;

  if (!row?.sql) return;

  if (!row.sql.includes("'handoff'")) {
    db.exec('ALTER TABLE refinements RENAME TO refinements_old_quality_migration');
    db.exec(`
      CREATE TABLE refinements (
        refinement_id TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        workspace_path TEXT NOT NULL,
        repo TEXT,
        ref TEXT,
        files_json TEXT NOT NULL DEFAULT '[]',
        reasoning TEXT NOT NULL,
        remember TEXT NOT NULL,
        quality TEXT NOT NULL CHECK(quality IN ('good','bad','handoff')) DEFAULT 'good',
        state TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
      );
      INSERT INTO refinements (
        refinement_id, agent_id, workspace_path, repo, ref,
        files_json, reasoning, remember, quality, state, created_at, updated_at
      )
      SELECT
        refinement_id, agent_id, workspace_path, repo, ref,
        files_json, reasoning, remember,
        CASE WHEN remember LIKE 'Review session handoff%' THEN 'handoff' ELSE quality END,
        state, created_at, updated_at
      FROM refinements_old_quality_migration;
      DROP TABLE refinements_old_quality_migration;
    `);
  }

  rewriteLegacyHandoffRefinements(db);
  db.prepare("INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('refinement_quality_schema_version', ?)")
    .run(REFINEMENT_QUALITY_SCHEMA_VERSION);
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_refinements_state ON refinements(state);
    CREATE INDEX IF NOT EXISTS idx_refinements_repo ON refinements(repo);
  `);
}

// ─── FTS helpers ──────────────────────────────────────────────────────────────

export function hasFts(db: DatabaseSync): boolean {
  const row = db.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_fts'"
  ).get() as Record<string, number> | undefined;
  return Boolean(row);
}

export function ftsTermsForRow(row: Partial<MemoryRow>): string {
  const tags = parseJsonList(row.tags_json);
  const refs = parseJsonList(row.references_json);
  const label = (row.label ?? 'OTHER').toLowerCase();
  return [
    ...tags, ...refs, label,
    row.file ?? '', row.failure_signature ?? '',
    row.workspace_path ?? '', row.repo ?? '', row.ref ?? '',
  ].filter(Boolean).join(' ');
}

// ─── Memory references ───────────────────────────────────────────────────────────

export function referenceKind(reference: string): string {
  if (/^https?:\/\//.test(reference)) return 'url';
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.\-]*):/);
  return m ? m[1]!.toLowerCase() : 'other';
}

export function replaceMemoryReferences(db: DatabaseSync, memoryId: string, references: string[]): void {
  db.prepare('DELETE FROM memory_references WHERE memory_id = ?').run(memoryId);
  const insert = db.prepare(
    'INSERT OR REPLACE INTO memory_references(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)'
  );
  references.forEach((ref, i) => insert.run(memoryId, ref, referenceKind(ref), i));
}

function backfillMemoryReferences(db: DatabaseSync): void {
  const rows = db.prepare('SELECT memory_id, references_json FROM agent_memories').all() as
    unknown as Array<{ memory_id: string; references_json: string }>;
  for (const row of rows) {
    const refs = parseJsonList(row.references_json);
    if (refs.length > 0) replaceMemoryReferences(db, row.memory_id, refs);
  }
}

export function ensureMemoryReferencesVersion(db: DatabaseSync): void {
  try {
    const row = db.prepare("SELECT value FROM awareness_meta WHERE key='memory_references_version'").get() as MetaRow | undefined;
    if (row?.value === REFERENCES_INDEX_VERSION) return;
    backfillMemoryReferences(db);
    db.prepare("INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('memory_references_version', ?)").run(REFERENCES_INDEX_VERSION);
  } catch {
    // awareness_meta may not exist yet on very old DBs; skip silently
  }
}

// ─── FTS ───────────────────────────────────────────────────────────────────

export function rebuildFts(db: DatabaseSync): void {
  db.exec('DELETE FROM memory_fts');
  const rows = db.prepare('SELECT * FROM agent_memories').all() as unknown as MemoryRow[];
  const insert = db.prepare(
    'INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
  );
  for (const row of rows) {
    insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
  }
}

export function ensureFtsVersion(db: DatabaseSync): void {
  if (!hasFts(db)) return;
  const row = db.prepare(
    "SELECT value FROM awareness_meta WHERE key='memory_fts_version'"
  ).get() as MetaRow | undefined;
  if (row?.value === FTS_INDEX_VERSION) return;
  rebuildFts(db);
  db.prepare(
    "INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('memory_fts_version', ?)"
  ).run(FTS_INDEX_VERSION);
}
