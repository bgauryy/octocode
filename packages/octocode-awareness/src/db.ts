/**
 * db.ts — SQLite connection, schema init, and utility helpers.
 * Requires Node >=22.13.0 (unflagged node:sqlite built-in).
 *
 * Clean schema scope:
 *   workspace_path is the primary isolation key.
 *   artifact is the optional workspace-local package/service/component slice.
 */

import { DatabaseSync } from 'node:sqlite';
import { mkdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { homedir, platform } from 'node:os';

import { parseJsonList, utcNow } from './helpers.js';
import type { TableInfoRow, MemoryRow } from './types.js';
import { journalModeForSqliteVersion } from './v4/runtime.js';

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_DB_NAME = 'awareness.sqlite3';
const MEMORY_HOME_ENV = 'OCTOCODE_MEMORY_HOME';
const V3_SCHEMA_VERSION = 3;
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
    // touch a foreign/future store. This is the compatibility firewall that
    // prevents an old v3 CLI/hook from shape-migrating a v4 database back to v3.
    assertV3SchemaIdentity(db);
    const versionRow = db.prepare('SELECT sqlite_version() AS version').get() as { version: string };
    const journalMode = journalModeForSqliteVersion(versionRow.version);
    // Unsafe embedded SQLite versions use rollback journaling instead of the
    // documented concurrent-WAL path. Changing mode is a write and may race a
    // first opener, so both modes use the same bounded BUSY retry.
    withSqliteBusyRetry(() => db.exec(`PRAGMA journal_mode = ${journalMode}`));
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    _db = db;
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

interface V3SchemaIdentity {
  applicationId: number;
  userVersion: number;
}

function readV3SchemaIdentity(db: DatabaseSync): V3SchemaIdentity {
  const application = db.prepare('PRAGMA application_id').get() as { application_id: number };
  const version = db.prepare('PRAGMA user_version').get() as { user_version: number };
  return {
    applicationId: application.application_id ?? 0,
    userVersion: version.user_version ?? 0,
  };
}

function assertV3SchemaIdentity(db: DatabaseSync): void {
  const identity = readV3SchemaIdentity(db);
  if (identity.applicationId !== 0) {
    throw new Error(
      `refusing foreign Awareness application_id ${identity.applicationId}; v3 expects 0`,
    );
  }
  if (identity.userVersion > V3_SCHEMA_VERSION) {
    throw new Error(
      `refusing newer Awareness schema version ${identity.userVersion}; v3 supports versions 0-${V3_SCHEMA_VERSION}`,
    );
  }
  if (identity.userVersion === 0) {
    const relations = db.prepare(`
      SELECT name, type
      FROM sqlite_schema
      WHERE type IN ('table', 'view')
        AND name NOT LIKE 'sqlite_%'
      ORDER BY name
    `).all() as Array<{ name: string; type: string }>;
    if (relations.length === 0) return;

    const known = new Set<string>([
      ...canonicalColumns().keys(),
      ...LEGACY_V0_RELATION_NAMES,
      'memories_fts',
    ]);
    const unexpected = relations.filter(({ name, type }) => {
      if (type !== 'table') return true;
      return !known.has(name)
        && !name.startsWith('memories_fts_')
        && !name.startsWith('memory_fts_');
    });
    if (unexpected.length > 0) {
      throw new Error(
        `refusing unrecognized non-empty unversioned SQLite store; unexpected relations: ${unexpected.map(({ name }) => name).join(', ')}`,
      );
    }
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

export function initDb(db: DatabaseSync): void {
  assertV3SchemaIdentity(db);
  if (db.isTransaction) {
    initDbSchema(db);
    return;
  }

  // Serialize the complete detect → migrate → index → version sequence. A
  // transaction around individual ALTERs is insufficient: two first openers
  // can both observe a missing column and race the same DDL.
  db.exec('PRAGMA foreign_keys = OFF');
  let began = false;
  try {
    withSqliteBusyRetry(() => db.exec('BEGIN IMMEDIATE'));
    began = true;
    initDbSchema(db);
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

function initDbSchema(db: DatabaseSync): void {
  migrateLegacyTaskRuns(db);

  // ── 1. All regular tables in a single exec block ───────────────────────────
  db.exec(SCHEMA_DDL);

  // Bring pre-existing stores up to the canonical schema BEFORE any index is
  // created — indexes below reference columns (failure_signature, valid_from,
  // embedding_model, …) that old stores may lack.
  migrateExistingTables(db);
  migrateExecutionSchemaV3(db);
  migrateRefinementQualityConstraint(db);
  migrateCheckConstraints(db);

  // ── 2. All indexes in a single exec block ──────────────────────────────────
  db.exec(`
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
  `);

  // ── 3. FTS5 virtual table (isolated try/catch — fts5 may be unavailable) ──
  try {
    db.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
      USING fts5(memory_id UNINDEXED, task_context, observation, tags)
    `);
  } catch {
    /* fts5 unavailable or already exists */
  }

  // ── 4. Seed FTS if the index is empty (fresh store or cleared) ─────────────
  if (hasFts(db)) {
    const row = db.prepare('SELECT COUNT(*) AS cnt FROM memories_fts').get() as { cnt: number };
    if (row.cnt === 0) rebuildFts(db);
  }

  db.exec('PRAGMA user_version = 3');
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
 * Schema v3 normalizes advisory file presence and makes every lock exclusive.
 * Backfill file rows before rebuilding task_runs/locks so no v2 files_json or
 * lock identity is lost. Historical standalone runs are classified as HOOK:
 * v2 could not distinguish explicit lock-only work from hook-created runs, and HOOK
 * is the conservative lifecycle (verification debt remains visible).
 */
function migrateExecutionSchemaV3(db: DatabaseSync): void {
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

/** Normalized, order-insensitive fingerprint of a table's CHECK clauses. */
function checkClauses(createSql: string): string {
  const matches = createSql.match(/CHECK\s*\([^)]*\)/gi) ?? [];
  return matches.map((c) => c.replace(/\s+/g, ' ').trim().toLowerCase()).sort().join(' | ');
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
    throw err;
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
