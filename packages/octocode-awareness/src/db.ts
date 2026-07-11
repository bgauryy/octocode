/**
 * db.ts — SQLite connection, schema init, and utility helpers.
 * Requires Node >=22.13.0 (unflagged node:sqlite built-in).
 *
 * Clean schema scope:
 *   workspace_path is the primary isolation key.
 *   artifact is the optional workspace-local package/service/component slice.
 */

import { DatabaseSync, type SQLInputValue } from 'node:sqlite';
import { createHash } from 'node:crypto';
import { mkdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { homedir, platform } from 'node:os';

import { parseJsonList, utcNow } from './helpers.js';
import type { TableInfoRow, MemoryRow } from './types.js';
import { journalModeForSqliteVersion } from './sqlite-runtime.js';

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_DB_NAME = 'awareness.sqlite3';
const MEMORY_HOME_ENV = 'OCTOCODE_MEMORY_HOME';
/** ASCII "OCT1". Canonical Awareness has one executable schema contract. */
export const AWARENESS_APPLICATION_ID = 0x4f435431;
export const AWARENESS_SCHEMA_VERSION = 1;
const LEGACY_MAX_USER_VERSION = 3;
const SQLITE_BUSY_RETRY_MS = 25;
const SQLITE_BUSY_DEADLINE_MS = 10_000;
const SQLITE_WAIT = new Int32Array(new SharedArrayBuffer(4));
const LEGACY_V0_RELATION_NAMES = new Set([
  'agent_intents',
  'agent_memories',
  'file_locks',
  'intent_events',
  'memory_fts',
  'notifications',
  'notification_reads',
  'task_log',
]);

// ─── Module-level singleton ───────────────────────────────────────────────────

let _db: DatabaseSync | undefined;
const _dbCache = new Map<string, DatabaseSync>();

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
 * Open (or create) the SQLite database, initialise the schema, and cache the
 * connection in the module-level singleton so getDb() works after the call.
 */
export function connectDb(dbPath: string): DatabaseSync {
  mkdirSync(dirname(dbPath), { recursive: true });
  const db = new DatabaseSync(dbPath);
  try {
    // busy_timeout is connection-local and must precede the identity reads: in
    // rollback-journal mode, a concurrent writer may otherwise make this
    // read-only guard fail immediately with SQLITE_BUSY.
    db.exec(`PRAGMA busy_timeout = ${SQLITE_BUSY_DEADLINE_MS}`);
    // Fail closed before journal mode, foreign-key state, DDL, or migrations can
    // touch a foreign/future store. A recognized legacy store is backed up before
    // its first write; canonical OCT1 stores take a strict read-only fast path.
    const schemaState = inspectSchemaState(db);
    if (schemaState === 'legacy') createPreV1Backup(db, dbPath);
    const versionRow = db.prepare('SELECT sqlite_version() AS version').get() as { version: string };
    const journalMode = journalModeForSqliteVersion(versionRow.version);
    // Unsafe embedded SQLite versions use rollback journaling instead of the
    // documented concurrent-WAL path. Changing mode is a write and may race a
    // first opener, so both modes use the same bounded BUSY retry.
    withSqliteBusyRetry(() => db.exec(`PRAGMA journal_mode = ${journalMode}`));
    db.exec('PRAGMA foreign_keys = ON');
    initializeDb(db, schemaState === 'legacy', schemaState);
    _db = db;
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

interface SchemaIdentity {
  applicationId: number;
  userVersion: number;
  relations: Array<{ name: string; type: string }>;
}

type SchemaState = 'fresh' | 'canonical' | 'legacy';

function readSchemaIdentity(db: DatabaseSync): SchemaIdentity {
  const application = db.prepare('PRAGMA application_id').get() as { application_id: number };
  const version = db.prepare('PRAGMA user_version').get() as { user_version: number };
  const relations = db.prepare(`
    SELECT name, type
    FROM sqlite_schema
    WHERE type IN ('table', 'view')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
      AND name NOT GLOB 'memory_fts_*'
    ORDER BY name
  `).all() as Array<{ name: string; type: string }>;
  return {
    applicationId: application.application_id ?? 0,
    userVersion: version.user_version ?? 0,
    relations,
  };
}

function hasColumns(db: DatabaseSync, table: string, columns: string[]): boolean {
  if (!tableExists(db, table)) return false;
  const actual = tableColumns(db, table);
  return columns.every((column) => actual.has(column));
}

function recognizedLegacySignature(db: DatabaseSync): boolean {
  const matchedTables = new Set<string>();
  for (const [table, columns] of [
    ['sessions', ['session_id', 'agent_id', 'started_at']],
    ['memories', ['memory_id', 'agent_id', 'task_context', 'observation', 'importance']],
    ['tasks', ['task_id', 'agent_id', 'rationale', 'test_plan', 'status']],
    ['task_runs', ['run_id', 'agent_id', 'rationale', 'test_plan', 'status']],
    ['locks', ['lock_id', 'file_path', 'acquired_at']],
    ['refinements', ['refinement_id', 'agent_id', 'reasoning', 'remember', 'state']],
    ['agent_intents', ['intent_id', 'agent_id', 'rationale', 'test_plan', 'status']],
    ['intent_events', ['event_id', 'intent_id', 'agent_id', 'event_type']],
    ['agent_memories', ['memory_id', 'agent_id', 'task_context', 'observation', 'importance_score']],
  ] as Array<[string, string[]]>) {
    if (hasColumns(db, table, columns)) matchedTables.add(table);
  }
  for (const [table, columns] of canonicalColumns()) {
    if (columns.length >= 2 && hasColumns(db, table, columns.slice(0, 2).map(({ name }) => name))) {
      matchedTables.add(table);
    }
  }
  // One generic-looking table is not enough authority to mutate a database.
  // Every shipped legacy generation had at least two recognizable relations.
  return matchedTables.size >= 2;
}

function inspectSchemaState(db: DatabaseSync): SchemaState {
  const identity = readSchemaIdentity(db);
  if (identity.applicationId === AWARENESS_APPLICATION_ID) {
    if (identity.userVersion !== AWARENESS_SCHEMA_VERSION) {
      throw new Error(
        `unsupported canonical Awareness schema version ${identity.userVersion}; expected ${AWARENESS_SCHEMA_VERSION}`,
      );
    }
    assertCanonicalRelationContract(db, identity.relations);
    assertCanonicalSchemaFingerprint(db);
    return 'canonical';
  }
  if (identity.applicationId !== 0) {
    throw new Error(
      `refusing foreign Awareness application_id ${identity.applicationId}; expected ${AWARENESS_APPLICATION_ID}`,
    );
  }
  if (identity.userVersion > LEGACY_MAX_USER_VERSION) {
    throw new Error(
      `refusing unsupported unbranded Awareness schema version ${identity.userVersion}; legacy versions are 0-${LEGACY_MAX_USER_VERSION}`,
    );
  }
  if (identity.relations.length === 0) {
    if (identity.userVersion === 0) return 'fresh';
    throw new Error(`refusing unrelated empty versioned SQLite store at user_version ${identity.userVersion}`);
  }

  const known = new Set<string>([
    ...canonicalColumns().keys(),
    ...LEGACY_V0_RELATION_NAMES,
    'memories_fts',
  ]);
  const unexpected = identity.relations.filter(({ name, type }) => type !== 'table' || !known.has(name));
  if (unexpected.length > 0 || !recognizedLegacySignature(db)) {
    const suffix = unexpected.length > 0
      ? `; unexpected relations: ${unexpected.map(({ name }) => name).join(', ')}`
      : '';
    throw new Error(`refusing unrecognized or unrelated SQLite store${suffix}`);
  }
  return 'legacy';
}

function createPreV1Backup(db: DatabaseSync, dbPath: string): string | null {
  if (dbPath === ':memory:') return null;
  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '');
  const backupPath = `${resolve(dbPath)}.pre-v1-${stamp}-${process.pid}.sqlite3`;
  db.exec(`VACUUM INTO '${backupPath.replace(/'/g, "''")}'`);
  return backupPath;
}

function assertDatabaseIntegrity(db: DatabaseSync): void {
  const integrity = db.prepare('PRAGMA integrity_check').all() as Array<{ integrity_check: string }>;
  const failures = integrity.filter(({ integrity_check }) => integrity_check !== 'ok');
  if (failures.length > 0) {
    throw new Error(`canonical v1 integrity_check failed: ${failures.map((row) => row.integrity_check).join('; ')}`);
  }
  const foreignKeys = db.prepare('PRAGMA foreign_key_check').all();
  if (foreignKeys.length > 0) {
    throw new Error(`canonical v1 foreign_key_check failed with ${foreignKeys.length} row(s)`);
  }
}

function isSqliteBusy(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const sqlite = error as Error & { errcode?: number; errstr?: string };
  return sqlite.errcode === 5 || /database is (?:locked|busy)/i.test(`${sqlite.errstr ?? ''} ${error.message}`);
}

function withSqliteBusyRetry<T>(operation: () => T): T {
  const deadline = Date.now() + SQLITE_BUSY_DEADLINE_MS;
  for (;;) {
    try {
      return operation();
    } catch (error) {
      if (!isSqliteBusy(error) || Date.now() >= deadline) throw error;
      Atomics.wait(SQLITE_WAIT, 0, 0, SQLITE_BUSY_RETRY_MS);
    }
  }
}

/**
 * Checkpoint the WAL so the main DB file absorbs pending pages.
 * Non-fatal on :memory: stores or when a concurrent reader blocks TRUNCATE.
 */
export function checkpointWal(db: DatabaseSync): void {
  try {
    db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  } catch {
    /* non-fatal */
  }
}

/**
 * Return a cached connection for high-frequency in-process harness operations.
 * Keyed by resolved DB path so tests and multiple workspaces stay isolated.
 */
export function connectCachedDb(dbPath: string): DatabaseSync {
  const resolved = resolve(dbPath);
  const cached = _dbCache.get(resolved);
  if (cached) return cached;
  const db = connectDb(resolved);
  _dbCache.set(resolved, db);
  return db;
}

/**
 * Return the cached database connection. Throws if connectDb() has not been
 * called yet in this process (or if the module was imported but the DB was
 * never opened).
 */
export function getDb(): DatabaseSync {
  if (!_db) throw new Error('Database not connected. Call connectDb() first.');
  return _db;
}

export interface DeliveryFingerprintKey {
  consumerId: string;
  channel: string;
  scopeKey: string;
}

/** Read the last payload fingerprint delivered to one consumer/scope. */
export function getDeliveryFingerprint(
  db: DatabaseSync,
  key: DeliveryFingerprintKey,
): string | null {
  const row = db.prepare(`SELECT fingerprint FROM delivery_state
    WHERE consumer_id = ? AND channel = ? AND scope_key = ?`)
    .get(key.consumerId, key.channel, key.scopeKey) as { fingerprint: string } | undefined;
  return row?.fingerprint ?? null;
}

/** Idempotently record the latest delivered payload fingerprint. */
export function setDeliveryFingerprint(
  db: DatabaseSync,
  params: DeliveryFingerprintKey & { fingerprint: string; deliveredAt?: string },
): void {
  db.prepare(`INSERT INTO delivery_state
      (consumer_id, channel, scope_key, fingerprint, delivered_at)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(consumer_id, channel, scope_key) DO UPDATE SET
      fingerprint = excluded.fingerprint,
      delivered_at = excluded.delivered_at`)
    .run(params.consumerId, params.channel, params.scopeKey, params.fingerprint, params.deliveredAt ?? utcNow());
}

// ─── Schema ───────────────────────────────────────────────────────────────────

/**
 * Canonical table DDL. This block is the single source of truth for the
 * schema: fresh stores are created from it directly, and pre-existing stores
 * are migrated against it column-by-column (see migrateExistingTables), so a
 * column added here is automatically backfilled everywhere — never add
 * hand-written ensureColumn calls for new columns.
 *
 * Timestamps: always INSERT explicit second-precision values (helpers.utcNow).
 * The strftime('%f') DEFAULTs below emit millisecond precision, which breaks
 * TEXT-comparison ordering against utcNow values; they cannot be edited to %S
 * because any change to this DDL alters the canonical schema fingerprint and
 * locks out every existing canonical store (needs a user_version bump + a
 * v1→v2 migration ladder that does not exist yet).
 */
const SCHEMA_DDL = `
    CREATE TABLE IF NOT EXISTS sessions (
      session_id     TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      workspace_path TEXT,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      started_at     TEXT NOT NULL,
      ended_at       TEXT,
      summary        TEXT
    );

    CREATE TABLE IF NOT EXISTS memories (
      memory_id             TEXT PRIMARY KEY,
      agent_id              TEXT NOT NULL,
      task_context          TEXT NOT NULL,
      observation           TEXT NOT NULL,
      importance            INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
      state                 TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
      label                 TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by         TEXT,
      tags_json             TEXT NOT NULL DEFAULT '[]',
      workspace_path        TEXT,
      artifact              TEXT,
      repo                  TEXT,
      ref                   TEXT,
      file_tree_fingerprint TEXT,
      novelty_score         REAL,
      last_accessed_at      TEXT,
      access_count          INTEGER NOT NULL DEFAULT 0,
      decay_half_life_days  REAL,
      failure_signature     TEXT,
      valid_from            TEXT,
      valid_to              TEXT,
      expired_at            TEXT,
      embedding             BLOB,
      embedding_model       TEXT,
      created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at            TEXT
    );

    CREATE TABLE IF NOT EXISTS plans (
      plan_id        TEXT PRIMARY KEY,
      name           TEXT NOT NULL,
      objective      TEXT NOT NULL,
      lead_agent_id  TEXT NOT NULL,
      status         TEXT NOT NULL DEFAULT 'DRAFT'
                     CHECK(status IN ('DRAFT','ACTIVE','PAUSED','COMPLETED','CANCELLED')),
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      doc_dir        TEXT NOT NULL,
      created_at     TEXT NOT NULL,
      updated_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS plan_members (
      plan_id    TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
      agent_id   TEXT NOT NULL,
      role       TEXT NOT NULL DEFAULT 'CONTRIBUTOR' CHECK(role IN ('LEAD','CONTRIBUTOR')),
      joined_at  TEXT NOT NULL,
      PRIMARY KEY(plan_id, agent_id)
    );

    CREATE TABLE IF NOT EXISTS plan_docs (
      plan_id       TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
      relative_path TEXT NOT NULL,
      title         TEXT NOT NULL,
      kind          TEXT NOT NULL DEFAULT 'SUPPORTING' CHECK(kind IN ('PRIMARY','SUPPORTING')),
      ordinal       INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY(plan_id, relative_path)
    );

    CREATE TABLE IF NOT EXISTS tasks (
      task_id      TEXT PRIMARY KEY,
      plan_id      TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
      title        TEXT NOT NULL,
      reasoning    TEXT NOT NULL,
      acceptance_criteria TEXT NOT NULL,
      status       TEXT NOT NULL DEFAULT 'OPEN'
                   CHECK(status IN ('OPEN','IN_PROGRESS','BLOCKED','VERIFY','DONE','FAILED','CANCELLED')),
      priority     INTEGER NOT NULL DEFAULT 0,
      created_by   TEXT NOT NULL,
      created_at   TEXT NOT NULL,
      updated_at   TEXT NOT NULL,
      completed_at TEXT
    );

    CREATE TABLE IF NOT EXISTS task_paths (
      task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      path    TEXT NOT NULL,
      ordinal INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY(task_id, path)
    );

    CREATE TABLE IF NOT EXISTS task_dependencies (
      task_id            TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      depends_on_task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      created_by         TEXT NOT NULL,
      created_at         TEXT NOT NULL,
      PRIMARY KEY(task_id, depends_on_task_id),
      CHECK(task_id <> depends_on_task_id)
    );

    CREATE TABLE IF NOT EXISTS task_runs (
      run_id         TEXT PRIMARY KEY,
      task_id        TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
      origin         TEXT NOT NULL DEFAULT 'TASK' CHECK(origin IN ('TASK','WORK','HOOK')),
      agent_id       TEXT NOT NULL,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      rationale      TEXT NOT NULL,
      test_plan      TEXT NOT NULL,
      context_ref    TEXT,
      status         TEXT NOT NULL DEFAULT 'ACTIVE'
                     CHECK(status IN ('PENDING','ACTIVE','SUCCESS','FAILED')),
      workspace_path TEXT,
      artifact       TEXT,
      created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS run_files (
      run_id         TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
      file_path      TEXT NOT NULL,
      reason_override TEXT,
      source         TEXT NOT NULL CHECK(source IN ('EXPLICIT','HOOK')),
      started_at     TEXT NOT NULL,
      heartbeat_at   TEXT NOT NULL,
      expires_at     TEXT NOT NULL,
      ended_at       TEXT,
      PRIMARY KEY(run_id, file_path)
    );

    CREATE TABLE IF NOT EXISTS task_claims (
      task_id      TEXT PRIMARY KEY REFERENCES tasks(task_id) ON DELETE CASCADE,
      run_id       TEXT NOT NULL UNIQUE REFERENCES task_runs(run_id) ON DELETE CASCADE,
      agent_id     TEXT NOT NULL,
      claimed_at   TEXT NOT NULL,
      heartbeat_at TEXT NOT NULL,
      expires_at   TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS task_events (
      event_id   TEXT PRIMARY KEY,
      task_id    TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      run_id     TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
      agent_id   TEXT NOT NULL,
      event_type TEXT NOT NULL,
      message    TEXT NOT NULL,
      created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS locks (
      lock_id     TEXT PRIMARY KEY,
      file_path   TEXT NOT NULL,
      run_id      TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
      acquired_at TEXT NOT NULL,
      expires_at  TEXT,
      UNIQUE(file_path, run_id)
    );

    CREATE TABLE IF NOT EXISTS delivery_state (
      consumer_id TEXT NOT NULL,
      channel     TEXT NOT NULL,
      scope_key   TEXT NOT NULL,
      fingerprint TEXT NOT NULL,
      delivered_at TEXT NOT NULL,
      PRIMARY KEY(consumer_id, channel, scope_key)
    );

    CREATE TABLE IF NOT EXISTS run_log (
      event_id   TEXT PRIMARY KEY,
      run_id     TEXT,
      agent_id   TEXT NOT NULL,
      event_type TEXT NOT NULL,
      message    TEXT NOT NULL,
      created_at TEXT NOT NULL,
      FOREIGN KEY(run_id) REFERENCES task_runs(run_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS refinements (
      refinement_id  TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      files_json     TEXT NOT NULL DEFAULT '[]',
      reasoning      TEXT NOT NULL,
      remember       TEXT NOT NULL,
      quality        TEXT NOT NULL CHECK(quality IN ('good','bad','handoff','instructions')) DEFAULT 'good',
      state          TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
      created_at     TEXT NOT NULL,
      updated_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS signals (
      signal_id      TEXT PRIMARY KEY,
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      from_agent     TEXT NOT NULL,
      to_agent       TEXT,
      kind           TEXT NOT NULL,
      subject        TEXT NOT NULL,
      body           TEXT,
      files_json     TEXT NOT NULL DEFAULT '[]',
      refs_json      TEXT NOT NULL DEFAULT '[]',
      thread_id      TEXT NOT NULL,
      reply_to       TEXT,
      importance     INTEGER NOT NULL DEFAULT 5,
      status         TEXT NOT NULL DEFAULT 'open',
      resolved_at    TEXT,
      created_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS signal_reads (
      signal_id TEXT NOT NULL,
      agent_id  TEXT NOT NULL,
      read_at   TEXT NOT NULL,
      PRIMARY KEY (signal_id, agent_id),
      FOREIGN KEY(signal_id) REFERENCES signals(signal_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS memory_refs (
      memory_id TEXT    NOT NULL,
      reference TEXT    NOT NULL,
      kind      TEXT,
      ordinal   INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (memory_id, reference),
      FOREIGN KEY(memory_id) REFERENCES memories(memory_id) ON DELETE CASCADE
    );

    -- ARCH-5: Agent identity registry — maps opaque agentIds to human-readable names.
    -- Separate from memories so the mapping persists even when memories are pruned.
    -- ON CONFLICT logic in agents.ts ensures a non-empty name is never overwritten by ''.
    CREATE TABLE IF NOT EXISTS agents (
      agent_id       TEXT PRIMARY KEY,
      agent_name     TEXT NOT NULL DEFAULT '',
      workspace_path TEXT,
      artifact       TEXT,
      context        TEXT,   -- 'pi' | 'cursor' | 'claude-code' | etc
      registered_at  TEXT NOT NULL,
      last_seen_at   TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS edit_log (
      edit_id        TEXT PRIMARY KEY,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      run_id         TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
      agent_id       TEXT NOT NULL,
      file_path      TEXT NOT NULL,
      operation      TEXT NOT NULL CHECK(operation IN ('create','update','delete','move','rename')),
      old_file_path  TEXT,          -- populated for move/rename operations
      lines_added    INTEGER,
      lines_removed  INTEGER,
      content_hash   TEXT,          -- sha256 of file content after edit
      workspace_path TEXT,
      artifact       TEXT,
      created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );

    CREATE TABLE IF NOT EXISTS harness_log (
      harness_id   TEXT PRIMARY KEY,
      session_id   TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      agent_id     TEXT NOT NULL,
      workspace_path TEXT,
      artifact     TEXT,
      event_type   TEXT NOT NULL CHECK(event_type IN ('mine','propose','validate','apply','capture','reflect')),
      payload_json TEXT,           -- JSON with event-specific data
      memory_id    TEXT REFERENCES memories(memory_id) ON DELETE SET NULL,
      run_id       TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
      created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
`;

const SCHEMA_INDEX_DDL = `
  CREATE INDEX IF NOT EXISTS idx_sessions_agent     ON sessions(agent_id);
  CREATE INDEX IF NOT EXISTS idx_sessions_workspace ON sessions(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_sessions_scope     ON sessions(workspace_path, artifact);

  CREATE INDEX IF NOT EXISTS idx_memories_importance      ON memories(importance);
  CREATE INDEX IF NOT EXISTS idx_memories_created_at      ON memories(created_at);
  CREATE INDEX IF NOT EXISTS idx_memories_state           ON memories(state);
  CREATE INDEX IF NOT EXISTS idx_memories_label           ON memories(label);
  CREATE INDEX IF NOT EXISTS idx_memories_failure_sig     ON memories(failure_signature);
  CREATE INDEX IF NOT EXISTS idx_memories_workspace_path  ON memories(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_memories_scope           ON memories(workspace_path, repo, ref);
  CREATE INDEX IF NOT EXISTS idx_memories_artifact_scope  ON memories(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_memories_repo_ref        ON memories(repo, ref);
  CREATE INDEX IF NOT EXISTS idx_memories_valid           ON memories(valid_from, valid_to);
  CREATE INDEX IF NOT EXISTS idx_memories_embedding_model ON memories(embedding_model);

  CREATE INDEX IF NOT EXISTS idx_plans_scope          ON plans(workspace_path, artifact, status);
  CREATE INDEX IF NOT EXISTS idx_plans_lead           ON plans(lead_agent_id, status);
  CREATE INDEX IF NOT EXISTS idx_plan_members_agent   ON plan_members(agent_id, plan_id);
  CREATE INDEX IF NOT EXISTS idx_tasks_plan_status    ON tasks(plan_id, status, priority DESC, created_at);
  CREATE INDEX IF NOT EXISTS idx_task_deps_dependency ON task_dependencies(depends_on_task_id);
  CREATE INDEX IF NOT EXISTS idx_task_claims_agent    ON task_claims(agent_id, expires_at);
  CREATE INDEX IF NOT EXISTS idx_task_claims_expiry   ON task_claims(expires_at);
  CREATE INDEX IF NOT EXISTS idx_task_runs_status     ON task_runs(status);
  CREATE INDEX IF NOT EXISTS idx_task_runs_agent      ON task_runs(agent_id, status);
  CREATE INDEX IF NOT EXISTS idx_task_runs_task       ON task_runs(task_id, created_at DESC);
  CREATE INDEX IF NOT EXISTS idx_task_runs_scope      ON task_runs(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_task_events_task     ON task_events(task_id, created_at);

  CREATE INDEX IF NOT EXISTS idx_run_files_path_active ON run_files(file_path, ended_at, expires_at);
  CREATE INDEX IF NOT EXISTS idx_run_files_heartbeat   ON run_files(heartbeat_at);

  CREATE INDEX IF NOT EXISTS idx_locks_file_path   ON locks(file_path);
  CREATE INDEX IF NOT EXISTS idx_locks_acquired_at ON locks(acquired_at);
  CREATE INDEX IF NOT EXISTS idx_locks_expires_at  ON locks(expires_at);

  CREATE INDEX IF NOT EXISTS idx_delivery_state_delivered ON delivery_state(delivered_at);

  CREATE INDEX IF NOT EXISTS idx_refinements_state         ON refinements(state);
  CREATE INDEX IF NOT EXISTS idx_refinements_scope         ON refinements(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_refinements_repo          ON refinements(repo);
  CREATE INDEX IF NOT EXISTS idx_refinements_state_updated ON refinements(state, updated_at DESC);

  CREATE INDEX IF NOT EXISTS idx_signals_status         ON signals(status);
  CREATE INDEX IF NOT EXISTS idx_signals_to_agent       ON signals(to_agent);
  CREATE INDEX IF NOT EXISTS idx_signals_workspace_path ON signals(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_signals_scope          ON signals(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_signals_created_at     ON signals(created_at);
  CREATE INDEX IF NOT EXISTS idx_signals_thread         ON signals(thread_id);

  CREATE INDEX IF NOT EXISTS idx_memory_refs_ref  ON memory_refs(reference);
  CREATE INDEX IF NOT EXISTS idx_memory_refs_kind ON memory_refs(kind);

  CREATE INDEX IF NOT EXISTS idx_agents_workspace ON agents(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_agents_scope     ON agents(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_agents_last_seen ON agents(last_seen_at DESC);

  CREATE INDEX IF NOT EXISTS idx_edit_log_session     ON edit_log(session_id);
  CREATE INDEX IF NOT EXISTS idx_edit_log_run         ON edit_log(run_id);
  CREATE INDEX IF NOT EXISTS idx_edit_log_agent       ON edit_log(agent_id);
  CREATE INDEX IF NOT EXISTS idx_edit_log_file        ON edit_log(file_path);
  CREATE INDEX IF NOT EXISTS idx_edit_log_workspace   ON edit_log(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_edit_log_scope       ON edit_log(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_edit_log_created_at  ON edit_log(created_at);

  CREATE INDEX IF NOT EXISTS idx_harness_log_session    ON harness_log(session_id);
  CREATE INDEX IF NOT EXISTS idx_harness_log_agent      ON harness_log(agent_id);
  CREATE INDEX IF NOT EXISTS idx_harness_log_scope      ON harness_log(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_harness_log_event_type ON harness_log(event_type);
  CREATE INDEX IF NOT EXISTS idx_harness_log_memory     ON harness_log(memory_id);
  CREATE INDEX IF NOT EXISTS idx_harness_log_run        ON harness_log(run_id);
`;

const FTS_SCHEMA_DDL = `
  CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
  USING fts5(memory_id UNINDEXED, task_context, observation, tags)
`;

function tableExists(db: DatabaseSync, table: string): boolean {
  return Boolean(db.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?",
  ).get(table));
}

function renameColumnIfPresent(
  db: DatabaseSync,
  table: string,
  from: string,
  to: string,
): void {
  if (!tableExists(db, table)) return;
  const columns = tableColumns(db, table);
  if (columns.has(from) && !columns.has(to)) {
    db.exec(`ALTER TABLE ${table} RENAME COLUMN ${from} TO ${to}`);
  }
}

/**
 * Schema v1 used `tasks` for short-lived lock/verification attempts. Schema v2
 * reserves `tasks` for durable plan work and gives attempts their honest name:
 * `task_runs`. IDs are preserved so existing hooks and audit history remain
 * connected; migrated standalone runs intentionally have task_id = NULL.
 */
function migrateLegacyTaskRuns(db: DatabaseSync): void {
  if (!tableExists(db, 'tasks')) return;
  const columns = tableColumns(db, 'tasks');
  const isLegacyExecutionTable = columns.has('agent_id') && columns.has('test_plan') && !columns.has('plan_id');
  if (!isLegacyExecutionTable) return;
  if (tableExists(db, 'task_runs')) {
    throw new Error('schema migration cannot move legacy tasks: task_runs already exists');
  }

  for (const index of ['idx_tasks_status', 'idx_tasks_agent_status', 'idx_tasks_workspace', 'idx_tasks_scope']) {
    db.exec(`DROP INDEX IF EXISTS ${index}`);
  }
  db.exec('ALTER TABLE tasks RENAME TO task_runs');
  renameColumnIfPresent(db, 'task_runs', 'task_id', 'run_id');
  renameColumnIfPresent(db, 'task_runs', 'plan_doc_ref', 'context_ref');
  renameColumnIfPresent(db, 'locks', 'task_id', 'run_id');
  if (tableExists(db, 'task_log') && !tableExists(db, 'run_log')) {
    db.exec('ALTER TABLE task_log RENAME TO run_log');
  }
  renameColumnIfPresent(db, 'run_log', 'task_id', 'run_id');
  renameColumnIfPresent(db, 'edit_log', 'task_id', 'run_id');
  renameColumnIfPresent(db, 'harness_log', 'task_id', 'run_id');
}

function expectCopied(changes: number | bigint, expected: number, relation: string): void {
  if (Number(changes) !== expected) {
    throw new Error(`schema migration copied ${String(changes)}/${expected} rows from ${relation}`);
  }
}

function legacySqlValue(value: unknown, field: string): SQLInputValue {
  if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'bigint') {
    return value;
  }
  if (value instanceof Uint8Array) return value;
  throw new Error(`schema migration cannot bind legacy field ${field}`);
}

function legacySqlValues(values: Array<[unknown, string]>): SQLInputValue[] {
  return values.map(([value, field]) => legacySqlValue(value, field));
}

/** Import the pre-canonical relation names before removing the legacy island. */
function migrateLegacyV0Relations(db: DatabaseSync): void {
  if (tableExists(db, 'agent_memories')) {
    const rows = db.prepare('SELECT * FROM agent_memories').all() as Array<Record<string, unknown>>;
    const insert = db.prepare(`INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance, state, label,
      superseded_by, tags_json, workspace_path, artifact, repo, ref,
      file_tree_fingerprint, novelty_score, last_accessed_at, access_count,
      decay_half_life_days, failure_signature, valid_from, valid_to, expired_at,
      embedding, embedding_model, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`);
    const insertRef = db.prepare(`INSERT INTO memory_refs(memory_id, reference, kind, ordinal)
      VALUES (?, ?, 'file', 0)`);
    for (const row of rows) {
      insert.run(...legacySqlValues([
        [row['memory_id'], 'agent_memories.memory_id'],
        [row['agent_id'], 'agent_memories.agent_id'],
        [row['task_context'], 'agent_memories.task_context'],
        [row['observation'], 'agent_memories.observation'],
        [row['importance_score'], 'agent_memories.importance_score'],
        [row['state'] ?? 'ACTIVE', 'agent_memories.state'],
        [row['label'] ?? 'OTHER', 'agent_memories.label'],
        [row['superseded_by'] ?? null, 'agent_memories.superseded_by'],
        [row['tags_json'] ?? '[]', 'agent_memories.tags_json'],
        [row['file_tree_fingerprint'] ?? null, 'agent_memories.file_tree_fingerprint'],
        [row['last_accessed_at'] ?? null, 'agent_memories.last_accessed_at'],
        [row['access_count'] ?? 0, 'agent_memories.access_count'],
        [row['decay_half_life_days'] ?? null, 'agent_memories.decay_half_life_days'],
        [row['failure_signature'] ?? null, 'agent_memories.failure_signature'],
        [row['valid_from'] ?? row['created_at'], 'agent_memories.valid_from'],
        [row['valid_to'] ?? null, 'agent_memories.valid_to'],
        [row['expired_at'] ?? null, 'agent_memories.expired_at'],
        [row['embedding'] ?? null, 'agent_memories.embedding'],
        [row['embedding_model'] ?? null, 'agent_memories.embedding_model'],
        [row['created_at'], 'agent_memories.created_at'],
        [row['updated_at'] ?? null, 'agent_memories.updated_at'],
      ]));
      if (typeof row['file'] === 'string' && row['file'].trim()) {
        insertRef.run(
          legacySqlValue(row['memory_id'], 'agent_memories.memory_id'),
          `file:${row['file']}`,
        );
      }
    }
  }

  if (tableExists(db, 'agent_intents')) {
    const rows = db.prepare('SELECT * FROM agent_intents').all() as Array<Record<string, unknown>>;
    const insertRun = db.prepare(`INSERT INTO task_runs (
      run_id, task_id, origin, agent_id, session_id, rationale, test_plan,
      context_ref, status, workspace_path, artifact, created_at, updated_at
    ) VALUES (?, NULL, 'WORK', ?, NULL, ?, ?, ?, ?, ?, NULL, ?, ?)`);
    const insertFile = db.prepare(`INSERT INTO run_files (
      run_id, file_path, reason_override, source, started_at, heartbeat_at,
      expires_at, ended_at
    ) VALUES (?, ?, NULL, 'EXPLICIT', ?, ?, ?, ?)`);
    for (const row of rows) {
      insertRun.run(...legacySqlValues([
        [row['intent_id'], 'agent_intents.intent_id'],
        [row['agent_id'], 'agent_intents.agent_id'],
        [row['rationale'], 'agent_intents.rationale'],
        [row['test_plan'], 'agent_intents.test_plan'],
        [row['plan_doc_ref'] ?? null, 'agent_intents.plan_doc_ref'],
        [row['status'], 'agent_intents.status'],
        [row['workspace_path'] ?? null, 'agent_intents.workspace_path'],
        [row['created_at'], 'agent_intents.created_at'],
        [row['updated_at'], 'agent_intents.updated_at'],
      ]));
      for (const filePath of parseJsonList(String(row['files_json'] ?? '[]'))) {
        const updatedAt = String(row['updated_at']);
        insertFile.run(...legacySqlValues([
          [row['intent_id'], 'agent_intents.intent_id'],
          [filePath, 'agent_intents.files_json[]'],
          [row['created_at'], 'agent_intents.created_at'],
          [updatedAt, 'agent_intents.updated_at'],
          [updatedAt, 'agent_intents.updated_at'],
          [row['status'] === 'ACTIVE' ? null : updatedAt, 'agent_intents.ended_at'],
        ]));
      }
    }
  }

  if (tableExists(db, 'file_locks')) {
    const expected = (db.prepare('SELECT COUNT(*) AS count FROM file_locks').get() as { count: number }).count;
    const result = db.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
      SELECT f.lock_id, f.file_path, f.intent_id, f.acquired_at, f.expires_at
      FROM file_locks f JOIN task_runs r ON r.run_id = f.intent_id`).run();
    expectCopied(result.changes, expected, 'file_locks');
  }

  if (tableExists(db, 'intent_events')) {
    const expected = (db.prepare('SELECT COUNT(*) AS count FROM intent_events').get() as { count: number }).count;
    const result = db.prepare(`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
      SELECT e.event_id, e.intent_id, e.agent_id, e.event_type, e.message, e.created_at
      FROM intent_events e LEFT JOIN task_runs r ON r.run_id = e.intent_id`).run();
    expectCopied(result.changes, expected, 'intent_events');
  }

  if (tableExists(db, 'task_log')) {
    const columns = tableColumns(db, 'task_log');
    const runColumn = columns.has('run_id') ? 'run_id' : 'task_id';
    const expected = (db.prepare('SELECT COUNT(*) AS count FROM task_log').get() as { count: number }).count;
    const result = db.prepare(`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
      SELECT event_id, ${runColumn}, agent_id, event_type, message, created_at FROM task_log`).run();
    expectCopied(result.changes, expected, 'task_log');
  }

  if (tableExists(db, 'notifications')) {
    const expected = (db.prepare('SELECT COUNT(*) AS count FROM notifications').get() as { count: number }).count;
    const result = db.prepare(`INSERT INTO signals (
      signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind,
      subject, body, files_json, refs_json, thread_id, reply_to, importance,
      status, resolved_at, created_at
    ) SELECT notification_id, workspace_path, NULL, repo, ref, from_agent, to_agent,
      kind, subject, body, files_json, refs_json, thread_id, in_reply_to,
      importance, status, CASE WHEN status = 'resolved' THEN created_at ELSE NULL END,
      created_at FROM notifications`).run();
    expectCopied(result.changes, expected, 'notifications');
  }

  if (tableExists(db, 'notification_reads')) {
    const expected = (db.prepare('SELECT COUNT(*) AS count FROM notification_reads').get() as { count: number }).count;
    const result = db.prepare(`INSERT INTO signal_reads(signal_id, agent_id, read_at)
      SELECT r.notification_id, r.agent_id, r.read_at
      FROM notification_reads r JOIN signals s ON s.signal_id = r.notification_id`).run();
    expectCopied(result.changes, expected, 'notification_reads');
  }

  for (const relation of [
    'notification_reads', 'notifications', 'file_locks', 'intent_events',
    'agent_intents', 'agent_memories', 'task_log', 'memory_fts',
  ]) {
    db.exec(`DROP TABLE IF EXISTS ${relation}`);
  }
}

function repairLegacyForeignKeyReferences(db: DatabaseSync): void {
  // Legacy execution rows could carry a host session ID without a sessions row.
  // Preserve that identifier by synthesizing the smallest honest session record.
  db.exec(`INSERT OR IGNORE INTO sessions (
    session_id, agent_id, workspace_path, artifact, repo, ref,
    started_at, ended_at, summary
  ) SELECT session_id, agent_id, workspace_path, artifact, NULL, NULL,
      created_at, CASE WHEN status = 'ACTIVE' THEN NULL ELSE updated_at END,
      'migrated legacy session reference'
    FROM task_runs
    WHERE session_id IS NOT NULL
    ORDER BY created_at, run_id`);

  // A missing durable task cannot be reconstructed honestly. Preserve its ID as
  // context on the run before using the canonical nullable historical FK.
  db.exec(`UPDATE task_runs
    SET context_ref = COALESCE(context_ref, 'legacy-task:' || task_id), task_id = NULL
    WHERE task_id IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM tasks WHERE tasks.task_id = task_runs.task_id
    )`);
}

export function initDb(db: DatabaseSync): void {
  initializeDb(db, false);
}

function mainDatabasePath(db: DatabaseSync): string | null {
  const row = (db.prepare('PRAGMA database_list').all() as Array<{ name: string; file: string }>)
    .find(({ name }) => name === 'main');
  return row?.file?.trim() || null;
}

function initializeDb(db: DatabaseSync, fileBackupCreated: boolean, knownState?: SchemaState): void {
  // connectDb passes the state it already classified; re-inspecting here would
  // repeat the full fingerprint read+hash on every open. Callers without a
  // prior classification (initDb) still inspect.
  const state = knownState ?? inspectSchemaState(db);
  if (state === 'canonical') {
    if (!db.isTransaction) db.exec('PRAGMA foreign_keys = ON');
    return;
  }
  if (db.isTransaction) {
    throw new Error('cannot initialize or migrate canonical v1 inside a caller-owned transaction');
  }
  if (state === 'legacy' && mainDatabasePath(db) && !fileBackupCreated) {
    throw new Error('file-backed legacy migration requires connectDb(path) so a pre-v1 backup is created');
  }

  // Serialize the complete detect → migrate → index → version sequence. A
  // transaction around individual ALTERs is insufficient: two first openers
  // can both observe a missing column and race the same DDL.
  db.exec('PRAGMA foreign_keys = OFF');
  let began = false;
  try {
    withSqliteBusyRetry(() => db.exec('BEGIN IMMEDIATE'));
    began = true;
    // A concurrent opener may have completed the migration while this
    // connection waited for the write lock. Reclassify under the lock.
    const lockedState = inspectSchemaState(db);
    if (lockedState !== 'canonical') initDbSchema(db, lockedState);
    db.exec('COMMIT');
    began = false;
  } catch (error) {
    if (began) {
      try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
    }
    throw error;
  } finally {
    db.exec('PRAGMA foreign_keys = ON');
  }
}

function initDbSchema(db: DatabaseSync, state: Exclude<SchemaState, 'canonical'>): void {
  migrateLegacyTaskRuns(db);

  // ── 1. All regular tables in a single exec block ───────────────────────────
  db.exec(SCHEMA_DDL);

  // Bring pre-existing stores up to the canonical schema BEFORE any index is
  // created — indexes below reference columns (failure_signature, valid_from,
  // embedding_model, …) that old stores may lack.
  migrateExistingTables(db);
  migrateLegacyExecutionSchema(db);
  migrateRefinementQualityConstraint(db);
  migrateCheckConstraints(db);
  migrateLegacyV0Relations(db);
  repairLegacyForeignKeyReferences(db);

  if (state === 'legacy') {
    // FTS is derived and may refer to a pre-canonical memory table. Rebuild all
    // ordinary tables from the one DDL so PK/FK/UNIQUE/CHECK drift cannot survive.
    db.exec('DROP TABLE IF EXISTS memories_fts');
    rebuildAllCanonicalTables(db);
    normalizeImportedTimestamps(db);
  }

  // ── 2. All indexes in a single canonical block ──────────────────────────────
  db.exec(SCHEMA_INDEX_DDL);

  // ── 3. FTS5 virtual table (isolated try/catch — fts5 may be unavailable) ──
  try {
    db.exec(FTS_SCHEMA_DDL);
  } catch {
    /* fts5 unavailable or already exists */
  }

  // ── 4. Seed FTS if the index is empty (fresh store or cleared) ─────────────
  if (hasFts(db)) {
    const row = db.prepare('SELECT COUNT(*) AS cnt FROM memories_fts').get() as { cnt: number };
    if (row.cnt === 0) rebuildFts(db);
  }

  assertCanonicalRelationContract(db);
  assertCanonicalSchemaFingerprint(db);
  assertDatabaseIntegrity(db);
  db.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
  db.exec(`PRAGMA user_version = ${AWARENESS_SCHEMA_VERSION}`);
}

// ─── Table introspection ──────────────────────────────────────────────────────

export function tableColumns(db: DatabaseSync, tableName: string): Set<string> {
  const rows = db.prepare(`PRAGMA table_info(${tableName})`).all() as unknown as TableInfoRow[];
  return new Set(rows.map(r => r.name));
}

interface ColumnInfo {
  name: string;
  type: string;
  notnull: number;
  dflt_value: string | null;
}

let _canonicalColumns: Map<string, ColumnInfo[]> | undefined;

/**
 * Desired columns per table, derived by instantiating SCHEMA_DDL in a
 * throwaway in-memory database and introspecting it. Computed once per
 * process.
 */
function canonicalColumns(): Map<string, ColumnInfo[]> {
  if (_canonicalColumns) return _canonicalColumns;
  const tmp = new DatabaseSync(':memory:');
  try {
    tmp.exec(SCHEMA_DDL);
    const tables = tmp.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    ).all() as unknown as Array<{ name: string }>;
    const map = new Map<string, ColumnInfo[]>();
    for (const { name } of tables) {
      map.set(name, tmp.prepare(`PRAGMA table_info(${name})`).all() as unknown as ColumnInfo[]);
    }
    _canonicalColumns = map;
    return map;
  } finally {
    tmp.close();
  }
}

/** A DEFAULT is only usable in ALTER TABLE ADD COLUMN if it is a constant. */
function isConstantDefault(dflt: string | null): dflt is string {
  return dflt !== null && !dflt.includes('(');
}

/**
 * Add every canonical column missing from a pre-existing store. Constant
 * defaults (and their NOT NULL) are preserved so old rows behave like fresh
 * ones; non-constant defaults (strftime) are added as plain nullable columns
 * since SQLite forbids them in ADD COLUMN.
 */
function migrateExistingTables(db: DatabaseSync): void {
  for (const [table, columns] of canonicalColumns()) {
    const existing = tableColumns(db, table);
    for (const col of columns) {
      if (existing.has(col.name)) continue;
      let clause = `${col.name} ${col.type}`;
      if (isConstantDefault(col.dflt_value)) {
        if (col.notnull) clause += ' NOT NULL';
        clause += ` DEFAULT ${col.dflt_value}`;
      }
      db.exec(`ALTER TABLE ${table} ADD COLUMN ${clause}`);
    }
  }
}

/**
 * The last unbranded runtime generation normalized advisory file presence and
 * made every lock exclusive. Preserve that import step on the way to v1.
 * Backfill file rows before rebuilding task_runs/locks so no v2 files_json or
 * lock identity is lost. Historical standalone runs are classified as HOOK:
 * v2 could not distinguish explicit lock-only work from hook-created runs, and HOOK
 * is the conservative lifecycle (verification debt remains visible).
 */
function migrateLegacyExecutionSchema(db: DatabaseSync): void {
  if (!tableExists(db, 'task_runs')) return;
  const runColumns = tableColumns(db, 'task_runs');
  const lockColumns = tableExists(db, 'locks') ? tableColumns(db, 'locks') : new Set<string>();
  const needsRunRebuild = runColumns.has('files_json');
  const needsLockRebuild = ['agent_id', 'session_id', 'lock_type'].some((name) => lockColumns.has(name));
  if (!needsRunRebuild && !needsLockRebuild) return;

  if (needsRunRebuild) {
    const rows = db.prepare(`SELECT run_id, task_id, status, files_json, created_at, updated_at
      FROM task_runs`).all() as unknown as Array<{
        run_id: string;
        task_id: string | null;
        status: string;
        files_json: string;
        created_at: string | null;
        updated_at: string | null;
      }>;
    const insert = db.prepare(`INSERT OR IGNORE INTO run_files
      (run_id, file_path, reason_override, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES (?, ?, NULL, ?, ?, ?, ?, ?)`);
    const now = utcNow();
    for (const row of rows) {
      const source = row.task_id == null ? 'HOOK' : 'EXPLICIT';
      const startedAt = row.created_at ?? now;
      const heartbeatAt = row.updated_at ?? startedAt;
      for (const filePath of parseJsonList(row.files_json)) {
        const lease = db.prepare(`SELECT MAX(expires_at) AS expires_at FROM (
          SELECT expires_at FROM locks WHERE run_id = ? AND file_path = ?
          UNION ALL
          SELECT expires_at FROM task_claims WHERE run_id = ?
        )`).get(row.run_id, filePath, row.run_id) as { expires_at: string | null };
        const expiresAt = lease.expires_at ?? heartbeatAt;
        const active = row.status === 'ACTIVE' && expiresAt > now;
        insert.run(row.run_id, filePath, source, startedAt, heartbeatAt, expiresAt, active ? null : heartbeatAt);
      }
    }
    db.prepare(`UPDATE task_runs SET origin = CASE
      WHEN task_id IS NOT NULL THEN 'TASK' ELSE 'HOOK' END`).run();
  }

  if (needsRunRebuild) {
    const sql = canonicalTableSql().get('task_runs');
    if (!sql) throw new Error('schema migration cannot find canonical task_runs DDL');
    rebuildTableFromCanonical(db, 'task_runs', sql);
  }
  if (needsLockRebuild) {
    const sql = canonicalTableSql().get('locks');
    if (!sql) throw new Error('schema migration cannot find canonical locks DDL');
    rebuildTableFromCanonical(db, 'locks', sql);
  }
}

let _canonicalTableSql: Map<string, string> | undefined;

/**
 * Canonical `CREATE TABLE` text per table, captured by instantiating SCHEMA_DDL
 * in a throwaway DB. Used to detect and repair CHECK-constraint drift on old
 * stores. Computed once per process.
 */
function canonicalTableSql(): Map<string, string> {
  if (_canonicalTableSql) return _canonicalTableSql;
  const tmp = new DatabaseSync(':memory:');
  try {
    tmp.exec(SCHEMA_DDL);
    const rows = tmp.prepare(
      "SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL"
    ).all() as unknown as Array<{ name: string; sql: string }>;
    _canonicalTableSql = new Map(rows.map((r) => [r.name, r.sql]));
    return _canonicalTableSql;
  } finally {
    tmp.close();
  }
}

function normalizeSchemaSql(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, ' ')
    .replace(/["`\[\]]/g, '')
    .replace(/\bIF\s+NOT\s+EXISTS\b/gi, '')
    .replace(/\s+/g, ' ')
    .replace(/\s*([(),])\s*/g, '$1')
    .trim()
    .toLowerCase();
}

interface SchemaObject {
  type: string;
  name: string;
  tableName: string;
  sql: string;
}

function readSchemaObjects(db: DatabaseSync): SchemaObject[] {
  const rows = db.prepare(`
    SELECT type, name, tbl_name, sql
    FROM sqlite_schema
    WHERE type IN ('table', 'view', 'index', 'trigger')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
      AND name NOT GLOB 'memory_fts_*'
    ORDER BY type, name
  `).all() as Array<{ type: string; name: string; tbl_name: string; sql: string | null }>;
  return rows.map((row) => ({
    type: row.type,
    name: row.name,
    tableName: row.tbl_name,
    sql: normalizeSchemaSql(row.sql ?? ''),
  }));
}

function schemaObjectsFingerprint(objects: SchemaObject[]): string {
  return createHash('sha256').update(JSON.stringify(objects)).digest('hex');
}

const _canonicalSchemaFingerprints = new Map<boolean, string>();

function canonicalSchemaFingerprint(includeFts: boolean): string {
  const cached = _canonicalSchemaFingerprints.get(includeFts);
  if (cached) return cached;
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    if (includeFts) canonical.exec(FTS_SCHEMA_DDL);
    const fingerprint = schemaObjectsFingerprint(readSchemaObjects(canonical));
    _canonicalSchemaFingerprints.set(includeFts, fingerprint);
    return fingerprint;
  } finally {
    canonical.close();
  }
}

function assertCanonicalRelationContract(
  db: DatabaseSync,
  relations?: SchemaIdentity['relations'],
): void {
  const actualRows = relations ?? readSchemaIdentity(db).relations;
  const expected = new Set(canonicalColumns().keys());
  const actual = new Set(actualRows.map(({ name }) => name));
  const missing = [...expected].filter((name) => !actual.has(name));
  const unexpected = actualRows.filter(({ name, type }) => (
    type !== 'table' || (!expected.has(name) && name !== 'memories_fts')
  ));
  if (missing.length === 0 && unexpected.length === 0) return;
  const details = [
    missing.length > 0 ? `missing: ${missing.join(', ')}` : null,
    unexpected.length > 0 ? `unexpected: ${unexpected.map(({ name }) => name).join(', ')}` : null,
  ].filter((value): value is string => value !== null).join('; ');
  throw new Error(`canonical v1 relation contract mismatch (${details})`);
}

function assertCanonicalSchemaFingerprint(db: DatabaseSync): void {
  const objects = readSchemaObjects(db);
  const includeFts = objects.some(({ type, name }) => type === 'table' && name === 'memories_fts');
  const expectedFingerprint = canonicalSchemaFingerprint(includeFts);
  const actualFingerprint = schemaObjectsFingerprint(objects);
  if (actualFingerprint !== expectedFingerprint) {
    throw new Error(
      `canonical v1 schema fingerprint mismatch (expected ${expectedFingerprint}, got ${actualFingerprint})`,
    );
  }
}

/**
 * Normalized, order-insensitive fingerprint of a table's CHECK clauses.
 * Extraction is paren-balanced (and skips 'string' literals), so a CHECK with
 * nested parens — CHECK(a > 0 AND (b < 10)) — is captured whole; a first-')'
 * regex would truncate it and silently mis-compare drift.
 */
function checkClauses(createSql: string): string {
  const clauses: string[] = [];
  const opener = /CHECK\s*\(/gi;
  let match: RegExpExecArray | null;
  while ((match = opener.exec(createSql)) !== null) {
    let depth = 1;
    let i = opener.lastIndex;
    while (i < createSql.length && depth > 0) {
      const ch = createSql[i];
      if (ch === "'") {
        i = createSql.indexOf("'", i + 1);
        if (i === -1) { i = createSql.length; break; }
      } else if (ch === '(') depth++;
      else if (ch === ')') depth--;
      i++;
    }
    clauses.push(createSql.slice(match.index, i));
    opener.lastIndex = i;
  }
  return clauses.map((c) => c.replace(/\s+/g, ' ').trim().toLowerCase()).sort().join(' | ');
}

/**
 * Rebuild one table from its canonical DDL, copying the intersection of old and
 * canonical columns. Indexes are intentionally not recreated here — initDb's
 * `CREATE INDEX IF NOT EXISTS` block runs immediately after migrations and
 * restores them. Wrapped in a SAVEPOINT so a failure leaves the old table intact.
 */
function rebuildTableFromCanonical(db: DatabaseSync, table: string, canonSql: string): void {
  const liveCols = tableColumns(db, table);
  const canonCols = (canonicalColumns().get(table) ?? []).map((c) => c.name).filter((n) => liveCols.has(n));
  if (canonCols.length === 0) return;
  const colList = canonCols.join(', ');
  const tmpName = `${table}__ckmig`;
  const createTmp = canonSql.replace(
    new RegExp(`(CREATE\\s+TABLE\\s+(?:IF\\s+NOT\\s+EXISTS\\s+)?)"?${table}"?`, 'i'),
    `$1${tmpName}`,
  );
  if (!createTmp.includes(tmpName)) {
    throw new Error(`check-constraint migration: cannot rename table ${table} in canonical DDL`);
  }
  const savepoint = `migrate_check_${table}`;
  db.exec(`SAVEPOINT ${savepoint}`);
  try {
    db.exec(`DROP TABLE IF EXISTS ${tmpName};`);
    db.exec(createTmp);
    db.exec(`INSERT INTO ${tmpName} (${colList}) SELECT ${colList} FROM ${table};`);
    db.exec(`DROP TABLE ${table};`);
    db.exec(`ALTER TABLE ${tmpName} RENAME TO ${table};`);
    db.exec(`RELEASE SAVEPOINT ${savepoint}`);
  } catch (err) {
    try { db.exec(`ROLLBACK TO SAVEPOINT ${savepoint}`); } catch { /* already rolled back */ }
    try { db.exec(`RELEASE SAVEPOINT ${savepoint}`); } catch { /* already released */ }
    const reason = err instanceof Error ? err.message : String(err);
    if (/CHECK constraint failed/i.test(reason)) {
      // Row data the new CHECK forbids (an enum narrowing/rename) — name the
      // table so the store can be repaired instead of just failing to open.
      throw new Error(
        `schema migration cannot rebuild table "${table}": existing rows violate a canonical CHECK constraint (${reason}); ` +
        'inspect that table\'s enum columns and update the offending rows before reopening',
      );
    }
    throw err;
  }
}

function rebuildAllCanonicalTables(db: DatabaseSync): void {
  for (const [table, sql] of canonicalTableSql()) {
    if (tableExists(db, table)) rebuildTableFromCanonical(db, table, sql);
  }
}

/**
 * Second-precision normalization for rows imported from pre-v1 stores. Legacy
 * generations stored millisecond ISO strings while every canonical write is
 * second-precision (helpers.utcNow); TEXT timestamps compare as strings, so
 * '…:10.350Z' sorts before '…:10Z' and an imported lease can be judged expired
 * early. Timestamp columns are derived from the canonical DDL, not hand-listed.
 */
function normalizeImportedTimestamps(db: DatabaseSync): void {
  for (const [table, columns] of canonicalColumns()) {
    if (!tableExists(db, table)) continue;
    for (const col of columns) {
      if (col.type.toUpperCase() !== 'TEXT') continue;
      if (!/(_at|_from|_to)$/.test(col.name)) continue;
      db.prepare(
        `UPDATE ${table} SET ${col.name} = substr(${col.name}, 1, 19) || 'Z'
         WHERE ${col.name} LIKE '____-__-__T__:__:__.%'`,
      ).run();
    }
  }
}

/**
 * Generic CHECK-constraint drift repair. migrateExistingTables only ADDs
 * columns — it cannot evolve a CHECK on a pre-existing table, so an old store
 * whose enum is narrower than the current DDL (e.g. harness_log.event_type
 * lacking 'reflect', tasks.status, locks.lock_type, memories.state) throws
 * "CHECK constraint failed" on any insert using a newer value. This detects
 * such drift against the canonical DDL and rebuilds only the drifted tables.
 * A current/fresh store matches canonical exactly, so nothing is rebuilt.
 */
function migrateCheckConstraints(db: DatabaseSync): void {
  const drifted: Array<[string, string]> = [];
  for (const [table, canonSql] of canonicalTableSql()) {
    const live = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name=?"
    ).get(table) as { sql: string } | undefined;
    if (!live?.sql) continue;
    if (checkClauses(live.sql) !== checkClauses(canonSql)) drifted.push([table, canonSql]);
  }
  if (drifted.length === 0) return;
  // initDb disables FK enforcement before its serialized transaction because
  // rebuilds transiently drop tables referenced by other canonical tables.
  for (const [table, canonSql] of drifted) rebuildTableFromCanonical(db, table, canonSql);
}

function migrateRefinementQualityConstraint(db: DatabaseSync): void {
  const row = db.prepare(
    "SELECT sql FROM sqlite_master WHERE type='table' AND name='refinements'"
  ).get() as { sql: string } | undefined;
  if (!row?.sql || row.sql.includes("'instructions'")) return;

  db.exec('SAVEPOINT migrate_refinement_quality_constraint');
  try {
    db.exec(`
      DROP TABLE IF EXISTS refinements_migration_new;
      CREATE TABLE refinements_migration_new (
        refinement_id  TEXT PRIMARY KEY,
        agent_id       TEXT NOT NULL,
        workspace_path TEXT NOT NULL,
        artifact       TEXT,
        repo           TEXT,
        ref            TEXT,
        files_json     TEXT NOT NULL DEFAULT '[]',
        reasoning      TEXT NOT NULL,
        remember       TEXT NOT NULL,
        quality        TEXT NOT NULL CHECK(quality IN ('good','bad','handoff','instructions')) DEFAULT 'good',
        state          TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
        created_at     TEXT NOT NULL,
        updated_at     TEXT NOT NULL
      );
      INSERT INTO refinements_migration_new (
        refinement_id, agent_id, workspace_path, artifact, repo, ref,
        files_json, reasoning, remember, quality, state, created_at, updated_at
      )
      SELECT
        refinement_id, agent_id, workspace_path, artifact, repo, ref,
        files_json, reasoning, remember, quality, state, created_at, updated_at
      FROM refinements;
      DROP TABLE refinements;
      ALTER TABLE refinements_migration_new RENAME TO refinements;
    `);
    db.exec('RELEASE SAVEPOINT migrate_refinement_quality_constraint');
  } catch (err) {
    try { db.exec('ROLLBACK TO SAVEPOINT migrate_refinement_quality_constraint'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT migrate_refinement_quality_constraint'); } catch { /* already released */ }
    throw err;
  }
}

// ─── FTS helpers ──────────────────────────────────────────────────────────────

export function hasFts(db: DatabaseSync): boolean {
  const row = db.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories_fts'"
  ).get() as Record<string, number> | undefined;
  return Boolean(row);
}

type FtsTermRow = Partial<MemoryRow> & { references?: string[] };

/**
 * Build the FTS5 `tags` column value for a memory row.
 *
 * Index semantic classifiers plus explicit provenance references. Workspace,
 * repo, git ref, and failure_signature remain structural filters so broad path
 * or repo names do not dominate natural-language ranking.
 */
export function ftsTermsForRow(row: FtsTermRow): string {
  const tags = parseJsonList(row.tags_json);
  const label = (row.label ?? 'OTHER').toLowerCase();
  return [...tags, label, ...(row.references ?? [])].filter(Boolean).join(' ');
}

export function rebuildFts(db: DatabaseSync): void {
  // DB-4 reverted: 'delete-all' FTS5 command only works on content= (contentless)
  // tables, not regular FTS5 tables. DELETE FROM is the correct approach for
  // a standard fts5 table (it goes through the shadow tables properly).
  db.exec('SAVEPOINT rebuild_fts');
  try {
    db.exec('DELETE FROM memories_fts');
    // Select only the columns needed for FTS indexing — avoids loading the
    // embedding BLOB (can be 1536 floats = 6KB per row) for all rows.
    const rows = db.prepare(
      'SELECT memory_id, task_context, observation, tags_json, label FROM memories'
    ).all() as unknown as Array<Pick<MemoryRow, 'memory_id' | 'task_context' | 'observation' | 'tags_json' | 'label'> & { references?: string[] }>;
    if (rows.length > 0) {
      const refs = db.prepare(
        `SELECT r.memory_id, r.reference
         FROM memory_refs r
         JOIN memories m ON m.memory_id = r.memory_id
         ORDER BY r.memory_id, r.ordinal`
      ).all() as unknown as Array<{ memory_id: string; reference: string }>;
      const refsByMemory = new Map<string, string[]>();
      for (const ref of refs) {
        const list = refsByMemory.get(ref.memory_id) ?? [];
        list.push(ref.reference);
        refsByMemory.set(ref.memory_id, list);
      }
      for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
    }
    const insert = db.prepare(
      'INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
    );
    for (const row of rows) {
      insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
    }
    db.exec('RELEASE SAVEPOINT rebuild_fts');
  } catch (e) {
    try { db.exec('ROLLBACK TO SAVEPOINT rebuild_fts'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT rebuild_fts'); } catch { /* already released */ }
    throw e;
  }
}

// ─── Memory references ────────────────────────────────────────────────────────

export function referenceKind(reference: string): string {
  if (/^https?:\/\//.test(reference)) return 'url';
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.\-]*):/);
  return m ? m[1]!.toLowerCase() : 'other';
}

export function replaceMemoryReferences(db: DatabaseSync, memoryId: string, references: string[]): void {
  db.prepare('DELETE FROM memory_refs WHERE memory_id = ?').run(memoryId);
  const insert = db.prepare(
    'INSERT OR REPLACE INTO memory_refs(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)'
  );
  references.forEach((ref, i) => insert.run(memoryId, ref, referenceKind(ref), i));
}

// ─── Lock maintenance ─────────────────────────────────────────────────────────

/**
 * Evict expired exclusive locks without changing run lifecycle. Advisory
 * presence is independent: WORK/HOOK ends explicitly and TASK ends through
 * task submit/release.
 */
export interface EvictExpiredLocksResult {
  pruned_locks: number;
}

export function evictExpiredLocks(db: DatabaseSync): EvictExpiredLocksResult {
  const now = utcNow();
  const stale = db.prepare(
    'SELECT COUNT(*) AS c FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?'
  ).get(now) as { c: number };
  if (stale.c === 0) return { pruned_locks: 0 };

  db.exec('SAVEPOINT evict_expired_locks');
  try {
    const deleteRes = db.prepare(
      'DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?'
    ).run(now) as { changes: number };
    db.exec('RELEASE SAVEPOINT evict_expired_locks');
    return { pruned_locks: deleteRes.changes };
  } catch (e) {
    try { db.exec('ROLLBACK TO SAVEPOINT evict_expired_locks'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT evict_expired_locks'); } catch { /* already released */ }
    throw e;
  }
}
