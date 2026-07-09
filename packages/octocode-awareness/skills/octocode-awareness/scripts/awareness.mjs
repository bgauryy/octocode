#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (w) => {
  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;
  console.error(w?.stack ?? String(w));
});

// bin/awareness.ts
import { writeFileSync as writeFileSync5, mkdirSync as mkdirSync6, existsSync as existsSync6 } from "node:fs";
import { spawnSync as spawnSync6 } from "node:child_process";
import { dirname as dirname6, isAbsolute as isAbsolute8, join as join9, resolve as resolve15 } from "node:path";
import { DatabaseSync as DatabaseSync2 } from "node:sqlite";
import { fileURLToPath as fileURLToPath4 } from "node:url";

// src/db.ts
import { DatabaseSync } from "node:sqlite";
import { mkdirSync } from "node:fs";
import { join, resolve as resolve2, dirname } from "node:path";
import { homedir, platform } from "node:os";

// src/helpers.ts
import { resolve } from "node:path";
var MEMORY_LABEL_VALUES = [
  "BUG",
  "FEATURE",
  "SUGGESTION",
  "GOTCHA",
  "IMPROVEMENT",
  "DECISION",
  "ARCHITECTURE",
  "SECURITY",
  "PERFORMANCE",
  "TEST",
  "BUILD",
  "DOCS",
  "CONFIG",
  "WORKFLOW",
  "REFACTOR",
  "API",
  "RELEASE",
  "INCIDENT",
  "EXPERIENCE",
  // post-task reflections (worked/partial/failed outcomes)
  "OVERRIDE",
  // contradicts model training defaults (e.g. "this repo uses Bun, not npm")
  "OTHER"
];
var MEMORY_LABELS = new Set(MEMORY_LABEL_VALUES);
var REFLECTION_IMPORTANCE = {
  failed: 8,
  partial: 6,
  worked: 5
};
function utcNow() {
  return (/* @__PURE__ */ new Date()).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function parseJsonList(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  try {
    const parsed = JSON.parse(String(value));
    return Array.isArray(parsed) ? parsed.map(String).filter(Boolean) : [];
  } catch {
    return [];
  }
}
function normalizeTags(tags = [], csv = "") {
  const raw = [...tags];
  if (csv) raw.push(...csv.split(","));
  const seen = /* @__PURE__ */ new Set();
  const out = [];
  for (const t of raw) {
    const cleaned = t.trim().toLowerCase().replace(/[^a-zA-Z0-9_.:-]+/g, "-").replace(/^-|-$/g, "");
    if (cleaned && !seen.has(cleaned)) {
      out.push(cleaned);
      seen.add(cleaned);
    }
  }
  return out;
}
function normalizeReferences(refs = []) {
  const seen = /* @__PURE__ */ new Set();
  return refs.map((r) => (r ?? "").trim().slice(0, 512)).filter((r) => r && !seen.has(r) && seen.add(r)).slice(0, 20);
}
function normalizeLabel(value) {
  if (value == null || String(value).trim() === "") return "OTHER";
  const cleaned = String(value).trim().toUpperCase().replace(/[\s-]+/g, "_");
  if (MEMORY_LABELS.has(cleaned)) return cleaned;
  throw new Error(`invalid label "${String(value)}"; allowed: ${MEMORY_LABEL_VALUES.join(", ")}`);
}
var NOTIFICATION_KIND_VALUES = [
  "claim",
  "handoff",
  "question",
  "reply",
  "blocker",
  "request",
  "decision",
  "fyi"
];
var NOTIFICATION_KINDS = new Set(NOTIFICATION_KIND_VALUES);
function normalizeNotificationKind(value) {
  const cleaned = String(value ?? "").trim().toLowerCase();
  if (NOTIFICATION_KINDS.has(cleaned)) {
    return cleaned;
  }
  throw new Error(
    `invalid signal kind "${String(value)}"; allowed: ${NOTIFICATION_KIND_VALUES.join(", ")}`
  );
}
var REFLECTION_OUTCOME_VALUES = ["worked", "partial", "failed"];
function normalizeReflectionOutcome(value) {
  if (value == null || String(value).trim() === "") return "partial";
  const cleaned = String(value).trim().toLowerCase();
  if (REFLECTION_OUTCOME_VALUES.includes(cleaned)) {
    return cleaned;
  }
  throw new Error(`invalid outcome "${String(value)}"; allowed: ${REFLECTION_OUTCOME_VALUES.join("|")}`);
}
function normalizeFilePath(filePath, cwd) {
  if (!filePath) return null;
  const p = String(filePath);
  return cwd ? resolve(cwd, p) : resolve(p);
}
function normalizeArtifact(value) {
  if (value == null) return null;
  const cleaned = String(value).trim().slice(0, 256);
  return cleaned.length > 0 ? cleaned : null;
}
function rowToMemory(row) {
  return {
    memory_id: row.memory_id,
    agent_id: row.agent_id,
    task_context: row.task_context,
    observation: row.observation,
    importance: row.importance,
    state: row.state ?? "ACTIVE",
    label: row.label ?? "OTHER",
    superseded_by: row.superseded_by ?? null,
    tags: parseJsonList(row.tags_json),
    // references are stored in memory_refs table; populated separately via JOIN
    references: [],
    workspace_path: row.workspace_path ?? null,
    artifact: row.artifact ?? null,
    repo: row.repo ?? null,
    ref: row.ref ?? null,
    novelty_score: row.novelty_score ?? null,
    failure_signature: row.failure_signature ?? null,
    access_count: row.access_count ?? 0,
    last_accessed_at: row.last_accessed_at ?? null,
    decay_half_life_days: row.decay_half_life_days ?? null,
    valid_from: row.valid_from ?? null,
    valid_to: row.valid_to ?? null,
    expired_at: row.expired_at ?? null,
    file_tree_fingerprint: row.file_tree_fingerprint ?? null,
    created_at: row.created_at,
    updated_at: row.updated_at ?? null
  };
}
function summarizeText(value, max) {
  const flat = value.replace(/\s+/g, " ").trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + "...";
}
function projectMemoryLean(memory) {
  return {
    memory_id: memory.memory_id,
    label: memory.label,
    importance: memory.importance,
    task_context: summarizeText(memory.task_context ?? "", 120),
    observation: summarizeText(memory.observation ?? "", 200),
    tags: memory.tags,
    references: memory.references,
    score: memory.score,
    failure_signature: memory.failure_signature,
    created_at: memory.created_at
  };
}

// src/db.ts
var DEFAULT_DB_NAME = "awareness.sqlite3";
var MEMORY_HOME_ENV = "OCTOCODE_MEMORY_HOME";
var SQLITE_BUSY_RETRY_MS = 25;
var SQLITE_BUSY_DEADLINE_MS = 1e4;
var SQLITE_WAIT = new Int32Array(new SharedArrayBuffer(4));
var _db;
function memoryHome() {
  const configured = process.env[MEMORY_HOME_ENV];
  if (configured?.trim()) return resolve2(configured.trim());
  const h = homedir();
  const p = platform();
  if (p === "win32") {
    const appData = process.env["APPDATA"] ?? join(h, "AppData", "Roaming");
    return join(appData, ".octocode", "memory");
  }
  if (p === "darwin") return join(h, ".octocode", "memory");
  const xdg = process.env["XDG_CONFIG_HOME"] ?? join(h, ".config");
  return join(xdg, ".octocode", "memory");
}
function resolveDbPath(dbArg) {
  if (dbArg) return resolve2(dbArg);
  return join(memoryHome(), DEFAULT_DB_NAME);
}
function connectDb(dbPath2) {
  mkdirSync(dirname(dbPath2), { recursive: true });
  const db3 = new DatabaseSync(dbPath2);
  try {
    db3.exec(`PRAGMA busy_timeout = ${SQLITE_BUSY_DEADLINE_MS}`);
    withSqliteBusyRetry(() => db3.exec("PRAGMA journal_mode = WAL"));
    db3.exec("PRAGMA foreign_keys = ON");
    initDb(db3);
    _db = db3;
    return db3;
  } catch (error) {
    db3.close();
    throw error;
  }
}
function isSqliteBusy(error) {
  if (!(error instanceof Error)) return false;
  const sqlite = error;
  return sqlite.errcode === 5 || /database is (?:locked|busy)/i.test(`${sqlite.errstr ?? ""} ${error.message}`);
}
function withSqliteBusyRetry(operation) {
  const deadline = Date.now() + SQLITE_BUSY_DEADLINE_MS;
  for (; ; ) {
    try {
      return operation();
    } catch (error) {
      if (!isSqliteBusy(error) || Date.now() >= deadline) throw error;
      Atomics.wait(SQLITE_WAIT, 0, 0, SQLITE_BUSY_RETRY_MS);
    }
  }
}
function checkpointWal(db3) {
  try {
    db3.exec("PRAGMA wal_checkpoint(TRUNCATE)");
  } catch {
  }
}
function getDeliveryFingerprint(db3, key) {
  const row = db3.prepare(`SELECT fingerprint FROM delivery_state
    WHERE consumer_id = ? AND channel = ? AND scope_key = ?`).get(key.consumerId, key.channel, key.scopeKey);
  return row?.fingerprint ?? null;
}
function setDeliveryFingerprint(db3, params) {
  db3.prepare(`INSERT INTO delivery_state
      (consumer_id, channel, scope_key, fingerprint, delivered_at)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(consumer_id, channel, scope_key) DO UPDATE SET
      fingerprint = excluded.fingerprint,
      delivered_at = excluded.delivered_at`).run(params.consumerId, params.channel, params.scopeKey, params.fingerprint, params.deliveredAt ?? utcNow());
}
var SCHEMA_DDL = `
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

    -- ARCH-5: Agent identity registry \u2014 maps opaque agentIds to human-readable names.
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
function tableExists(db3, table) {
  return Boolean(db3.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?"
  ).get(table));
}
function renameColumnIfPresent(db3, table, from, to) {
  if (!tableExists(db3, table)) return;
  const columns = tableColumns(db3, table);
  if (columns.has(from) && !columns.has(to)) {
    db3.exec(`ALTER TABLE ${table} RENAME COLUMN ${from} TO ${to}`);
  }
}
function migrateLegacyTaskRuns(db3) {
  if (!tableExists(db3, "tasks")) return;
  const columns = tableColumns(db3, "tasks");
  const isLegacyExecutionTable = columns.has("agent_id") && columns.has("test_plan") && !columns.has("plan_id");
  if (!isLegacyExecutionTable) return;
  if (tableExists(db3, "task_runs")) {
    throw new Error("schema migration cannot move legacy tasks: task_runs already exists");
  }
  for (const index of ["idx_tasks_status", "idx_tasks_agent_status", "idx_tasks_workspace", "idx_tasks_scope"]) {
    db3.exec(`DROP INDEX IF EXISTS ${index}`);
  }
  db3.exec("ALTER TABLE tasks RENAME TO task_runs");
  renameColumnIfPresent(db3, "task_runs", "task_id", "run_id");
  renameColumnIfPresent(db3, "task_runs", "plan_doc_ref", "context_ref");
  renameColumnIfPresent(db3, "locks", "task_id", "run_id");
  if (tableExists(db3, "task_log") && !tableExists(db3, "run_log")) {
    db3.exec("ALTER TABLE task_log RENAME TO run_log");
  }
  renameColumnIfPresent(db3, "run_log", "task_id", "run_id");
  renameColumnIfPresent(db3, "edit_log", "task_id", "run_id");
  renameColumnIfPresent(db3, "harness_log", "task_id", "run_id");
}
function initDb(db3) {
  if (db3.isTransaction) {
    initDbSchema(db3);
    return;
  }
  db3.exec("PRAGMA foreign_keys = OFF");
  let began = false;
  try {
    withSqliteBusyRetry(() => db3.exec("BEGIN IMMEDIATE"));
    began = true;
    initDbSchema(db3);
    db3.exec("COMMIT");
    began = false;
  } catch (error) {
    if (began) {
      try {
        db3.exec("ROLLBACK");
      } catch {
      }
    }
    throw error;
  } finally {
    db3.exec("PRAGMA foreign_keys = ON");
  }
}
function initDbSchema(db3) {
  migrateLegacyTaskRuns(db3);
  db3.exec(SCHEMA_DDL);
  migrateExistingTables(db3);
  migrateExecutionSchemaV3(db3);
  migrateRefinementQualityConstraint(db3);
  migrateCheckConstraints(db3);
  db3.exec(`
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
  try {
    db3.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
      USING fts5(memory_id UNINDEXED, task_context, observation, tags)
    `);
  } catch {
  }
  if (hasFts(db3)) {
    const row = db3.prepare("SELECT COUNT(*) AS cnt FROM memories_fts").get();
    if (row.cnt === 0) rebuildFts(db3);
  }
  db3.exec("PRAGMA user_version = 3");
}
function tableColumns(db3, tableName) {
  const rows = db3.prepare(`PRAGMA table_info(${tableName})`).all();
  return new Set(rows.map((r) => r.name));
}
var _canonicalColumns;
function canonicalColumns() {
  if (_canonicalColumns) return _canonicalColumns;
  const tmp = new DatabaseSync(":memory:");
  try {
    tmp.exec(SCHEMA_DDL);
    const tables = tmp.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    ).all();
    const map = /* @__PURE__ */ new Map();
    for (const { name } of tables) {
      map.set(name, tmp.prepare(`PRAGMA table_info(${name})`).all());
    }
    _canonicalColumns = map;
    return map;
  } finally {
    tmp.close();
  }
}
function isConstantDefault(dflt) {
  return dflt !== null && !dflt.includes("(");
}
function migrateExistingTables(db3) {
  for (const [table, columns] of canonicalColumns()) {
    const existing = tableColumns(db3, table);
    for (const col of columns) {
      if (existing.has(col.name)) continue;
      let clause = `${col.name} ${col.type}`;
      if (isConstantDefault(col.dflt_value)) {
        if (col.notnull) clause += " NOT NULL";
        clause += ` DEFAULT ${col.dflt_value}`;
      }
      db3.exec(`ALTER TABLE ${table} ADD COLUMN ${clause}`);
    }
  }
}
function migrateExecutionSchemaV3(db3) {
  if (!tableExists(db3, "task_runs")) return;
  const runColumns = tableColumns(db3, "task_runs");
  const lockColumns = tableExists(db3, "locks") ? tableColumns(db3, "locks") : /* @__PURE__ */ new Set();
  const needsRunRebuild = runColumns.has("files_json");
  const needsLockRebuild = ["agent_id", "session_id", "lock_type"].some((name) => lockColumns.has(name));
  if (!needsRunRebuild && !needsLockRebuild) return;
  if (needsRunRebuild) {
    const rows = db3.prepare(`SELECT run_id, task_id, status, files_json, created_at, updated_at
      FROM task_runs`).all();
    const insert = db3.prepare(`INSERT OR IGNORE INTO run_files
      (run_id, file_path, reason_override, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES (?, ?, NULL, ?, ?, ?, ?, ?)`);
    const now = utcNow();
    for (const row of rows) {
      const source = row.task_id == null ? "HOOK" : "EXPLICIT";
      const startedAt = row.created_at ?? now;
      const heartbeatAt = row.updated_at ?? startedAt;
      for (const filePath of parseJsonList(row.files_json)) {
        const lease = db3.prepare(`SELECT MAX(expires_at) AS expires_at FROM (
          SELECT expires_at FROM locks WHERE run_id = ? AND file_path = ?
          UNION ALL
          SELECT expires_at FROM task_claims WHERE run_id = ?
        )`).get(row.run_id, filePath, row.run_id);
        const expiresAt = lease.expires_at ?? heartbeatAt;
        const active = row.status === "ACTIVE" && expiresAt > now;
        insert.run(row.run_id, filePath, source, startedAt, heartbeatAt, expiresAt, active ? null : heartbeatAt);
      }
    }
    db3.prepare(`UPDATE task_runs SET origin = CASE
      WHEN task_id IS NOT NULL THEN 'TASK' ELSE 'HOOK' END`).run();
  }
  if (needsRunRebuild) {
    const sql = canonicalTableSql().get("task_runs");
    if (!sql) throw new Error("schema migration cannot find canonical task_runs DDL");
    rebuildTableFromCanonical(db3, "task_runs", sql);
  }
  if (needsLockRebuild) {
    const sql = canonicalTableSql().get("locks");
    if (!sql) throw new Error("schema migration cannot find canonical locks DDL");
    rebuildTableFromCanonical(db3, "locks", sql);
  }
}
var _canonicalTableSql;
function canonicalTableSql() {
  if (_canonicalTableSql) return _canonicalTableSql;
  const tmp = new DatabaseSync(":memory:");
  try {
    tmp.exec(SCHEMA_DDL);
    const rows = tmp.prepare(
      "SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL"
    ).all();
    _canonicalTableSql = new Map(rows.map((r) => [r.name, r.sql]));
    return _canonicalTableSql;
  } finally {
    tmp.close();
  }
}
function checkClauses(createSql) {
  const matches = createSql.match(/CHECK\s*\([^)]*\)/gi) ?? [];
  return matches.map((c) => c.replace(/\s+/g, " ").trim().toLowerCase()).sort().join(" | ");
}
function rebuildTableFromCanonical(db3, table, canonSql) {
  const liveCols = tableColumns(db3, table);
  const canonCols = (canonicalColumns().get(table) ?? []).map((c) => c.name).filter((n) => liveCols.has(n));
  if (canonCols.length === 0) return;
  const colList = canonCols.join(", ");
  const tmpName = `${table}__ckmig`;
  const createTmp = canonSql.replace(
    new RegExp(`(CREATE\\s+TABLE\\s+(?:IF\\s+NOT\\s+EXISTS\\s+)?)"?${table}"?`, "i"),
    `$1${tmpName}`
  );
  if (!createTmp.includes(tmpName)) {
    throw new Error(`check-constraint migration: cannot rename table ${table} in canonical DDL`);
  }
  const savepoint = `migrate_check_${table}`;
  db3.exec(`SAVEPOINT ${savepoint}`);
  try {
    db3.exec(`DROP TABLE IF EXISTS ${tmpName};`);
    db3.exec(createTmp);
    db3.exec(`INSERT INTO ${tmpName} (${colList}) SELECT ${colList} FROM ${table};`);
    db3.exec(`DROP TABLE ${table};`);
    db3.exec(`ALTER TABLE ${tmpName} RENAME TO ${table};`);
    db3.exec(`RELEASE SAVEPOINT ${savepoint}`);
  } catch (err) {
    try {
      db3.exec(`ROLLBACK TO SAVEPOINT ${savepoint}`);
    } catch {
    }
    try {
      db3.exec(`RELEASE SAVEPOINT ${savepoint}`);
    } catch {
    }
    throw err;
  }
}
function migrateCheckConstraints(db3) {
  const drifted = [];
  for (const [table, canonSql] of canonicalTableSql()) {
    const live = db3.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name=?"
    ).get(table);
    if (!live?.sql) continue;
    if (checkClauses(live.sql) !== checkClauses(canonSql)) drifted.push([table, canonSql]);
  }
  if (drifted.length === 0) return;
  for (const [table, canonSql] of drifted) rebuildTableFromCanonical(db3, table, canonSql);
}
function migrateRefinementQualityConstraint(db3) {
  const row = db3.prepare(
    "SELECT sql FROM sqlite_master WHERE type='table' AND name='refinements'"
  ).get();
  if (!row?.sql || row.sql.includes("'instructions'")) return;
  db3.exec("SAVEPOINT migrate_refinement_quality_constraint");
  try {
    db3.exec(`
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
    db3.exec("RELEASE SAVEPOINT migrate_refinement_quality_constraint");
  } catch (err) {
    try {
      db3.exec("ROLLBACK TO SAVEPOINT migrate_refinement_quality_constraint");
    } catch {
    }
    try {
      db3.exec("RELEASE SAVEPOINT migrate_refinement_quality_constraint");
    } catch {
    }
    throw err;
  }
}
function hasFts(db3) {
  const row = db3.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories_fts'"
  ).get();
  return Boolean(row);
}
function ftsTermsForRow(row) {
  const tags = parseJsonList(row.tags_json);
  const label = (row.label ?? "OTHER").toLowerCase();
  return [...tags, label, ...row.references ?? []].filter(Boolean).join(" ");
}
function rebuildFts(db3) {
  db3.exec("SAVEPOINT rebuild_fts");
  try {
    db3.exec("DELETE FROM memories_fts");
    const rows = db3.prepare(
      "SELECT memory_id, task_context, observation, tags_json, label FROM memories"
    ).all();
    if (rows.length > 0) {
      const refs = db3.prepare(
        `SELECT r.memory_id, r.reference
         FROM memory_refs r
         JOIN memories m ON m.memory_id = r.memory_id
         ORDER BY r.memory_id, r.ordinal`
      ).all();
      const refsByMemory = /* @__PURE__ */ new Map();
      for (const ref of refs) {
        const list = refsByMemory.get(ref.memory_id) ?? [];
        list.push(ref.reference);
        refsByMemory.set(ref.memory_id, list);
      }
      for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
    }
    const insert = db3.prepare(
      "INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
    );
    for (const row of rows) {
      insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
    }
    db3.exec("RELEASE SAVEPOINT rebuild_fts");
  } catch (e) {
    try {
      db3.exec("ROLLBACK TO SAVEPOINT rebuild_fts");
    } catch {
    }
    try {
      db3.exec("RELEASE SAVEPOINT rebuild_fts");
    } catch {
    }
    throw e;
  }
}
function referenceKind(reference) {
  if (/^https?:\/\//.test(reference)) return "url";
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.\-]*):/);
  return m ? m[1].toLowerCase() : "other";
}
function replaceMemoryReferences(db3, memoryId, references) {
  db3.prepare("DELETE FROM memory_refs WHERE memory_id = ?").run(memoryId);
  const insert = db3.prepare(
    "INSERT OR REPLACE INTO memory_refs(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)"
  );
  references.forEach((ref, i) => insert.run(memoryId, ref, referenceKind(ref), i));
}
function evictExpiredLocks(db3) {
  const now = utcNow();
  const stale = db3.prepare(
    "SELECT COUNT(*) AS c FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?"
  ).get(now);
  if (stale.c === 0) return { pruned_locks: 0 };
  db3.exec("SAVEPOINT evict_expired_locks");
  try {
    const deleteRes = db3.prepare(
      "DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?"
    ).run(now);
    db3.exec("RELEASE SAVEPOINT evict_expired_locks");
    return { pruned_locks: deleteRes.changes };
  } catch (e) {
    try {
      db3.exec("ROLLBACK TO SAVEPOINT evict_expired_locks");
    } catch {
    }
    try {
      db3.exec("RELEASE SAVEPOINT evict_expired_locks");
    } catch {
    }
    throw e;
  }
}

// src/memory.ts
import { randomUUID } from "node:crypto";

// src/git.ts
import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { basename, dirname as dirname2, join as join2, resolve as resolve3 } from "node:path";
function runCmd(cmd, args2, cwd) {
  try {
    const r = spawnSync(cmd, args2, { cwd: cwd ?? process.cwd(), encoding: "utf8", timeout: 5e3 });
    return r.status === 0 ? r.stdout.trim() : null;
  } catch {
    return null;
  }
}
function detectGit(cwd) {
  const root = runCmd("git", ["-C", cwd ?? ".", "rev-parse", "--show-toplevel"]);
  if (!root) return { is_repo: false };
  const branch = runCmd("git", ["-C", root, "rev-parse", "--abbrev-ref", "HEAD"]);
  const remote = runCmd("git", ["-C", root, "remote", "get-url", "origin"]);
  const repoName = remote ? (remote.match(/github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?$/) ?? [])[1] ?? basename(root) : basename(root);
  return { is_repo: true, root, repo: repoName, branch, remote };
}
function canonicalizePath(input) {
  let dir = resolve3(input);
  const tail = [];
  for (let guard = 0; guard < 4096; guard += 1) {
    try {
      return tail.length ? join2(realpathSync(dir), ...tail) : realpathSync(dir);
    } catch {
      const parent = dirname2(dir);
      if (parent === dir) return resolve3(input);
      tail.unshift(basename(dir));
      dir = parent;
    }
  }
  return resolve3(input);
}
function fillScope(partial, cwd) {
  const explicitWorkspace = partial.workspace_path ? canonicalizePath(partial.workspace_path) : null;
  const scope = {
    workspace_path: explicitWorkspace,
    artifact: partial.artifact ?? null,
    repo: partial.repo ?? null,
    ref: partial.ref ?? null
  };
  const git = detectGit(scope.workspace_path ?? cwd ?? process.cwd());
  if (!git.is_repo) return scope;
  if (git.root) scope.workspace_path = canonicalizePath(git.root);
  if (!scope.repo && git.repo) scope.repo = git.repo;
  if (!scope.ref && git.branch) scope.ref = git.branch;
  return scope;
}
function normalizeWorkspacePath(workspacePath, cwd) {
  const candidate = workspacePath ? resolve3(workspacePath) : cwd ? resolve3(cwd) : null;
  const scope = fillScope({ workspace_path: candidate }, candidate ?? process.cwd());
  if (scope.workspace_path) return scope.workspace_path;
  return candidate;
}

// src/memory.ts
var DECAY_WEIGHTS = { importance: 0.25, recency: 0.3, access: 0.15, lexical: 0.3 };
var DEFAULT_HALF_LIFE_DAYS = 30;
var ACCESS_SATURATION = 50;
var BM25_SQUASH_K = 1;
var BM25_DEGENERATE_MAX = 0.01;
var JUDGMENT_RELEVANCE_FLOOR = 0.35;
var SALIENCE_FLOOR = 8;
var LABEL_HALF_LIFE_DAYS = {
  DECISION: 90,
  ARCHITECTURE: 90,
  SECURITY: 90,
  GOTCHA: 90,
  OVERRIDE: 90,
  // permanent corrections to model defaults — decay as slowly as DECISION
  EXPERIENCE: 14
};
var SCORING_PREFETCH_FACTOR = 3;
var SIMILARITY_THRESHOLD = 0.45;
var SIMILARITY_PREFETCH = 12;
var STOP_WORDS = /* @__PURE__ */ new Set([
  // Articles / conjunctions
  "the",
  "and",
  "for",
  "with",
  "from",
  "into",
  "not",
  // Demonstratives
  "this",
  "that",
  "its",
  // Question words
  "what",
  "when",
  "about",
  "before",
  "after",
  // Common verbs (too generic to be useful in memory search)
  "are",
  "was",
  "has",
  "had",
  "can",
  "did",
  "use",
  "used",
  "using"
]);
function textTokens(text) {
  const split = text.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2").replace(/[:_-]/g, " ").toLowerCase();
  return new Set(
    (split.match(/[a-z0-9]{3,}/g) ?? []).filter((t) => !STOP_WORDS.has(t))
  );
}
function jaccard(a, b) {
  if (a.size === 0 || b.size === 0) return 0;
  let intersection = 0;
  for (const token of a) if (b.has(token)) intersection++;
  return intersection / (a.size + b.size - intersection);
}
function findSimilarMemories(db3, text, limit = 3, excludeMemoryId = null, scopeOptions = {}) {
  const queryTokens = textTokens(text);
  if (queryTokens.size === 0) return [];
  const candidates = lexicalSearch(
    db3,
    text,
    SIMILARITY_PREFETCH,
    1,
    [],
    [],
    ["ACTIVE"],
    scopeOptions
  ).filter((m) => m.memory_id !== excludeMemoryId);
  return candidates.map((m) => ({
    memory_id: m.memory_id,
    similarity: jaccard(queryTokens, textTokens(`${m.task_context} ${m.observation}`))
  })).filter((m) => m.similarity >= SIMILARITY_THRESHOLD).sort((a, b) => b.similarity - a.similarity).slice(0, limit);
}
function decayComponents(memory, lexical, weights = DECAY_WEIGHTS) {
  const halfLife = memory.decay_half_life_days ?? DEFAULT_HALF_LIFE_DAYS;
  const lastUsedStr = memory.last_accessed_at ?? memory.created_at;
  let recency = 0;
  if (lastUsedStr) {
    const ageDays = Math.max(0, (Date.now() - new Date(lastUsedStr).getTime()) / 864e5);
    recency = Math.exp(-Math.LN2 * ageDays / Math.max(halfLife, 0.01));
  }
  const importance = (memory.importance ?? 0) / 10;
  const access = Math.min(
    Math.log1p(memory.access_count ?? 0) / Math.log1p(ACCESS_SATURATION),
    1
  );
  const relevance = Math.max(0, Math.min(1, lexical));
  const final = weights.importance * importance + weights.recency * recency + weights.access * access + weights.lexical * relevance;
  return { importance, recency, access, relevance, weights, final };
}
function decayScore(memory, lexical, weights = DECAY_WEIGHTS) {
  return decayComponents(memory, lexical, weights).final;
}
function buildFtsQuery(query) {
  const normalized = query.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2").replace(/[:_-]/g, " ").toLowerCase();
  const tokens = [
    ...new Set(
      (normalized.match(/[a-z0-9]{3,}/g) ?? []).filter((t) => !STOP_WORDS.has(t))
    )
  ].slice(0, 16);
  if (tokens.length === 0) return null;
  return tokens.join(" OR ");
}
function escapeLike(value) {
  return value.replace(/[\\%_]/g, "\\$&");
}
function appendFallbackQueryConditions(query, conditions, params) {
  const tokens = [...textTokens(query)].slice(0, 16);
  if (tokens.length === 0) return;
  const tokenClauses = [];
  for (const token of tokens) {
    const pattern = `%${escapeLike(token)}%`;
    tokenClauses.push(`(
      lower(m.task_context) LIKE ? ESCAPE '\\'
      OR lower(m.observation) LIKE ? ESCAPE '\\'
      OR lower(m.tags_json) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.label, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.workspace_path, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.artifact, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.repo, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.ref, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.failure_signature, '')) LIKE ? ESCAPE '\\'
      OR EXISTS (
        SELECT 1 FROM memory_refs r
        WHERE r.memory_id = m.memory_id
          AND lower(r.reference) LIKE ? ESCAPE '\\'
      )
    )`);
    params.push(pattern, pattern, pattern, pattern, pattern, pattern, pattern, pattern, pattern, pattern);
  }
  conditions.push(`(${tokenClauses.join(" OR ")})`);
}
function fallbackSearch(db3, query, conditions, params, limit) {
  const fallbackConditions = [...conditions];
  const fallbackParams = [...params];
  appendFallbackQueryConditions(query, fallbackConditions, fallbackParams);
  const sql = `
    SELECT m.*, 0 AS _bm25
    FROM memories m
    WHERE ${fallbackConditions.join(" AND ")}
    ORDER BY m.importance DESC, m.created_at DESC
    LIMIT ?
  `;
  return db3.prepare(sql).all(...fallbackParams, limit);
}
function applyScopeConditions(conditions, params, options = {}) {
  const artifact2 = normalizeArtifact(options.artifact);
  const scope = fillScope(
    {
      workspace_path: options.workspacePath ?? null,
      artifact: artifact2,
      repo: options.repo ?? null,
      ref: options.ref ?? null
    },
    options.cwd ?? options.workspacePath ?? process.cwd()
  );
  if (options.globalOnly) {
    conditions.push("m.workspace_path IS NULL", "m.artifact IS NULL", "m.repo IS NULL", "m.ref IS NULL");
    return;
  }
  if (scope.workspace_path) {
    conditions.push(options.strictScope ? "m.workspace_path = ?" : "(m.workspace_path IS NULL OR m.workspace_path = ?)");
    params.push(scope.workspace_path);
  }
  if (scope.artifact) {
    conditions.push(options.strictScope ? "m.artifact = ?" : "(m.artifact IS NULL OR m.artifact = ?)");
    params.push(scope.artifact);
  }
  if (scope.repo) {
    conditions.push(options.strictScope ? "m.repo = ?" : "(m.repo IS NULL OR m.repo = ?)");
    params.push(scope.repo);
  }
  if (scope.ref) {
    conditions.push(options.strictScope ? "m.ref = ?" : "(m.ref IS NULL OR m.ref = ?)");
    params.push(scope.ref);
  }
}
function lexicalSearch(db3, query, limit, minImportance, tags, labels, states, scopeOptions = {}) {
  const ftsQuery = query ? buildFtsQuery(query) : null;
  const params = [];
  const conditions = [
    "m.importance >= ?",
    `m.state IN (${states.map(() => "?").join(",")})`
  ];
  params.push(minImportance, ...states);
  if (labels.length > 0) {
    conditions.push(`m.label IN (${labels.map(() => "?").join(",")})`);
    params.push(...labels);
  }
  for (const tag of tags) {
    conditions.push("EXISTS (SELECT 1 FROM json_each(m.tags_json) WHERE value = ?)");
    params.push(tag);
  }
  applyScopeConditions(conditions, params, scopeOptions);
  if (scopeOptions.asOf) {
    conditions.push("(m.valid_from IS NULL OR m.valid_from <= ?)");
    conditions.push("(m.valid_to IS NULL OR m.valid_to > ?)");
    params.push(scopeOptions.asOf, scopeOptions.asOf);
  }
  const candidateIds = scopeOptions.candidateMemoryIds ? [...new Set(scopeOptions.candidateMemoryIds)].filter(Boolean) : null;
  if (candidateIds && candidateIds.length === 0) return [];
  let usingCandidateTable = false;
  if (candidateIds) {
    if (candidateIds.length <= 400) {
      conditions.push(`m.memory_id IN (${candidateIds.map(() => "?").join(",")})`);
      params.push(...candidateIds);
    } else {
      db3.exec("CREATE TEMP TABLE IF NOT EXISTS temp_memory_candidate_ids(memory_id TEXT PRIMARY KEY)");
      db3.exec("DELETE FROM temp_memory_candidate_ids");
      const insertCandidate = db3.prepare("INSERT OR IGNORE INTO temp_memory_candidate_ids(memory_id) VALUES (?)");
      for (const id of candidateIds) insertCandidate.run(id);
      conditions.push("EXISTS (SELECT 1 FROM temp_memory_candidate_ids c WHERE c.memory_id = m.memory_id)");
      usingCandidateTable = true;
    }
  }
  let rows;
  try {
    if (ftsQuery && hasFts(db3)) {
      try {
        const sql = `
          SELECT m.*, ABS(bm25(memories_fts, 0, 10, 7, 2)) AS _bm25
          FROM memories m
          JOIN memories_fts ON memories_fts.memory_id = m.memory_id
          WHERE memories_fts MATCH ?
            AND ${conditions.join(" AND ")}
          ORDER BY _bm25 DESC
          LIMIT ?
        `;
        rows = db3.prepare(sql).all(ftsQuery, ...params, limit);
      } catch {
        rows = fallbackSearch(db3, query, conditions, params, limit);
      }
    } else {
      rows = fallbackSearch(db3, query, conditions, params, limit);
    }
  } finally {
    if (usingCandidateTable) {
      try {
        db3.exec("DELETE FROM temp_memory_candidate_ids");
      } catch {
      }
    }
  }
  const maxBm25 = rows.reduce((m, r) => Math.max(m, r._bm25 ?? 0), 0);
  return rows.map((row) => {
    const lexical = maxBm25 >= BM25_DEGENERATE_MAX ? (row._bm25 ?? 0) / (maxBm25 + BM25_SQUASH_K) : 0.5;
    const mem = rowToMemory(row);
    mem.lexical = lexical;
    mem.score = decayScore(mem, lexical);
    return mem;
  });
}
function attachMemoryReferences(db3, memories) {
  if (memories.length === 0) return;
  try {
    const ids = [...new Set(memories.map((m) => m.memory_id))];
    const ph = ids.map(() => "?").join(",");
    const rows = db3.prepare(
      `SELECT memory_id, reference
       FROM memory_refs
       WHERE memory_id IN (${ph})
       ORDER BY memory_id, ordinal`
    ).all(...ids);
    const refsByMemory = /* @__PURE__ */ new Map();
    for (const row of rows) {
      const refs = refsByMemory.get(row.memory_id) ?? [];
      refs.push(row.reference);
      refsByMemory.set(row.memory_id, refs);
    }
    for (const memory of memories) {
      memory.references = refsByMemory.get(memory.memory_id) ?? [];
    }
  } catch (e) {
    if (!(e instanceof Error && e.message.includes("no such table"))) throw e;
  }
}
function compileRecallRegex(pattern) {
  try {
    return new RegExp(pattern);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`invalid regex ${JSON.stringify(pattern)}: ${message}`);
  }
}
function intersectCandidateIds(current, next) {
  if (current === null) return new Set(next);
  const out = /* @__PURE__ */ new Set();
  for (const id of current) if (next.has(id)) out.add(id);
  return out;
}
function exactReferenceCandidateIds(db3, references) {
  const refs = normalizeReferences(references);
  if (refs.length === 0) return /* @__PURE__ */ new Set();
  const rows = db3.prepare(
    `SELECT memory_id
     FROM memory_refs
     WHERE reference IN (${refs.map(() => "?").join(",")})
     GROUP BY memory_id
     HAVING COUNT(DISTINCT reference) = ?`
  ).all(...refs, refs.length);
  return new Set(rows.map((row) => row.memory_id));
}
function fileReferenceCandidates(files, baseDir) {
  const refs = /* @__PURE__ */ new Set();
  for (const raw of files) {
    const file = String(raw ?? "").trim();
    if (!file) continue;
    refs.add(file);
    if (file.startsWith("file:")) {
      const unprefixed = file.slice(5);
      if (unprefixed) refs.add(unprefixed);
      continue;
    }
    refs.add(`file:${file}`);
    const normalized = normalizeFilePath(file, baseDir ?? void 0);
    if (normalized) {
      refs.add(normalized);
      refs.add(`file:${normalized}`);
    }
  }
  return [...refs];
}
function anyReferenceCandidateIds(db3, references) {
  const refs = [...new Set(references.map((ref) => String(ref ?? "").trim().slice(0, 512)).filter(Boolean))];
  if (refs.length === 0) return /* @__PURE__ */ new Set();
  const rows = db3.prepare(
    `SELECT DISTINCT memory_id
     FROM memory_refs
     WHERE reference IN (${refs.map(() => "?").join(",")})`
  ).all(...refs);
  return new Set(rows.map((row) => row.memory_id));
}
function fileRegexCandidateIds(db3, regexes) {
  if (regexes.length === 0) return /* @__PURE__ */ new Set();
  const rows = db3.prepare(
    `SELECT memory_id, reference
     FROM memory_refs
     WHERE kind = 'file' OR reference LIKE 'file:%'
     ORDER BY memory_id, ordinal`
  ).all();
  const refsByMemory = /* @__PURE__ */ new Map();
  for (const row of rows) {
    const refs = refsByMemory.get(row.memory_id) ?? [];
    refs.push(row.reference);
    refsByMemory.set(row.memory_id, refs);
  }
  const ids = /* @__PURE__ */ new Set();
  for (const [memoryId, refs] of refsByMemory.entries()) {
    if (regexes.every((re) => refs.some((ref) => re.test(ref)))) ids.add(memoryId);
  }
  return ids;
}
function regexCandidateIds(db3, regexes) {
  if (regexes.length === 0) return /* @__PURE__ */ new Set();
  const rows = db3.prepare(
    `SELECT m.*, group_concat(r.reference, char(31)) AS references_text
     FROM memories m
     LEFT JOIN memory_refs r ON r.memory_id = m.memory_id
     GROUP BY m.memory_id`
  ).all();
  const ids = /* @__PURE__ */ new Set();
  for (const row of rows) {
    const haystack = [
      row.task_context,
      row.observation,
      ...parseJsonList(row.tags_json),
      ...row.references_text ? row.references_text.split("") : [],
      row.label,
      row.workspace_path,
      row.artifact,
      row.repo,
      row.ref,
      row.failure_signature
    ].filter(Boolean).join(" ");
    if (regexes.every((re) => re.test(haystack))) ids.add(row.memory_id);
  }
  return ids;
}
function bumpAccess(db3, memoryIds) {
  if (memoryIds.length === 0) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => "?").join(",");
  db3.prepare(`
    UPDATE memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${placeholders})
  `).run(now, ...memoryIds);
}
function insertMemory(db3, params) {
  const {
    agentId: agentId2 = "agent",
    taskContext,
    observation,
    importance,
    label,
    tags = [],
    tagsCsv = "",
    references = [],
    supersedes = [],
    failureSignature = null,
    validFrom: vf,
    validTo: vt,
    workspacePath,
    artifact: artifact2,
    repo: repoArg,
    ref: refArg,
    fileTreeFingerprint = null,
    cwd
  } = params;
  const imp = Number(importance);
  if (!Number.isInteger(imp) || imp < 1 || imp > 10) {
    throw new Error(`importance must be 1\u201310, got ${String(importance)}`);
  }
  const memoryId = "mem_" + randomUUID().replace(/-/g, "");
  const tagList = normalizeTags(tags, tagsCsv);
  const refList = normalizeReferences(references);
  const normalizedLabel = normalizeLabel(Array.isArray(label) ? label[0] : label);
  const createdAt = utcNow();
  const validFromVal = vf ?? createdAt;
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact2), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const halfLifeDefault = LABEL_HALF_LIFE_DAYS[normalizedLabel] ?? null;
  let noveltyScore = 0;
  let similarMemoryIds = [];
  const superseded = [];
  db3.exec("BEGIN IMMEDIATE");
  try {
    const similar = params.preComputedSimilar ?? findSimilarMemories(db3, `${taskContext} ${observation}`, 3, null, {
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd
    });
    noveltyScore = Math.max(0, Math.min(1, 1 - (similar[0]?.similarity ?? 0)));
    similarMemoryIds = similar.map((m) => m.memory_id);
    db3.prepare(`
      INSERT INTO memories (
        memory_id, agent_id, task_context, observation, importance,
        label, tags_json, workspace_path, artifact, repo, ref,
        file_tree_fingerprint, novelty_score, created_at, updated_at,
        last_accessed_at, access_count, failure_signature, valid_from, valid_to, decay_half_life_days
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)
    `).run(
      memoryId,
      agentId2,
      taskContext,
      observation,
      imp,
      normalizedLabel,
      JSON.stringify(tagList),
      scope.workspace_path,
      scope.artifact,
      scope.repo,
      scope.ref,
      fileTreeFingerprint,
      noveltyScore,
      createdAt,
      createdAt,
      createdAt,
      failureSignature ?? null,
      validFromVal,
      vt ?? null,
      halfLifeDefault
    );
    if (refList.length > 0) {
      try {
        replaceMemoryReferences(db3, memoryId, refList);
      } catch (e) {
        if (!(e instanceof Error && e.message.includes("no such table"))) throw e;
      }
    }
    if (hasFts(db3)) {
      db3.prepare(
        "INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
      ).run(
        memoryId,
        taskContext,
        observation,
        ftsTermsForRow({
          tags_json: JSON.stringify(tagList),
          label: normalizedLabel,
          references: refList
        })
      );
    }
    for (const oldId of supersedes) {
      const r = db3.prepare(`
        UPDATE memories
        SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
            valid_to = COALESCE(valid_to, ?), expired_at = ?
        WHERE memory_id = ? AND memory_id <> ?
      `).run(memoryId, createdAt, validFromVal, createdAt, oldId, memoryId);
      if (r.changes) superseded.push(oldId);
    }
    db3.exec("COMMIT");
  } catch (e) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
  return {
    memoryId,
    memory: {
      memory_id: memoryId,
      agent_id: agentId2,
      task_context: taskContext,
      observation,
      importance: imp,
      label: normalizedLabel,
      tags: tagList,
      references: refList,
      workspace_path: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      failure_signature: failureSignature ?? null,
      novelty_score: noveltyScore,
      state: "ACTIVE",
      created_at: createdAt
    },
    superseded,
    noveltyScore,
    similarMemoryIds
  };
}
function getMemory(db3, params = {}) {
  const {
    query = "",
    limit: limitRaw = 3,
    minImportance: minImpRaw = 1,
    label,
    tags = [],
    smart = false,
    workspacePath,
    artifact: artifact2,
    repo: repoArg,
    ref: refArg,
    states: statesRaw,
    sort = "smart",
    globalOnly = false,
    strictScope = false,
    asOf,
    references = [],
    regex = [],
    fileRegex = [],
    files = [],
    explain = false,
    cwd: cwdParam
  } = params;
  const limit = Math.min(20, Math.max(1, Number(limitRaw) || 3));
  let minImportance = Math.max(1, Number(minImpRaw) || 1);
  if (smart === true || smart === "true") minImportance = Math.max(1, minImportance - 1);
  const states = statesRaw ?? ["ACTIVE"];
  const labels = label ? Array.isArray(label) ? label.map((value) => normalizeLabel(value)) : [normalizeLabel(label)] : [];
  const effectiveCwd = cwdParam ?? workspacePath ?? void 0;
  const asOfDate = asOf ? new Date(asOf) : null;
  if (asOfDate && isNaN(asOfDate.getTime())) {
    throw new Error(`invalid --as-of value "${asOf}" \u2014 expected ISO 8601 date string (e.g. 2024-06-01T00:00:00Z)`);
  }
  let candidateIds = null;
  const refFilters = normalizeReferences(references);
  const fileRefFilters = fileReferenceCandidates(files, effectiveCwd);
  const compiledRegex = regex.map(compileRecallRegex);
  const compiledFileRegex = fileRegex.map(compileRecallRegex);
  if (refFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, exactReferenceCandidateIds(db3, refFilters));
  }
  if (fileRefFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, anyReferenceCandidateIds(db3, fileRefFilters));
  }
  if (compiledFileRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, fileRegexCandidateIds(db3, compiledFileRegex));
  }
  if (compiledRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, regexCandidateIds(db3, compiledRegex));
  }
  let memories = lexicalSearch(
    db3,
    query,
    limit * SCORING_PREFETCH_FACTOR,
    minImportance,
    tags,
    labels,
    states,
    {
      workspacePath: workspacePath ?? cwdParam ?? null,
      artifact: artifact2,
      repo: repoArg,
      ref: refArg,
      strictScope,
      globalOnly,
      cwd: cwdParam,
      asOf: asOf ?? null,
      candidateMemoryIds: candidateIds ? [...candidateIds] : void 0
    }
  );
  attachMemoryReferences(db3, memories);
  if (fileRefFilters.length > 0) {
    const normFiles = new Set(fileRefFilters);
    memories = memories.filter(
      (m) => m.references.some((r) => normFiles.has(r))
    );
  }
  if (refFilters.length > 0) {
    memories = memories.filter((m) => refFilters.every((ref) => m.references.includes(ref)));
  }
  if (compiledRegex.length > 0 || compiledFileRegex.length > 0) {
    memories = memories.filter((m) => {
      if (compiledFileRegex.length > 0) {
        const fileRefs = (m.references ?? []).filter((r) => r.startsWith("file:"));
        if (!compiledFileRegex.every((re) => fileRefs.some((r) => re.test(r)))) return false;
      }
      if (compiledRegex.length > 0) {
        const haystack = [
          m.task_context,
          m.observation,
          ...m.tags ?? [],
          ...m.references ?? [],
          m.label,
          m.workspace_path,
          m.artifact,
          m.repo,
          m.ref,
          m.failure_signature
        ].filter(Boolean).join(" ");
        if (!compiledRegex.every((re) => re.test(haystack))) return false;
      }
      return true;
    });
  }
  if (asOfDate) {
    memories = memories.filter((m) => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }
  if (sort === "importance") {
    memories.sort((a, b) => b.importance - a.importance || (b.score ?? 0) - (a.score ?? 0));
  } else if (sort === "recent") {
    memories.sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""));
  } else if (sort === "accessed") {
    memories.sort((a, b) => (b.last_accessed_at ?? b.created_at ?? "").localeCompare(a.last_accessed_at ?? a.created_at ?? ""));
  } else {
    memories.sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  }
  memories = memories.slice(0, limit);
  if (explain) {
    for (const m of memories) {
      const components = decayComponents(m, m.lexical ?? 0);
      m.score_components = components;
      m.score = components.final;
    }
  }
  bumpAccess(db3, memories.map((m) => m.memory_id));
  const mode = hasFts(db3) ? "lexical" : "fallback";
  const result = {
    count: memories.length,
    memories,
    mode,
    sort,
    as_of: asOf ?? null,
    global_only: Boolean(globalOnly),
    states
  };
  if (query.trim()) {
    const topRelevance = memories[0]?.lexical ?? 0;
    if (memories.length === 0) {
      result.judgment_required = true;
      result.judgment_reason = "no results \u2014 absence of recall is not proof of absence; retry with --smart or broader terms";
    } else if (mode === "fallback") {
      result.judgment_required = true;
      result.judgment_reason = "FTS unavailable \u2014 results are unranked substring matches; verify relevance before relying on them";
    } else if (topRelevance < JUDGMENT_RELEVANCE_FLOOR) {
      result.judgment_required = true;
      result.judgment_reason = `weak lexical match (top relevance ${topRelevance.toFixed(2)} < ${JUDGMENT_RELEVANCE_FLOOR}) \u2014 treat results as leads, not answers`;
    }
  }
  return result;
}
function forgetMemory(db3, params) {
  const { memoryIds = [], tags = [], before, dryRun = false } = params;
  let { maxImportance } = params;
  const scope = fillScope(
    {
      workspace_path: params.workspacePath ?? null,
      artifact: normalizeArtifact(params.artifact),
      repo: params.repo ?? null,
      ref: params.ref ?? null
    },
    params.cwd ?? params.workspacePath ?? process.cwd()
  );
  const selectorGroups = [];
  const bindParams = [];
  let salienceFloorApplied = false;
  if (memoryIds.length > 0) {
    selectorGroups.push(`memory_id IN (${memoryIds.map(() => "?").join(",")})`);
    bindParams.push(...memoryIds);
  }
  const attrConds = [];
  const attrBinds = [];
  if (tags.length > 0) {
    attrConds.push(
      `(${tags.map(() => "EXISTS (SELECT 1 FROM json_each(tags_json) WHERE value = ?)").join(" OR ")})`
    );
    attrBinds.push(...tags);
  }
  if (before) {
    attrConds.push("created_at < ?");
    attrBinds.push(before);
  }
  if (attrConds.length > 0 || maxImportance != null) {
    if (maxImportance == null) {
      maxImportance = SALIENCE_FLOOR - 1;
      salienceFloorApplied = true;
    }
    attrConds.push("importance <= ?");
    attrBinds.push(maxImportance);
    selectorGroups.push(`(${attrConds.join(" AND ")})`);
    bindParams.push(...attrBinds);
  }
  if (selectorGroups.length === 0) {
    throw new Error("forgetMemory requires at least one filter: memoryIds, tags, before, or maxImportance");
  }
  const scopeConds = [];
  const scopeBinds = [];
  if (params.workspacePath && scope.workspace_path) {
    scopeConds.push("workspace_path = ?");
    scopeBinds.push(scope.workspace_path);
  }
  if (params.artifact && scope.artifact) {
    scopeConds.push("artifact = ?");
    scopeBinds.push(scope.artifact);
  }
  if (params.repo && scope.repo) {
    scopeConds.push("repo = ?");
    scopeBinds.push(scope.repo);
  }
  if (params.ref && scope.ref) {
    scopeConds.push("ref = ?");
    scopeBinds.push(scope.ref);
  }
  const selectorWhere = selectorGroups.join(" OR ");
  const where = scopeConds.length > 0 ? `(${selectorWhere}) AND ${scopeConds.join(" AND ")}` : selectorWhere;
  const rows = db3.prepare(
    `SELECT memory_id FROM memories WHERE ${where}`
  ).all(...bindParams, ...scopeBinds);
  const ids = rows.map((r) => r.memory_id);
  if (dryRun) {
    return {
      deleted: 0,
      dry_run: true,
      would_delete: ids.length,
      memory_ids: ids,
      ...salienceFloorApplied ? { salience_floor: SALIENCE_FLOOR } : {}
    };
  }
  if (ids.length > 0) {
    const ph = ids.map(() => "?").join(",");
    db3.exec("BEGIN IMMEDIATE");
    try {
      db3.prepare(`DELETE FROM memories WHERE memory_id IN (${ph})`).run(...ids);
      if (hasFts(db3)) {
        db3.prepare(`DELETE FROM memories_fts WHERE memory_id IN (${ph})`).run(...ids);
      }
      try {
        db3.prepare(`DELETE FROM memory_refs WHERE memory_id IN (${ph})`).run(...ids);
      } catch {
      }
      db3.exec("COMMIT");
    } catch (e) {
      try {
        db3.exec("ROLLBACK");
      } catch {
      }
      throw e;
    }
  }
  return {
    deleted: ids.length,
    memory_ids: ids,
    ...salienceFloorApplied ? { salience_floor: SALIENCE_FLOOR } : {}
  };
}
function stripSurface(sig) {
  const idx = sig.indexOf("|surface:");
  return idx >= 0 ? sig.slice(0, idx) : sig;
}
function extractSurface(sig) {
  const idx = sig.indexOf("|surface:");
  return idx >= 0 ? sig.slice(idx + 9) : null;
}
function sigTokens(sig) {
  return new Set(
    sig.split(/[|:]+/).map((s) => s.trim().toLowerCase()).filter((s) => s.length > 2 && s !== "mechanism" && s !== "cause" && s !== "surface")
  );
}
function mineWeakness(db3, params = {}) {
  const { minCount = 2, limit = 20, cwd } = params;
  const wsPath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : cwd ? normalizeWorkspacePath(null, cwd) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const conditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
  const bindParams = [];
  if (wsPath) {
    conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
    bindParams.push(wsPath);
  }
  if (artifact2) {
    conditions.push("(artifact = ? OR artifact IS NULL)");
    bindParams.push(artifact2);
  }
  if (params.agentId) {
    conditions.push("agent_id = ?");
    bindParams.push(params.agentId);
  }
  const rows = db3.prepare(`
    SELECT failure_signature,
           count(*) AS freq,
           avg(importance) AS avg_imp,
           count(*) * avg(importance) AS score,
           group_concat(memory_id, ',') AS ids,
           group_concat(DISTINCT label) AS labels
    FROM memories
    WHERE ${conditions.join(" AND ")}
    GROUP BY failure_signature
    ORDER BY score DESC
  `).all(...bindParams);
  const mergedMap = /* @__PURE__ */ new Map();
  for (const row of rows) {
    const base = stripSurface(row.failure_signature);
    const surface = extractSurface(row.failure_signature);
    const existing = mergedMap.get(base);
    if (existing) {
      existing.total_freq += row.freq;
      existing.total_score += row.score;
      existing.importance_sum += row.avg_imp * row.freq;
      existing.ids.push(...row.ids.split(","));
      for (const l of row.labels.split(",").filter(Boolean)) existing.labels.add(l);
      if (surface) existing.surfaces.add(surface);
      if (row.score > existing.raw_score) {
        existing.raw_sig = row.failure_signature;
        existing.raw_score = row.score;
      }
    } else {
      mergedMap.set(base, {
        base_sig: base,
        raw_sig: row.failure_signature,
        total_freq: row.freq,
        total_score: row.score,
        importance_sum: row.avg_imp * row.freq,
        ids: row.ids.split(","),
        labels: new Set(row.labels.split(",").filter(Boolean)),
        surfaces: new Set(surface ? [surface] : []),
        raw_score: row.score
      });
    }
  }
  const merged = [...mergedMap.values()].filter((m) => m.total_freq >= minCount).sort((a, b) => b.total_score - a.total_score);
  const repMap = /* @__PURE__ */ new Map();
  const allRawSigs = merged.map((m) => m.raw_sig);
  if (allRawSigs.length > 0) {
    const ph = allRawSigs.map(() => "?").join(",");
    const repRows = db3.prepare(
      `SELECT failure_signature, observation, max(importance)
       FROM memories
       WHERE failure_signature IN (${ph}) AND ${conditions.join(" AND ")}
       GROUP BY failure_signature`
    ).all(...allRawSigs, ...bindParams);
    for (const r of repRows) repMap.set(stripSurface(r.failure_signature), r.observation);
  }
  const selected = [];
  for (const m of merged) {
    if (selected.length >= limit) break;
    const toks = sigTokens(m.base_sig);
    const tooSimilar = selected.some(
      (sel) => jaccard(sigTokens(sel.base_signature), toks) >= 0.5
    );
    if (tooSimilar) continue;
    selected.push({
      failure_signature: m.raw_sig,
      base_signature: m.base_sig,
      surfaces: [...m.surfaces].sort(),
      count: m.total_freq,
      avg_importance: Math.round(m.importance_sum / m.total_freq * 10) / 10,
      score: Math.round(m.total_score * 10) / 10,
      memory_ids: [...new Set(m.ids)],
      representative: (repMap.get(m.base_sig) ?? "").slice(0, 200),
      labels: [...m.labels].sort()
    });
  }
  const totals = db3.prepare(
    `SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
     FROM memories WHERE ${conditions.join(" AND ")}`
  ).get(...bindParams);
  const next = selected.length > 0 ? "Next: choose one cluster, inspect its memory_ids, implement one scoped fix, verify it, then run octocode-awareness reflect record with the same --failure-signature and either --fix-repo or --fix-harness." : "No recurring failure cluster met the threshold. Record verified failures with octocode-awareness reflect record --failure-signature <signature>, then mine again after repetition.";
  return { ok: true, clusters: selected, total_signatures: totals.sigs, total_memories: totals.mems, next };
}
function cosineSimilarity(a, b) {
  if (a.length !== b.length || a.length === 0) return 0;
  let dot = 0, normA = 0, normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom === 0 ? 0 : dot / denom;
}
function storeEmbedding(db3, memoryId, embedding, model) {
  const blob = Buffer.from(embedding.buffer, embedding.byteOffset, embedding.byteLength);
  db3.prepare(
    `UPDATE memories SET embedding = ?, embedding_model = ?, updated_at = ?
     WHERE memory_id = ?`
  ).run(blob, model, utcNow(), memoryId);
}
function searchByEmbedding(db3, queryEmbedding, limit = 5, threshold = 0.75, model) {
  const conditions = ["state = 'ACTIVE'", "embedding IS NOT NULL"];
  const binds = [];
  if (model) {
    conditions.push("embedding_model = ?");
    binds.push(model);
  }
  const rows = db3.prepare(
    `SELECT memory_id, embedding, embedding_model FROM memories
     WHERE ${conditions.join(" AND ")}
     ORDER BY COALESCE(last_accessed_at, created_at) DESC
     LIMIT 2000`
  ).all(...binds);
  const results = [];
  for (const row of rows) {
    try {
      const stored = new Float32Array(
        row.embedding.buffer,
        row.embedding.byteOffset,
        row.embedding.byteLength / 4
      );
      const sim = cosineSimilarity(queryEmbedding, stored);
      if (sim >= threshold) results.push({ memory_id: row.memory_id, similarity: sim });
    } catch {
    }
  }
  return results.sort((a, b) => b.similarity - a.similarity).slice(0, limit);
}

// src/embed-host.ts
import { spawnSync as spawnSync2 } from "node:child_process";
function resolveEmbedCommand(env = process.env) {
  const raw = env["OCTOCODE_EMBED_CMD"];
  if (typeof raw !== "string") return null;
  const cmd = raw.trim();
  return cmd.length > 0 ? cmd : null;
}
function runHostEmbedder(text, options = {}) {
  const command2 = options.command ?? resolveEmbedCommand(options.env);
  if (!command2) {
    throw new Error("OCTOCODE_EMBED_CMD is not set");
  }
  const timeoutMs = options.timeoutMs ?? 15e3;
  const done = spawnSync2(command2, {
    input: text,
    encoding: "utf8",
    shell: true,
    timeout: timeoutMs,
    env: options.env ?? process.env,
    maxBuffer: 8 * 1024 * 1024
  });
  if (done.error) {
    throw new Error(`OCTOCODE_EMBED_CMD failed to start: ${done.error.message}`);
  }
  if (done.status !== 0) {
    const err = (done.stderr || done.stdout || "").trim().slice(0, 400);
    throw new Error(`OCTOCODE_EMBED_CMD exited ${done.status}${err ? `: ${err}` : ""}`);
  }
  const stdout = (done.stdout || "").trim();
  if (!stdout) throw new Error("OCTOCODE_EMBED_CMD returned empty stdout");
  let parsed;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    throw new Error("OCTOCODE_EMBED_CMD stdout is not JSON");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("OCTOCODE_EMBED_CMD JSON must be an object with embedding[]");
  }
  const record = parsed;
  const values = record["embedding"];
  if (!Array.isArray(values) || values.length === 0 || !values.every((v) => typeof v === "number" && Number.isFinite(v))) {
    throw new Error("OCTOCODE_EMBED_CMD embedding must be a non-empty number[]");
  }
  const modelRaw = record["model"];
  const model = typeof modelRaw === "string" && modelRaw.trim() ? modelRaw.trim() : "host-embed";
  return { embedding: Float32Array.from(values), model };
}

// src/docs.ts
import { resolve as resolve4 } from "node:path";

// src/audit.ts
import { randomUUID as randomUUID2 } from "node:crypto";
import { createHash } from "node:crypto";

// src/sql/audit.ts
var EDIT_LOG_INSERT = `
  INSERT INTO edit_log (
    edit_id, session_id, run_id, agent_id,
    file_path, operation, old_file_path,
    lines_added, lines_removed, content_hash,
    workspace_path, artifact, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;
var HARNESS_LOG_INSERT = `
  INSERT INTO harness_log (
    harness_id, session_id, agent_id, workspace_path, artifact, event_type,
    payload_json, memory_id, run_id, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;

// src/audit.ts
function insertEditLog(db3, params) {
  const editId = "edit_" + randomUUID2();
  const now = utcNow();
  db3.prepare(EDIT_LOG_INSERT).run(
    editId,
    params.sessionId ?? null,
    params.runId ?? null,
    params.agentId,
    params.filePath,
    params.operation,
    params.oldFilePath ?? null,
    params.linesAdded ?? null,
    params.linesRemoved ?? null,
    params.contentHash ?? null,
    params.workspacePath ?? null,
    normalizeArtifact(params.artifact),
    now
  );
  return { editId };
}
function insertHarnessLog(db3, params) {
  const harnessId = "harness_" + randomUUID2();
  const now = utcNow();
  const payloadJson = params.payload !== void 0 ? JSON.stringify(params.payload) : null;
  db3.prepare(HARNESS_LOG_INSERT).run(
    harnessId,
    params.sessionId ?? null,
    params.agentId,
    params.workspacePath ?? null,
    normalizeArtifact(params.artifact),
    params.eventType,
    payloadJson,
    params.memoryId ?? null,
    params.runId ?? null,
    now
  );
  return harnessId;
}

// src/docs.ts
var DEFAULT_MIN_EDITS_SINCE_SYNC = 5;
var DEFAULT_MIN_LINES_SINCE_SYNC = 50;
function lastEditTimestamp(db3, filePath, workspacePath, artifact2) {
  const conditions = ["file_path = ?"];
  const binds = [filePath];
  if (workspacePath) {
    conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
    binds.push(workspacePath);
  }
  if (artifact2) {
    conditions.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact2);
  }
  const row = db3.prepare(
    `SELECT MAX(created_at) AS ts FROM edit_log WHERE ${conditions.join(" AND ")}`
  ).get(...binds);
  return row?.ts ?? null;
}
function sourceActivitySince(db3, sourceDirs, since, workspacePath, artifact2) {
  if (sourceDirs.length === 0) return { edits: 0, linesChanged: 0, files: [], latest: null };
  const conditions = [];
  const binds = [];
  const dirClauses = sourceDirs.map(() => "file_path LIKE ?");
  conditions.push(`(${dirClauses.join(" OR ")})`);
  binds.push(...sourceDirs.map((d) => `${d.replace(/\/+$/, "")}/%`));
  if (since) {
    conditions.push("created_at > ?");
    binds.push(since);
  }
  if (workspacePath) {
    conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
    binds.push(workspacePath);
  }
  if (artifact2) {
    conditions.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact2);
  }
  const rows = db3.prepare(
    `SELECT file_path, lines_added, lines_removed, created_at
     FROM edit_log WHERE ${conditions.join(" AND ")}`
  ).all(...binds);
  const files = [...new Set(rows.map((r) => r.file_path))];
  const linesChanged = rows.reduce((sum, r) => sum + (r.lines_added ?? 0) + (r.lines_removed ?? 0), 0);
  const latest = rows.reduce(
    (max, r) => !max || r.created_at > max ? r.created_at : max,
    null
  );
  return { edits: rows.length, linesChanged, files, latest };
}
function mineDocStaleness(db3, params) {
  const minEdits = params.minEditsSinceSync ?? DEFAULT_MIN_EDITS_SINCE_SYNC;
  const minLines = params.minLinesSinceSync ?? DEFAULT_MIN_LINES_SINCE_SYNC;
  const workspacePath = params.workspacePath ?? null;
  const artifact2 = normalizeArtifact(params.artifact);
  const base = workspacePath ?? process.cwd();
  const entries = params.targets.map((target) => {
    const resolvedDoc = resolve4(base, target.docFile);
    const resolvedDirs = target.sourceDirs.map((d) => resolve4(base, d));
    const docLastSyncedAt = lastEditTimestamp(db3, resolvedDoc, workspacePath, artifact2);
    const activity = sourceActivitySince(db3, resolvedDirs, docLastSyncedAt, workspacePath, artifact2);
    const stale = activity.edits >= minEdits || activity.linesChanged >= minLines;
    return {
      doc_file: target.docFile,
      source_dirs: target.sourceDirs,
      doc_last_synced_at: docLastSyncedAt,
      edits_since_sync: activity.edits,
      lines_changed_since_sync: activity.linesChanged,
      files_touched: activity.files,
      latest_source_edit_at: activity.latest,
      stale
    };
  });
  return {
    ok: true,
    checked: entries.length,
    stale_count: entries.filter((e) => e.stale).length,
    entries
  };
}
function proposeDocRefresh(db3, entry2, params) {
  const sinceLabel = entry2.doc_last_synced_at ?? "doc was last tracked (no prior edit_log record)";
  return insertHarnessLog(db3, {
    agentId: params.agentId,
    sessionId: params.sessionId ?? null,
    workspacePath: params.workspacePath ?? null,
    artifact: params.artifact ?? null,
    eventType: "propose",
    payload: {
      failure_signature: "doc-staleness",
      target_file: entry2.doc_file,
      proposed_change: `Refresh ${entry2.doc_file} \u2014 ${entry2.edits_since_sync} edit(s) / ${entry2.lines_changed_since_sync} line(s) changed across ${entry2.source_dirs.join(", ")} since ${sinceLabel}.`,
      evidence: {
        edits_since_sync: entry2.edits_since_sync,
        lines_changed_since_sync: entry2.lines_changed_since_sync,
        files_touched: entry2.files_touched,
        doc_last_synced_at: entry2.doc_last_synced_at,
        latest_source_edit_at: entry2.latest_source_edit_at
      }
    }
  });
}

// src/docs-catalog.ts
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { basename as basename2, dirname as dirname3, join as join3 } from "node:path";
import { fileURLToPath } from "node:url";
function packageSkillReferencesDir(cwd = process.cwd()) {
  const here = dirname3(fileURLToPath(import.meta.url));
  const candidates = [
    join3(here, "..", "skills", "octocode-awareness", "references"),
    // dist/skills
    join3(here, "..", "..", "skills", "octocode-awareness", "references"),
    // package root
    join3(cwd, "packages", "octocode-awareness", "skills", "octocode-awareness", "references"),
    join3(cwd, "skills", "octocode-awareness", "references")
  ];
  return candidates.find((candidate) => existsSync(candidate)) ?? candidates[0];
}
function firstParagraph(lines, start = 0) {
  const chunks = [];
  let i = start;
  while (i < lines.length && !lines[i].trim()) i++;
  while (i < lines.length) {
    const line = lines[i].trim();
    if (!line) break;
    if (line.startsWith("#") || line.startsWith("```") || line.startsWith("|") || line.startsWith("- ")) break;
    chunks.push(line);
    i++;
  }
  return chunks.join(" ").replace(/\s+/g, " ").trim();
}
function parseDocFile(filePath) {
  const name = basename2(filePath, ".md");
  const raw = readFileSync(filePath, "utf8");
  const lines = raw.split(/\r?\n/);
  let title = name;
  let bodyStart = 0;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line.startsWith("# ")) {
      title = line.slice(2).trim();
      bodyStart = i + 1;
      break;
    }
  }
  let description = firstParagraph(lines, bodyStart);
  if (!description) {
    const whenLine = lines.find((line) => /^(use|read|load)\s+this\b/i.test(line.trim()));
    description = whenLine?.trim() ?? `${title} skill reference.`;
  }
  if (description.length > 180) description = `${description.slice(0, 177).trimEnd()}...`;
  return {
    name,
    title,
    description,
    kind: "skill-ref",
    path: filePath
  };
}
function listSkillDocs(options = {}) {
  const root = options.root ?? packageSkillReferencesDir(options.cwd);
  const lean = Boolean(options.lean);
  if (!existsSync(root)) {
    return { ok: true, count: 0, docs: [], root };
  }
  const docs = readdirSync(root).filter((name) => name.endsWith(".md")).sort((a, b) => a.localeCompare(b)).map((name) => parseDocFile(join3(root, name)));
  return {
    ok: true,
    count: docs.length,
    docs: lean ? docs.map((doc) => ({ name: doc.name, title: doc.title })) : docs,
    root
  };
}
function showSkillDoc(nameOrPath, options = {}) {
  const list = listSkillDocs(options);
  const needle = nameOrPath.replace(/\.md$/i, "").trim().toLowerCase();
  const match = list.docs.find((doc) => doc.name.toLowerCase() === needle) ?? list.docs.find((doc) => doc.title.toLowerCase() === needle);
  if (!match) {
    const suggestions = list.docs.filter((doc) => doc.name.includes(needle) || doc.title.toLowerCase().includes(needle)).map((doc) => doc.name).slice(0, 5);
    return {
      ok: false,
      error: `unknown doc "${nameOrPath}". Run docs list --compact.`,
      suggestions: suggestions.length > 0 ? suggestions : list.docs.slice(0, 5).map((doc) => doc.name)
    };
  }
  const content = readFileSync(match.path, "utf8");
  return {
    ok: true,
    name: match.name,
    title: match.title,
    description: match.description,
    kind: match.kind,
    path: match.path,
    content
  };
}

// src/refinements.ts
import { randomUUID as randomUUID3 } from "node:crypto";

// src/sql/refinements.ts
var COLS = "refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json, reasoning, remember, quality, state, created_at, updated_at";
var REFINEMENTS_INSERT = `INSERT INTO refinements (
     refinement_id, agent_id, workspace_path, artifact, repo, ref,
     files_json, reasoning, remember, quality, state, created_at, updated_at
   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`;
var REFINEMENTS_SELECT_OPEN = `SELECT ${COLS} FROM refinements
   WHERE state IN ('open','ongoing') AND quality NOT IN ('handoff','instructions')
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;
var REFINEMENTS_SELECT_BY_WORKSPACE = `SELECT ${COLS} FROM refinements
   WHERE (workspace_path = ? OR workspace_path IS NULL)
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;
var REFINEMENTS_DELETE = `DELETE FROM refinements WHERE refinement_id IN `;

// src/refinements.ts
function insertRefinement(db3, params) {
  const {
    agentId: agentId2 = "agent",
    reasoning,
    remember,
    quality = "good",
    state = "open",
    workspacePath,
    artifact: artifact2,
    repo: repoArg,
    ref: refArg,
    files = [],
    cwd
  } = params;
  const refinementId = "ref_" + randomUUID3().replace(/-/g, "");
  const now = utcNow();
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact2), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  db3.prepare(REFINEMENTS_INSERT).run(
    refinementId,
    agentId2,
    scope.workspace_path ?? process.cwd(),
    scope.artifact,
    scope.repo ?? null,
    scope.ref ?? null,
    JSON.stringify(files),
    reasoning,
    remember,
    quality,
    state,
    now,
    now
  );
  return {
    refinementId,
    refinement: {
      refinement_id: refinementId,
      agent_id: agentId2,
      workspace_path: scope.workspace_path ?? process.cwd(),
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      files,
      reasoning,
      remember,
      quality,
      state,
      created_at: now,
      updated_at: now
    }
  };
}
function getRefinements(db3, params = {}) {
  const {
    workspacePath,
    artifact: artifact2,
    repo: repoArg,
    ref: refArg,
    quality,
    includeHandoffs = false,
    states: statesRaw,
    limit: limitRaw = 10,
    cwd
  } = params;
  const limit = Math.min(50, Math.max(1, Number(limitRaw) || 10));
  const states = statesRaw ?? ["open", "ongoing"];
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact2), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const queryParams = [...states];
  const stateFilter = `state IN (${states.map(() => "?").join(",")})`;
  let sql = `SELECT * FROM refinements WHERE ${stateFilter}`;
  if (quality) {
    sql += " AND quality = ?";
    queryParams.push(quality);
  } else if (!includeHandoffs) {
    sql += " AND quality NOT IN ('handoff', 'instructions')";
  }
  if (scope.workspace_path) {
    sql += " AND (workspace_path = ? OR workspace_path IS NULL)";
    queryParams.push(scope.workspace_path);
  }
  if (scope.artifact) {
    sql += " AND (artifact = ? OR artifact IS NULL)";
    queryParams.push(scope.artifact);
  }
  if (scope.repo) {
    sql += " AND (repo = ? OR repo IS NULL)";
    queryParams.push(scope.repo);
  }
  if (scope.ref) {
    sql += " AND (ref = ? OR ref IS NULL)";
    queryParams.push(scope.ref);
  }
  sql += ` ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC LIMIT ?`;
  queryParams.push(limit);
  let handoffCount;
  let instructionsCount;
  if (!quality && !includeHandoffs) {
    const countParams = [...states];
    let scopeSql = "";
    if (scope.workspace_path) {
      scopeSql += " AND (workspace_path = ? OR workspace_path IS NULL)";
      countParams.push(scope.workspace_path);
    }
    if (scope.artifact) {
      scopeSql += " AND (artifact = ? OR artifact IS NULL)";
      countParams.push(scope.artifact);
    }
    if (scope.repo) {
      scopeSql += " AND (repo = ? OR repo IS NULL)";
      countParams.push(scope.repo);
    }
    if (scope.ref) {
      scopeSql += " AND (ref = ? OR ref IS NULL)";
      countParams.push(scope.ref);
    }
    const rows2 = db3.prepare(
      `SELECT quality, COUNT(*) AS c FROM refinements
        WHERE ${stateFilter} AND quality IN ('handoff', 'instructions') ${scopeSql}
        GROUP BY quality`
    ).all(...countParams);
    const byQuality = new Map(rows2.map((r) => [r.quality, Number(r.c)]));
    handoffCount = byQuality.get("handoff") ?? 0;
    instructionsCount = byQuality.get("instructions") ?? 0;
  }
  const rows = db3.prepare(sql).all(...queryParams);
  const refinements = rows.map((r) => ({
    refinement_id: r.refinement_id,
    agent_id: r.agent_id,
    workspace_path: r.workspace_path,
    artifact: r.artifact ?? null,
    repo: r.repo,
    ref: r.ref,
    files: parseJsonList(r.files_json),
    reasoning: r.reasoning,
    remember: r.remember,
    quality: r.quality,
    state: r.state,
    created_at: r.created_at,
    updated_at: r.updated_at
  }));
  return {
    count: refinements.length,
    refinements,
    ...handoffCount !== void 0 ? { handoff_count: handoffCount } : {},
    ...instructionsCount !== void 0 ? { instructions_count: instructionsCount } : {}
  };
}
function updateRefinement(db3, params) {
  const { refinementId, state, quality, reasoning, remember, files } = params;
  const sets = [];
  const binds = [];
  if (state !== void 0) {
    sets.push("state = ?");
    binds.push(state);
  }
  if (quality !== void 0) {
    sets.push("quality = ?");
    binds.push(quality);
  }
  if (reasoning !== void 0) {
    sets.push("reasoning = ?");
    binds.push(reasoning);
  }
  if (remember !== void 0) {
    sets.push("remember = ?");
    binds.push(remember);
  }
  if (files !== void 0) {
    sets.push("files_json = ?");
    binds.push(JSON.stringify(files));
  }
  if (sets.length === 0) throw new Error("updateRefinement: no fields to update");
  sets.push("updated_at = ?");
  binds.push(utcNow());
  const r = db3.prepare(
    `UPDATE refinements SET ${sets.join(", ")} WHERE refinement_id = ?`
  ).run(...binds, refinementId);
  if (r.changes === 0) return { updated: false, refinement: null };
  const row = db3.prepare("SELECT * FROM refinements WHERE refinement_id = ?").get(refinementId);
  return {
    updated: true,
    refinement: {
      refinement_id: row.refinement_id,
      agent_id: row.agent_id,
      workspace_path: row.workspace_path,
      artifact: row.artifact ?? null,
      repo: row.repo,
      ref: row.ref,
      files: parseJsonList(row.files_json),
      reasoning: row.reasoning,
      remember: row.remember,
      quality: row.quality,
      state: row.state,
      created_at: row.created_at,
      updated_at: row.updated_at
    }
  };
}
function deleteRefinement(db3, params) {
  const { refinementIds, workspacePath, dryRun = false } = params;
  if (refinementIds.length === 0) {
    return { deleted: 0, refinement_ids: [] };
  }
  const ph = refinementIds.map(() => "?").join(",");
  const where = [`refinement_id IN (${ph})`];
  const binds = [...refinementIds];
  if (workspacePath) {
    where.push("(workspace_path = ? OR workspace_path IS NULL)");
    binds.push(workspacePath);
  }
  const artifact2 = normalizeArtifact(params.artifact);
  if (artifact2) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact2);
  }
  const rows = db3.prepare(
    `SELECT refinement_id FROM refinements WHERE ${where.join(" AND ")}`
  ).all(...binds);
  const ids = rows.map((r) => r.refinement_id);
  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, refinement_ids: ids };
  }
  if (ids.length > 0) {
    const delPh = ids.map(() => "?").join(",");
    db3.prepare(`${REFINEMENTS_DELETE}(${delPh})`).run(...ids);
  }
  return { deleted: ids.length, refinement_ids: ids };
}

// src/intents.ts
import { randomUUID as randomUUID5 } from "node:crypto";
import { isAbsolute as isAbsolute2, resolve as resolve6 } from "node:path";

// src/work.ts
import { randomUUID as randomUUID4 } from "node:crypto";
import { isAbsolute, resolve as resolve5 } from "node:path";
var DEFAULT_PRESENCE_TTL_MS = 10 * 6e4;
var MAX_PRESENCE_TTL_MS = 60 * 6e4;
var PEER_DETAIL_LIMIT = 5;
function required(value, name) {
  const normalized = value?.trim() ?? "";
  if (!normalized) throw new Error(`${name} is required`);
  return normalized;
}
function workspaceRoot(workspacePath) {
  const candidate = workspacePath ?? process.cwd();
  return normalizeWorkspacePath(candidate, candidate) ?? resolve5(candidate);
}
function normalizeFiles(files, workspacePath) {
  if (files.length === 0) throw new Error("at least one target file is required");
  const base = canonicalizePath(workspacePath ? resolve5(workspacePath) : process.cwd());
  return [...new Set(files.map((file) => {
    const value = required(file, "target file");
    return canonicalizePath(isAbsolute(value) ? resolve5(value) : resolve5(base, value));
  }))];
}
function expiry(ttlMs) {
  const effective = Math.min(Math.max(1, ttlMs ?? DEFAULT_PRESENCE_TTL_MS), MAX_PRESENCE_TTL_MS);
  return new Date(Date.now() + effective).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function getRun(db3, runId) {
  const row = db3.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(runId);
  if (!row) throw new Error(`run not found: ${runId}`);
  return row;
}
function fileRows(db3, runId) {
  return db3.prepare("SELECT * FROM run_files WHERE run_id = ? ORDER BY file_path").all(runId);
}
function activePeerRows(db3, runId, files) {
  if (files.length === 0) return [];
  const now = utcNow();
  const rows = db3.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
      tr.rationale, rf.heartbeat_at, rf.expires_at,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive
    FROM run_files rf
    JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE rf.run_id <> ?
      AND rf.file_path IN (${files.map(() => "?").join(",")})
      AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id`).all(now, runId, ...files, now);
  return rows.map((row) => ({ ...row, exclusive: Boolean(row.exclusive) }));
}
function mutationResult(db3, runId, affectedFiles) {
  const allFiles = fileRows(db3, runId);
  const affected = affectedFiles ? new Set(affectedFiles) : null;
  const files = affected ? allFiles.filter((file) => affected.has(file.file_path)) : allFiles;
  const allPeers = activePeerRows(db3, runId, files.filter((file) => file.ended_at == null).map((file) => file.file_path));
  return {
    run: getRun(db3, runId),
    files,
    peers: allPeers.slice(0, PEER_DETAIL_LIMIT),
    peer_count: allPeers.length
  };
}
function conflictRows(db3, runId, files, exclusive) {
  if (files.length === 0) return [];
  const now = utcNow();
  const placeholders = files.map(() => "?").join(",");
  if (exclusive) {
    return db3.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
        tr.rationale, rf.heartbeat_at, rf.expires_at,
        EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
          AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
        'ACTIVE_WORK' AS conflict_type
      FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
      WHERE rf.file_path IN (${placeholders}) AND rf.run_id <> ?
        AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
      ORDER BY rf.file_path, rf.heartbeat_at DESC`).all(now, ...files, runId, now);
  }
  return db3.prepare(`SELECT l.run_id, tr.task_id, tr.origin, tr.agent_id, l.file_path,
      tr.rationale, rf.heartbeat_at, COALESCE(l.expires_at, rf.expires_at) AS expires_at,
      1 AS exclusive, 'EXCLUSIVE_LOCK' AS conflict_type
    FROM locks l
    JOIN task_runs tr ON tr.run_id = l.run_id
    LEFT JOIN run_files rf ON rf.run_id = l.run_id AND rf.file_path = l.file_path
    WHERE l.file_path IN (${placeholders}) AND l.run_id <> ? AND tr.status = 'ACTIVE'
      AND (l.expires_at IS NULL OR l.expires_at > ?)
    ORDER BY l.file_path, l.acquired_at DESC`).all(...files, runId, now);
}
function reusableWorkRun(db3, agentId2, files, now) {
  const rows = db3.prepare(`SELECT DISTINCT tr.run_id
    FROM task_runs tr JOIN run_files rf ON rf.run_id = tr.run_id
    WHERE tr.agent_id = ? AND tr.origin = 'WORK' AND tr.status = 'ACTIVE'
      AND rf.file_path IN (${files.map(() => "?").join(",")})
      AND rf.ended_at IS NULL AND rf.expires_at > ?`).all(agentId2, ...files, now);
  if (rows.length > 1) throw new Error("target files match multiple active WORK runs; pass --run-id");
  return rows[0]?.run_id ?? null;
}
function startWork(db3, params) {
  const agentId2 = required(params.agentId, "agent id");
  const files = normalizeFiles(params.targetFiles, params.workspacePath);
  const now = utcNow();
  const expiresAt = expiry(params.ttlMs);
  const requestedOrigin = params.origin ?? "WORK";
  const source = params.source ?? (requestedOrigin === "HOOK" ? "HOOK" : "EXPLICIT");
  const wsPath = workspaceRoot(params.workspacePath);
  const artifact2 = normalizeArtifact(params.artifact);
  let runId = params.runId ?? null;
  if (!runId) {
    required(params.rationale, "rationale");
    required(params.testPlan, "test plan");
  }
  db3.exec("BEGIN IMMEDIATE");
  try {
    if (!runId && requestedOrigin === "WORK") runId = reusableWorkRun(db3, agentId2, files, now);
    runId ??= `run_${randomUUID4().replace(/-/g, "")}`;
    let run = db3.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(runId);
    if (run) {
      if (run.agent_id !== agentId2) throw new Error(`run ${runId} belongs to ${run.agent_id}`);
      if (run.status !== "ACTIVE") throw new Error(`run ${runId} is not ACTIVE`);
    } else {
      if (params.sessionId) {
        db3.prepare(`INSERT OR IGNORE INTO sessions
          (session_id, agent_id, workspace_path, artifact, started_at) VALUES (?, ?, ?, ?, ?)`).run(params.sessionId, agentId2, wsPath, artifact2, now);
      }
      db3.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, session_id, rationale, test_plan, context_ref,
         status, workspace_path, artifact, created_at, updated_at)
        VALUES (?, NULL, ?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)`).run(
        runId,
        requestedOrigin,
        agentId2,
        params.sessionId ?? null,
        required(params.rationale, "rationale"),
        required(params.testPlan, "test plan"),
        params.contextRef ?? null,
        wsPath,
        artifact2,
        now,
        now
      );
      run = getRun(db3, runId);
    }
    const conflicts = conflictRows(db3, runId, files, params.exclusive === true);
    if (conflicts.length > 0) {
      db3.exec("ROLLBACK");
      return { ok: false, conflict: true, conflicts };
    }
    const upsert = db3.prepare(`INSERT INTO run_files
      (run_id, file_path, reason_override, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, NULL)
      ON CONFLICT(run_id, file_path) DO UPDATE SET
        reason_override = COALESCE(excluded.reason_override, run_files.reason_override),
        source = excluded.source,
        started_at = CASE WHEN run_files.ended_at IS NULL THEN run_files.started_at ELSE excluded.started_at END,
        heartbeat_at = excluded.heartbeat_at,
        expires_at = excluded.expires_at,
        ended_at = NULL`);
    for (const file of files) {
      upsert.run(runId, file, params.reasonOverride?.trim() || null, source, now, now, expiresAt);
      if (params.exclusive) {
        db3.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`).run(`lock_${randomUUID4().replace(/-/g, "")}`, file, runId, now, expiresAt);
      }
    }
    db3.prepare("UPDATE task_runs SET updated_at = ? WHERE run_id = ?").run(now, runId);
    db3.exec("COMMIT");
    return { ok: true, ...mutationResult(db3, runId, files) };
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
}
function touchWork(db3, params) {
  const run = getRun(db3, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.status !== "ACTIVE") throw new Error(`run ${params.runId} is not ACTIVE`);
  const targets = params.targetFiles?.length ? normalizeFiles(params.targetFiles, run.workspace_path) : fileRows(db3, params.runId).filter((file) => file.ended_at == null).map((file) => file.file_path);
  if (targets.length === 0) throw new Error("run has no active file presence");
  const now = utcNow();
  const expiresAt = expiry(params.ttlMs);
  db3.exec("BEGIN IMMEDIATE");
  try {
    const update = db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?
      WHERE run_id = ? AND file_path = ? AND ended_at IS NULL`);
    for (const file of targets) {
      const result = update.run(now, expiresAt, params.runId, file);
      if (result.changes === 0) throw new Error(`active file presence not found: ${file}`);
    }
    db3.prepare(`UPDATE locks SET expires_at = ? WHERE run_id = ?
      AND file_path IN (${targets.map(() => "?").join(",")})`).run(expiresAt, params.runId, ...targets);
    db3.prepare("UPDATE task_runs SET updated_at = ? WHERE run_id = ?").run(now, params.runId);
    db3.exec("COMMIT");
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return mutationResult(db3, params.runId, targets);
}
function endWork(db3, params) {
  const run = getRun(db3, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.origin === "TASK") throw new Error("TASK work must end through task submit or task release");
  const targets = params.targetFiles?.length ? normalizeFiles(params.targetFiles, run.workspace_path) : fileRows(db3, params.runId).filter((file) => file.ended_at == null).map((file) => file.file_path);
  const now = utcNow();
  db3.exec("BEGIN IMMEDIATE");
  try {
    if (targets.length > 0) {
      const ended = db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND file_path IN (${targets.map(() => "?").join(",")}) AND ended_at IS NULL`).run(now, now, now, params.runId, ...targets);
      if (ended.changes !== targets.length) {
        throw new Error("one or more active file presences were not found for this run");
      }
      db3.prepare(`DELETE FROM locks WHERE run_id = ?
        AND file_path IN (${targets.map(() => "?").join(",")})`).run(params.runId, ...targets);
    }
    const active = db3.prepare(`SELECT 1 FROM run_files
      WHERE run_id = ? AND ended_at IS NULL AND expires_at > ? LIMIT 1`).get(params.runId, now);
    if (!active) {
      db3.prepare(`UPDATE task_runs SET status = 'PENDING', updated_at = ?
        WHERE run_id = ? AND status = 'ACTIVE' AND origin IN ('WORK','HOOK')`).run(now, params.runId);
    }
    db3.exec("COMMIT");
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return mutationResult(db3, params.runId, targets);
}
function listWork(db3, params = {}) {
  const now = utcNow();
  const where = ["1 = 1"];
  const binds = [now];
  if (params.activeOnly !== false) {
    where.push("rf.ended_at IS NULL", "rf.expires_at > ?", "tr.status = 'ACTIVE'");
    binds.push(now);
  }
  if (params.workspacePath) {
    where.push("tr.workspace_path = ?");
    binds.push(workspaceRoot(params.workspacePath));
  }
  const artifact2 = normalizeArtifact(params.artifact);
  if (artifact2) {
    where.push("(tr.artifact = ? OR tr.artifact IS NULL)");
    binds.push(artifact2);
  }
  if (params.agentId) {
    where.push("tr.agent_id = ?");
    binds.push(params.agentId);
  }
  if (params.runId) {
    where.push("tr.run_id = ?");
    binds.push(params.runId);
  }
  if (params.filePath) {
    where.push("rf.file_path = ?");
    binds.push(normalizeFiles([params.filePath], params.workspacePath)[0]);
  }
  const rows = db3.prepare(`SELECT rf.*, tr.task_id, tr.origin, tr.agent_id, tr.session_id,
      tr.rationale, tr.test_plan, tr.status, tr.workspace_path, tr.artifact,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive
    FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE ${where.join(" AND ")}
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id`).all(...binds);
  const files = rows.map((row) => ({ ...row, exclusive: Boolean(row.exclusive) }));
  return { count: files.length, files };
}
function showWork(db3, params) {
  return listWork(db3, { ...params, filePath: params.filePath });
}

// src/intents.ts
var MAX_LOCK_TTL_MS = 10 * 6e4;
var VALID_RELEASE_STATUSES = /* @__PURE__ */ new Set(["PENDING", "ACTIVE", "SUCCESS", "FAILED"]);
function effectiveTtlMs(ttlMs) {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}
function workspaceScopeRoot(workspacePath) {
  const candidate = workspacePath ?? process.cwd();
  return normalizeWorkspacePath(candidate, candidate) ?? resolve6(candidate);
}
function workspaceFileBase(workspacePath) {
  return workspacePath ? resolve6(workspacePath) : process.cwd();
}
function resolveTargetFiles(targetFiles = [], workspacePath) {
  const root = workspaceFileBase(workspacePath);
  return targetFiles.map((file) => canonicalizePath(isAbsolute2(file) ? resolve6(file) : resolve6(root, file)));
}
function activeLockRows(db3, params = {}) {
  evictExpiredLocks(db3);
  const now = utcNow();
  const clauses = ["ai.status = 'ACTIVE'", "(fl.expires_at IS NULL OR fl.expires_at > ?)"];
  const binds = [now];
  if (params.workspacePath) {
    clauses.push("ai.workspace_path = ?");
    binds.push(workspaceScopeRoot(params.workspacePath));
  }
  const artifact2 = normalizeArtifact(params.artifact);
  if (artifact2) {
    clauses.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    binds.push(artifact2);
  }
  if (params.agentId) {
    clauses.push("ai.agent_id = ?");
    binds.push(params.agentId);
  }
  if (params.sessionId) {
    clauses.push("ai.session_id = ?");
    binds.push(params.sessionId);
  }
  if (params.runId) {
    clauses.push("fl.run_id = ?");
    binds.push(params.runId);
  }
  return db3.prepare(
    `SELECT fl.lock_id, fl.run_id, fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact,
            ai.rationale AS reasoning, ai.test_plan AS test_plan, 'EXCLUSIVE' AS lock_type,
            fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN task_runs ai ON ai.run_id = fl.run_id
      WHERE ${clauses.join(" AND ")}
      ORDER BY fl.acquired_at DESC`
  ).all(...binds);
}
function preFlightIntent(db3, params) {
  const agentId2 = params.agentId ?? "agent";
  const result = startWork(db3, {
    agentId: agentId2,
    sessionId: params.sessionId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    runId: params.runId,
    rationale: params.rationale ?? "agent write operation",
    testPlan: params.testPlan ?? "post-edit verification",
    contextRef: params.contextRef,
    targetFiles: params.targetFiles ?? [],
    origin: "WORK",
    source: "EXPLICIT",
    ttlMs: effectiveTtlMs(params.ttlMs),
    exclusive: true
  });
  if (!result.ok) {
    return {
      ok: false,
      conflict: true,
      conflicts: result.conflicts.map((conflict) => {
        const holder = db3.prepare(`SELECT tr.session_id, s.ended_at, l.acquired_at
          FROM task_runs tr
          LEFT JOIN sessions s ON s.session_id = tr.session_id
          LEFT JOIN locks l ON l.run_id = tr.run_id AND l.file_path = ?
          WHERE tr.run_id = ?`).get(conflict.file_path, conflict.run_id);
        return {
          file_path: conflict.file_path,
          lock_type: "EXCLUSIVE",
          agent_id: conflict.agent_id,
          acquired_at: holder?.acquired_at ?? conflict.heartbeat_at,
          expires_at: conflict.expires_at,
          run_id: conflict.run_id,
          reasoning: conflict.rationale,
          test_plan: db3.prepare("SELECT test_plan FROM task_runs WHERE run_id = ?").get(conflict.run_id)?.["test_plan"] ?? "post-edit verification",
          session_id: holder?.session_id ?? null,
          holder_session_active: !holder?.ended_at
        };
      })
    };
  }
  const locks = activeLockRows(db3, { runId: result.run.run_id });
  return {
    ok: true,
    run: {
      run_id: result.run.run_id,
      task_id: result.run.task_id,
      origin: result.run.origin,
      agent_id: result.run.agent_id,
      session_id: result.run.session_id,
      workspace_path: result.run.workspace_path ?? workspaceScopeRoot(params.workspacePath),
      artifact: result.run.artifact,
      context_ref: result.run.context_ref,
      target_files: result.files.filter((file) => file.ended_at == null).map((file) => file.file_path),
      locks: locks.map((lock) => ({
        lock_id: lock.lock_id,
        file_path: lock.file_path,
        lock_type: "EXCLUSIVE",
        agent_id: lock.agent_id,
        session_id: lock.session_id,
        acquired_at: lock.acquired_at,
        expires_at: lock.expires_at
      })),
      status: result.run.status,
      created_at: result.run.created_at
    }
  };
}
function releaseFileLock(db3, params) {
  const {
    agentId: agentId2 = "agent",
    sessionId: sessionId2 = null,
    workspacePath = null,
    artifact: artifact2 = null,
    runId = null,
    targetFiles = [],
    status: statusArg = "SUCCESS",
    verified = false,
    verifiedNote
  } = params;
  if (!VALID_RELEASE_STATUSES.has(String(statusArg))) {
    throw new Error(`releaseFileLock status must be ACTIVE, PENDING, SUCCESS, or FAILED; got "${statusArg}"`);
  }
  const requestedStatus = String(statusArg);
  const requestedSuccessWithoutVerification = requestedStatus === "SUCCESS" && !verified;
  const effectiveStatus = requestedSuccessWithoutVerification ? "PENDING" : requestedStatus;
  const now = utcNow();
  const whereClauses = ["ai.run_id = fl.run_id", "ai.agent_id = ?"];
  const whereParams = [agentId2];
  if (sessionId2) {
    whereClauses.push("ai.session_id = ?");
    whereParams.push(sessionId2);
  }
  const artifactScope = normalizeArtifact(artifact2);
  if (workspacePath) {
    whereClauses.push("ai.workspace_path = ?");
    whereParams.push(workspaceScopeRoot(workspacePath));
  }
  if (artifactScope) {
    whereClauses.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    whereParams.push(artifactScope);
  }
  if (runId) {
    whereClauses.push("fl.run_id = ?");
    whereParams.push(runId);
  }
  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => "?").join(",");
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }
  const where = whereClauses.join(" AND ");
  const locks = db3.prepare(
    `SELECT fl.lock_id, fl.run_id, fl.file_path
       FROM locks fl JOIN task_runs ai ON ai.run_id = fl.run_id
      WHERE ${where}`
  ).all(...whereParams);
  const runIds = [...new Set(locks.map((l) => l.run_id))];
  if (runId && !runIds.includes(runId)) {
    const directWhere = ["run_id = ?", "agent_id = ?"];
    const directParams = [runId, agentId2];
    if (sessionId2) {
      directWhere.push("session_id = ?");
      directParams.push(sessionId2);
    }
    if (workspacePath) {
      directWhere.push("workspace_path = ?");
      directParams.push(workspaceScopeRoot(workspacePath));
    }
    if (artifactScope) {
      directWhere.push("(artifact = ? OR artifact IS NULL)");
      directParams.push(artifactScope);
    }
    const directRun = db3.prepare(`SELECT run_id FROM task_runs WHERE ${directWhere.join(" AND ")}`).get(...directParams);
    if (directRun) runIds.push(directRun.run_id);
  }
  const ambiguousRelease = !runId && absFiles.length > 0 && runIds.length > 1;
  if (ambiguousRelease) {
    return {
      agent_id: agentId2,
      status: effectiveStatus,
      released: false,
      locks_released: 0,
      run_ids: runIds,
      updated_at: now,
      ambiguousRelease: "target-file release matched multiple active runs; pass --run-id to release exactly one run"
    };
  }
  if (runIds.length === 0) {
    return {
      agent_id: agentId2,
      status: effectiveStatus,
      released: false,
      locks_released: 0,
      run_ids: [],
      updated_at: now
    };
  }
  const runMetadata = db3.prepare(`SELECT run_id, task_id, origin FROM task_runs
    WHERE run_id IN (${runIds.map(() => "?").join(",")})`).all(...runIds);
  if (effectiveStatus !== "ACTIVE" && runMetadata.some((run) => run.task_id != null)) {
    throw new Error("task-linked runs must use task submit or task release; lock release may only keep them ACTIVE");
  }
  db3.exec("BEGIN IMMEDIATE");
  let updatedRuns = 0;
  try {
    const lockIds = locks.map((lock) => lock.lock_id);
    if (lockIds.length > 0) {
      db3.prepare(`DELETE FROM locks WHERE lock_id IN (${lockIds.map(() => "?").join(",")})`).run(...lockIds);
    }
    for (const tid of runIds) {
      const metadata = runMetadata.find((run) => run.run_id === tid);
      if (effectiveStatus !== "ACTIVE" && metadata?.origin !== "TASK") {
        const releasedFiles = locks.filter((lock) => lock.run_id === tid).map((lock) => lock.file_path);
        const fileClause = releasedFiles.length > 0 ? ` AND file_path IN (${releasedFiles.map(() => "?").join(",")})` : "";
        db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
          WHERE run_id = ? AND ended_at IS NULL${fileClause}`).run(now, now, now, tid, ...releasedFiles);
      }
      const remaining = db3.prepare("SELECT 1 FROM locks WHERE run_id = ? LIMIT 1").get(tid);
      if (!remaining) {
        const updated = db3.prepare(
          `UPDATE task_runs SET status = ?, updated_at = ?
           WHERE run_id = ? AND agent_id = ? AND status IN ('ACTIVE','PENDING')`
        ).run(effectiveStatus, now, tid, agentId2);
        updatedRuns += updated.changes;
        if (updated.changes > 0 && verified && verifiedNote) {
          try {
            db3.prepare(
              `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
               VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
            ).run("evt_" + randomUUID5().replace(/-/g, ""), tid, agentId2, verifiedNote, now);
          } catch {
          }
        }
      }
    }
    db3.exec("COMMIT");
  } catch (e) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
  return {
    agent_id: agentId2,
    status: effectiveStatus,
    released: locks.length > 0 || updatedRuns > 0,
    locks_released: locks.length,
    run_ids: runIds,
    updated_at: now,
    ...requestedSuccessWithoutVerification ? { unverifiedConclusion: "SUCCESS requested without --verified; stored as PENDING until verify records the test result." } : {}
  };
}

// src/plans.ts
import { randomUUID as randomUUID6 } from "node:crypto";
import { existsSync as existsSync2, mkdirSync as mkdirSync2, rmSync, writeFileSync } from "node:fs";
import { isAbsolute as isAbsolute3, join as join4, relative, resolve as resolve7 } from "node:path";
function required2(value, field) {
  const normalized = value.trim();
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}
function slugify(value) {
  const slug = value.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 64);
  return slug || "plan";
}
function planTimestamp(now) {
  return now.replace(/[-:]/g, "").replace("T", "-").replace(/\.\d+Z$/, "Z");
}
function renderPrimaryPlanDoc(plan) {
  return `# ${plan.name}

Plan ID: \`${plan.plan_id}\`  
Lead agent: \`${plan.lead_agent_id}\`  
## Objective

${plan.objective}

## Live work

Plan lifecycle, membership, and task state are live in the Awareness database. Use \`task list --plan-id ${plan.plan_id}\` or \`task ready --plan-id ${plan.plan_id}\`; do not duplicate an editable task checklist here.

## Supporting decisions

Add durable design notes under \`docs/\` and register them with the plan.
`;
}
function rowToPlan(row) {
  return row;
}
function createPlan(db3, params) {
  const name = required2(params.name, "plan name");
  const objective = required2(params.objective, "plan objective");
  const leadAgentId = required2(params.leadAgentId, "lead agent id");
  const workspacePath = normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve7(params.workspacePath);
  const now = utcNow();
  const planId = `plan_${randomUUID6().replace(/-/g, "")}`;
  const docDir = `.octocode/plan/${planTimestamp(now)}-${slugify(name)}`;
  const absoluteDir = join4(workspacePath, docDir);
  if (existsSync2(absoluteDir)) {
    throw new Error(`plan document directory already exists: ${docDir}`);
  }
  const plan = {
    plan_id: planId,
    name,
    objective,
    lead_agent_id: leadAgentId,
    status: "DRAFT",
    workspace_path: workspacePath,
    artifact: normalizeArtifact(params.artifact),
    doc_dir: docDir,
    created_at: now,
    updated_at: now
  };
  mkdirSync2(join4(absoluteDir, "docs"), { recursive: true });
  const planPath = join4(absoluteDir, "PLAN.md");
  const manifestPath = join4(absoluteDir, "manifest.json");
  try {
    writeFileSync(planPath, renderPrimaryPlanDoc(plan), { encoding: "utf8", flag: "wx" });
    writeFileSync(manifestPath, `${JSON.stringify({
      schema_version: 1,
      plan_id: planId,
      primary_doc: "PLAN.md",
      supporting_docs_dir: "docs",
      live_task_state: "awareness.sqlite3"
    }, null, 2)}
`, { encoding: "utf8", flag: "wx" });
    db3.exec("BEGIN IMMEDIATE");
    try {
      db3.prepare(`INSERT INTO plans
        (plan_id, name, objective, lead_agent_id, status, workspace_path, artifact, doc_dir, created_at, updated_at)
        VALUES (?, ?, ?, ?, 'DRAFT', ?, ?, ?, ?, ?)`).run(planId, name, objective, leadAgentId, workspacePath, plan.artifact, docDir, now, now);
      db3.prepare(`INSERT INTO plan_members(plan_id, agent_id, role, joined_at)
        VALUES (?, ?, 'LEAD', ?)`).run(planId, leadAgentId, now);
      db3.prepare(`INSERT INTO plan_docs(plan_id, relative_path, title, kind, ordinal)
        VALUES (?, 'PLAN.md', ?, 'PRIMARY', 0)`).run(planId, name);
      db3.exec("COMMIT");
    } catch (error) {
      try {
        db3.exec("ROLLBACK");
      } catch {
      }
      throw error;
    }
  } catch (error) {
    rmSync(absoluteDir, { recursive: true, force: true });
    throw error;
  }
  return { plan, document_path: planPath, manifest_path: manifestPath };
}
function getPlan(db3, planId) {
  const row = db3.prepare("SELECT * FROM plans WHERE plan_id = ?").get(planId);
  if (!row) return null;
  const members = db3.prepare(
    "SELECT agent_id, role, joined_at FROM plan_members WHERE plan_id = ? ORDER BY role, joined_at, agent_id"
  ).all(planId);
  const docs = db3.prepare(
    "SELECT relative_path, title, kind, ordinal FROM plan_docs WHERE plan_id = ? ORDER BY ordinal, relative_path"
  ).all(planId);
  return { ...rowToPlan(row), members, docs };
}
function listPlans(db3, params = {}) {
  const where = ["1 = 1"];
  const binds = [];
  if (params.workspacePath) {
    where.push("workspace_path = ?");
    binds.push(normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve7(params.workspacePath));
  }
  const artifact2 = normalizeArtifact(params.artifact);
  if (artifact2) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact2);
  }
  if (params.status) {
    where.push("status = ?");
    binds.push(params.status);
  }
  return db3.prepare(
    `SELECT * FROM plans WHERE ${where.join(" AND ")} ORDER BY updated_at DESC, plan_id`
  ).all(...binds).map((row) => rowToPlan(row));
}
function joinPlan(db3, params) {
  const agentId2 = required2(params.agentId, "agent id");
  if (!getPlan(db3, params.planId)) throw new Error(`plan not found: ${params.planId}`);
  const now = utcNow();
  db3.prepare(`INSERT INTO plan_members(plan_id, agent_id, role, joined_at)
    VALUES (?, ?, 'CONTRIBUTOR', ?)
    ON CONFLICT(plan_id, agent_id) DO NOTHING`).run(params.planId, agentId2, now);
  return db3.prepare(
    "SELECT agent_id, role, joined_at FROM plan_members WHERE plan_id = ? AND agent_id = ?"
  ).get(params.planId, agentId2);
}
function registerPlanDocument(db3, params) {
  const plan = getPlan(db3, params.planId);
  if (!plan) throw new Error(`plan not found: ${params.planId}`);
  const member = db3.prepare("SELECT 1 FROM plan_members WHERE plan_id = ? AND agent_id = ?").get(params.planId, required2(params.agentId, "agent id"));
  if (!member) throw new Error(`agent ${params.agentId} must join plan ${params.planId} before registering docs`);
  const relativePath = required2(params.relativePath, "document path").replace(/\\/g, "/");
  if (isAbsolute3(relativePath)) throw new Error("plan document path must be relative to the plan folder");
  const planDir = resolve7(plan.workspace_path, plan.doc_dir);
  const absolutePath = resolve7(planDir, relativePath);
  const withinPlan = relative(planDir, absolutePath);
  if (!withinPlan || withinPlan === ".." || withinPlan.startsWith("../") || isAbsolute3(withinPlan)) {
    throw new Error("plan document path must stay inside the plan folder");
  }
  if (!existsSync2(absolutePath)) throw new Error(`plan document does not exist: ${relativePath}`);
  const nextOrdinal = db3.prepare(
    "SELECT COALESCE(MAX(ordinal), 0) + 1 AS ordinal FROM plan_docs WHERE plan_id = ?"
  ).get(params.planId).ordinal;
  db3.prepare(`INSERT INTO plan_docs(plan_id, relative_path, title, kind, ordinal)
    VALUES (?, ?, ?, 'SUPPORTING', ?)
    ON CONFLICT(plan_id, relative_path) DO UPDATE SET title = excluded.title`).run(params.planId, relativePath, required2(params.title, "document title"), nextOrdinal);
  return db3.prepare(
    "SELECT relative_path, title, kind, ordinal FROM plan_docs WHERE plan_id = ? AND relative_path = ?"
  ).get(params.planId, relativePath);
}
function updatePlanStatus(db3, params) {
  const plan = getPlan(db3, params.planId);
  if (!plan) throw new Error(`plan not found: ${params.planId}`);
  if (plan.lead_agent_id !== params.agentId) {
    throw new Error(`only lead agent ${plan.lead_agent_id} can change plan status`);
  }
  const now = utcNow();
  db3.prepare("UPDATE plans SET status = ?, updated_at = ? WHERE plan_id = ?").run(params.status, now, params.planId);
  return getPlan(db3, params.planId);
}

// src/tasks.ts
import { randomUUID as randomUUID7 } from "node:crypto";
import { isAbsolute as isAbsolute4, relative as relative2, resolve as resolve8, sep } from "node:path";
var DEFAULT_CLAIM_LEASE_MS = 30 * 6e4;
var MAX_CLAIM_LEASE_MS = 60 * 6e4;
function required3(value, field) {
  const normalized = value.trim();
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}
function normalizeTaskPaths(workspacePath, paths) {
  if (paths.length === 0) throw new Error("at least one task path is required");
  const root = resolve8(workspacePath);
  const normalized = paths.map((input) => {
    const value = required3(input, "task path");
    const absolute = isAbsolute4(value) ? resolve8(value) : resolve8(root, value);
    const rel = relative2(root, absolute);
    if (!rel || rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute4(rel)) {
      throw new Error(`task path must be workspace-relative and below the workspace: ${input}`);
    }
    return rel.split(sep).join("/");
  });
  return [...new Set(normalized)];
}
function event(db3, taskId, runId, agentId2, eventType, message, now = utcNow()) {
  db3.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`).run(`tevt_${randomUUID7().replace(/-/g, "")}`, taskId, runId, agentId2, eventType, message, now);
}
function evictExpiredTaskClaims(db3, now = utcNow()) {
  const expired = db3.prepare(
    "SELECT task_id, run_id, agent_id FROM task_claims WHERE expires_at <= ?"
  ).all(now);
  if (expired.length === 0) return;
  db3.exec("SAVEPOINT evict_expired_task_claims");
  try {
    for (const claim of expired) {
      db3.prepare("DELETE FROM locks WHERE run_id = ?").run(claim.run_id);
      db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, claim.run_id);
      db3.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'").run(now, claim.run_id);
      db3.prepare("UPDATE tasks SET status = 'OPEN', updated_at = ? WHERE task_id = ? AND status = 'IN_PROGRESS'").run(now, claim.task_id);
      db3.prepare("DELETE FROM task_claims WHERE task_id = ?").run(claim.task_id);
      event(db3, claim.task_id, claim.run_id, claim.agent_id, "CLAIM_EXPIRED", "claim lease expired", now);
    }
    db3.exec("RELEASE SAVEPOINT evict_expired_task_claims");
  } catch (e) {
    try {
      db3.exec("ROLLBACK TO SAVEPOINT evict_expired_task_claims");
    } catch {
    }
    try {
      db3.exec("RELEASE SAVEPOINT evict_expired_task_claims");
    } catch {
    }
    throw e;
  }
}
function hydrateTask(db3, row) {
  const taskId = String(row["task_id"]);
  const paths = db3.prepare("SELECT path FROM task_paths WHERE task_id = ? ORDER BY ordinal, path").all(taskId).map((item) => String(item["path"]));
  const dependencies = db3.prepare(
    "SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ? ORDER BY created_at, depends_on_task_id"
  ).all(taskId).map((item) => String(item["depends_on_task_id"]));
  const claim = db3.prepare("SELECT * FROM task_claims WHERE task_id = ?").get(taskId);
  return { ...row, paths, dependencies, claim: claim ?? null };
}
function getTask(db3, taskId) {
  evictExpiredTaskClaims(db3);
  const row = db3.prepare("SELECT * FROM tasks WHERE task_id = ?").get(taskId);
  return row ? hydrateTask(db3, row) : null;
}
function activeTaskClaimForAgent(db3, params) {
  evictExpiredTaskClaims(db3);
  const workspacePath = normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve8(params.workspacePath);
  const where = ["c.agent_id = ?", "p.workspace_path = ?", "c.expires_at > ?"];
  const binds = [params.agentId, workspacePath, utcNow()];
  if (params.artifact) {
    where.push("(p.artifact = ? OR p.artifact IS NULL)");
    binds.push(params.artifact);
  }
  const claims = db3.prepare(`SELECT c.* FROM task_claims c
    JOIN tasks t ON t.task_id = c.task_id
    JOIN plans p ON p.plan_id = t.plan_id
    WHERE ${where.join(" AND ")} ORDER BY c.claimed_at DESC LIMIT 2`).all(...binds);
  return claims.length === 1 ? claims[0] : null;
}
function createTask(db3, params) {
  const plan = db3.prepare("SELECT workspace_path, status FROM plans WHERE plan_id = ?").get(params.planId);
  if (!plan) throw new Error(`plan not found: ${params.planId}`);
  if (["COMPLETED", "CANCELLED"].includes(plan.status)) {
    throw new Error(`cannot add tasks to ${plan.status.toLowerCase()} plan ${params.planId}`);
  }
  const title = required3(params.title, "task title");
  const reasoning = required3(params.reasoning, "task reasoning");
  const createdBy = required3(params.createdBy, "task creator");
  const acceptance = params.acceptanceCriteria?.trim() || "Complete the described work and verify affected behavior.";
  const paths = normalizeTaskPaths(plan.workspace_path, params.paths);
  const taskId = `task_${randomUUID7().replace(/-/g, "")}`;
  const now = utcNow();
  db3.exec("BEGIN IMMEDIATE");
  try {
    db3.prepare(`INSERT INTO tasks
      (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 'OPEN', ?, ?, ?, ?)`).run(taskId, params.planId, title, reasoning, acceptance, params.priority ?? 0, createdBy, now, now);
    const insertPath = db3.prepare("INSERT INTO task_paths(task_id, path, ordinal) VALUES (?, ?, ?)");
    paths.forEach((path2, ordinal) => insertPath.run(taskId, path2, ordinal));
    event(db3, taskId, null, createdBy, "CREATED", reasoning, now);
    for (const dependency of params.dependsOn ?? []) {
      addTaskDependency(db3, { taskId, dependsOnTaskId: dependency, agentId: createdBy });
    }
    db3.exec("COMMIT");
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return { task: getTask(db3, taskId) };
}
function addTaskDependency(db3, params) {
  if (params.taskId === params.dependsOnTaskId) throw new Error("a task cannot depend on itself");
  const rows = db3.prepare("SELECT task_id, plan_id FROM tasks WHERE task_id IN (?, ?)").all(params.taskId, params.dependsOnTaskId);
  if (rows.length !== 2) throw new Error("both dependency tasks must exist");
  if (rows[0].plan_id !== rows[1].plan_id) throw new Error("task dependencies must stay within one plan");
  const cycle = db3.prepare(`WITH RECURSIVE chain(task_id) AS (
      SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ?
      UNION
      SELECT td.depends_on_task_id FROM task_dependencies td JOIN chain c ON td.task_id = c.task_id
    ) SELECT 1 FROM chain WHERE task_id = ? LIMIT 1`).get(params.dependsOnTaskId, params.taskId);
  if (cycle) throw new Error("task dependency would create a cycle");
  const now = utcNow();
  db3.prepare(`INSERT OR IGNORE INTO task_dependencies
    (task_id, depends_on_task_id, created_by, created_at) VALUES (?, ?, ?, ?)`).run(params.taskId, params.dependsOnTaskId, params.agentId, now);
  event(db3, params.taskId, null, params.agentId, "DEPENDENCY_ADDED", params.dependsOnTaskId, now);
}
function listTasks(db3, params = {}) {
  evictExpiredTaskClaims(db3);
  const where = ["1 = 1"];
  const binds = [];
  if (params.planId) {
    where.push("t.plan_id = ?");
    binds.push(params.planId);
  }
  if (params.status) {
    where.push("t.status = ?");
    binds.push(params.status);
  }
  if (params.agentId) {
    where.push("EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id AND c.agent_id = ?)");
    binds.push(params.agentId);
  }
  return db3.prepare(`SELECT t.* FROM tasks t WHERE ${where.join(" AND ")}
    ORDER BY t.priority DESC, t.created_at, t.task_id`).all(...binds).map((row) => hydrateTask(db3, row));
}
function listReadyTasks(db3, params = {}) {
  evictExpiredTaskClaims(db3);
  const binds = [];
  const planWhere = params.planId ? "AND t.plan_id = ?" : "";
  if (params.planId) binds.push(params.planId);
  const rows = db3.prepare(`SELECT t.* FROM tasks t
    WHERE t.status = 'OPEN' ${planWhere}
      AND NOT EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id)
      AND NOT EXISTS (
        SELECT 1 FROM task_dependencies td
        JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
      )
    ORDER BY t.priority DESC, t.created_at, t.task_id`).all(...binds);
  return rows.map((row) => hydrateTask(db3, row));
}
function claimTask(db3, params) {
  const agentId2 = required3(params.agentId, "agent id");
  const now = utcNow();
  const leaseMs = Math.min(Math.max(1, params.leaseMs ?? DEFAULT_CLAIM_LEASE_MS), MAX_CLAIM_LEASE_MS);
  const expiresAt = new Date(Date.parse(now) + leaseMs).toISOString().replace(/\.\d{3}Z$/, "Z");
  const runId = `run_${randomUUID7().replace(/-/g, "")}`;
  db3.exec("BEGIN IMMEDIATE");
  try {
    evictExpiredTaskClaims(db3, now);
    const row = db3.prepare(`SELECT t.*, p.workspace_path, p.artifact
      FROM tasks t JOIN plans p ON p.plan_id = t.plan_id WHERE t.task_id = ?`).get(params.taskId);
    if (!row) {
      db3.exec("ROLLBACK");
      return { ok: false, error: `task not found: ${params.taskId}`, task_id: params.taskId };
    }
    const existing = db3.prepare("SELECT agent_id FROM task_claims WHERE task_id = ?").get(params.taskId);
    if (existing) {
      db3.exec("ROLLBACK");
      return { ok: false, error: `task is already claimed by ${existing.agent_id}`, task_id: params.taskId };
    }
    if (row["status"] !== "OPEN") {
      db3.exec("ROLLBACK");
      return { ok: false, error: `task is not ready: status=${String(row["status"])}`, task_id: params.taskId };
    }
    const blocked = db3.prepare(`SELECT 1 FROM task_dependencies td
      JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
      WHERE td.task_id = ? AND dependency.status <> 'DONE' LIMIT 1`).get(params.taskId);
    if (blocked) {
      db3.exec("ROLLBACK");
      return { ok: false, error: "task is blocked by unfinished dependencies", task_id: params.taskId };
    }
    const workspacePath = String(row["workspace_path"]);
    const artifact2 = row["artifact"] == null ? null : String(row["artifact"]);
    const reasoning = String(row["reasoning"]);
    const acceptanceCriteria = String(row["acceptance_criteria"]);
    const planId = String(row["plan_id"]);
    if (params.sessionId) {
      db3.prepare(`INSERT OR IGNORE INTO sessions(session_id, agent_id, workspace_path, artifact, started_at)
        VALUES (?, ?, ?, ?, ?)`).run(params.sessionId, agentId2, workspacePath, artifact2, now);
    }
    db3.prepare(`INSERT INTO task_runs
      (run_id, task_id, origin, agent_id, session_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
      VALUES (?, ?, 'TASK', ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)`).run(
      runId,
      params.taskId,
      agentId2,
      params.sessionId ?? null,
      reasoning,
      params.testPlan?.trim() || acceptanceCriteria,
      workspacePath,
      artifact2,
      now,
      now
    );
    db3.prepare(`INSERT INTO task_claims(task_id, run_id, agent_id, claimed_at, heartbeat_at, expires_at)
      VALUES (?, ?, ?, ?, ?, ?)`).run(params.taskId, runId, agentId2, now, now, expiresAt);
    db3.prepare("UPDATE tasks SET status = 'IN_PROGRESS', updated_at = ? WHERE task_id = ?").run(now, params.taskId);
    db3.prepare(`INSERT INTO plan_members(plan_id, agent_id, role, joined_at)
      VALUES (?, ?, 'CONTRIBUTOR', ?) ON CONFLICT(plan_id, agent_id) DO NOTHING`).run(planId, agentId2, now);
    event(db3, params.taskId, runId, agentId2, "CLAIMED", "task claimed", now);
    db3.exec("COMMIT");
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  const task = getTask(db3, params.taskId);
  const run = db3.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(runId);
  return { ok: true, task, run, claim: task.claim };
}
function heartbeatTaskClaim(db3, params) {
  const now = utcNow();
  const leaseMs = Math.min(Math.max(1, params.leaseMs ?? DEFAULT_CLAIM_LEASE_MS), MAX_CLAIM_LEASE_MS);
  const expiresAt = new Date(Date.parse(now) + leaseMs).toISOString().replace(/\.\d{3}Z$/, "Z");
  const result = db3.prepare(`UPDATE task_claims SET heartbeat_at = ?, expires_at = ?
    WHERE task_id = ? AND run_id = ? AND agent_id = ?`).run(now, expiresAt, params.taskId, params.runId, params.agentId);
  if (result.changes === 0) throw new Error("active task claim not found for this agent and run");
  return db3.prepare("SELECT * FROM task_claims WHERE task_id = ?").get(params.taskId);
}
function submitTask(db3, params) {
  const now = utcNow();
  evictExpiredTaskClaims(db3, now);
  db3.exec("BEGIN IMMEDIATE");
  try {
    const claim = db3.prepare(
      `SELECT 1 AS ok FROM task_claims
       WHERE task_id = ? AND run_id = ? AND agent_id = ? AND expires_at > ?`
    ).get(params.taskId, params.runId, params.agentId, now);
    if (!claim) {
      db3.exec("ROLLBACK");
      throw new Error("only the active claimant can submit this task");
    }
    db3.prepare("DELETE FROM locks WHERE run_id = ?").run(params.runId);
    db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
      WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, params.runId);
    db3.prepare("UPDATE task_runs SET status = 'PENDING', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'").run(now, params.runId);
    db3.prepare("UPDATE tasks SET status = 'VERIFY', updated_at = ? WHERE task_id = ?").run(now, params.taskId);
    db3.prepare("DELETE FROM task_claims WHERE task_id = ?").run(params.taskId);
    event(db3, params.taskId, params.runId, params.agentId, "SUBMITTED", params.message?.trim() || "submitted for verification", now);
    db3.exec("COMMIT");
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return {
    task: getTask(db3, params.taskId),
    run: db3.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(params.runId)
  };
}
function releaseTaskClaim(db3, params) {
  const now = utcNow();
  const blockedReason = params.blockedReason?.trim();
  evictExpiredTaskClaims(db3, now);
  db3.exec("BEGIN IMMEDIATE");
  try {
    const claim = db3.prepare(
      `SELECT 1 AS ok FROM task_claims
       WHERE task_id = ? AND run_id = ? AND agent_id = ? AND expires_at > ?`
    ).get(params.taskId, params.runId, params.agentId, now);
    if (!claim) {
      db3.exec("ROLLBACK");
      throw new Error("only the active claimant can release this task");
    }
    db3.prepare("DELETE FROM locks WHERE run_id = ?").run(params.runId);
    db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
      WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, params.runId);
    db3.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'").run(now, params.runId);
    db3.prepare("UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ?").run(blockedReason ? "BLOCKED" : "OPEN", now, params.taskId);
    db3.prepare("DELETE FROM task_claims WHERE task_id = ?").run(params.taskId);
    event(db3, params.taskId, params.runId, params.agentId, blockedReason ? "BLOCKED" : "RELEASED", blockedReason || "claim released", now);
    db3.exec("COMMIT");
  } catch (error) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return getTask(db3, params.taskId);
}

// src/reflect.ts
import { resolve as resolve9 } from "node:path";
var NEXT_MSG = [
  "Next: inspect created fixes with octocode-awareness refinement get --state open.",
  "After applying and verifying a fix, close it with octocode-awareness refinement set --refinement-id <id> --state done.",
  "Use octocode-awareness reflect mine-weakness for recurring failures and octocode-awareness reflect export-harness for human-reviewed harness proposals."
].join(" ");
function normalizeScopePaths(paths = [], prefix, baseCwd) {
  const base = baseCwd ?? process.cwd();
  return [...new Set(paths.filter(Boolean).map((p) => {
    const abs = p.startsWith("/") ? p : resolve9(base, p);
    return `${prefix}:${abs}`;
  }))];
}
function reflect(db3, params) {
  const {
    agentId: agentId2 = "agent",
    task,
    outcome,
    lesson,
    worked,
    didntWork,
    fixRepo,
    fixHarness,
    fixInstructions,
    failureSignature: failSigArg,
    importance: impArg,
    judgmentNote,
    duo = false,
    evalFailures = [],
    references = [],
    file,
    files = [],
    folders = [],
    validFrom,
    validTo,
    workspacePath,
    artifact: artifact2,
    repo: repoArg,
    ref: refArg,
    cwd
  } = params;
  const resolvedOutcome = normalizeReflectionOutcome(outcome);
  const bits = [`[reflection:${resolvedOutcome}] ${task}`];
  if (worked) bits.push(`worked: ${worked}`);
  if (didntWork) bits.push(`didn't work: ${didntWork}`);
  if (judgmentNote) bits.push(`judgment: ${judgmentNote}`);
  if (fixHarness) bits.push(`harness fix: ${fixHarness}`);
  if (fixInstructions) bits.push(`instructions feedback: ${fixInstructions}`);
  const narrative = bits.join(" | ");
  const observation = lesson ? bits.length > 1 ? `${lesson}  (${narrative})` : lesson : narrative;
  const importance = impArg != null ? Number(impArg) : REFLECTION_IMPORTANCE[resolvedOutcome] ?? 5;
  const hasEvalFailures = evalFailures.length > 0;
  const tags = [
    "reflection",
    resolvedOutcome,
    ...fixHarness ? ["harness"] : [],
    // `developer-review` is the query tag the DEVELOPER_REVIEW.md projection reads;
    // `instructions` scopes it to the instruction-author feedback channel.
    ...fixInstructions ? ["instructions", "developer-review"] : [],
    ...hasEvalFailures ? ["eval"] : []
  ];
  const failSig = failSigArg ?? evalFailures.find((f) => f.failure_signature)?.failure_signature ?? null;
  const sig = failSig ?? (resolvedOutcome === "failed" && fixHarness ? "harness:reflection|outcome:failed" : null);
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact2), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const scopeReferences = [
    ...references,
    ...normalizeScopePaths(file ? [file] : [], "file", cwd),
    ...normalizeScopePaths(files, "file", cwd),
    ...normalizeScopePaths(folders, "dir", cwd)
  ];
  const { memoryId, similarMemoryIds, noveltyScore } = insertMemory(db3, {
    agentId: agentId2,
    taskContext: task,
    observation,
    importance,
    label: "EXPERIENCE",
    // distinct label so reflections are filterable and excluded from briefings
    tags,
    references: scopeReferences,
    failureSignature: sig,
    validFrom,
    validTo,
    workspacePath: scope.workspace_path,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    cwd
  });
  const evalFailureIds = [];
  for (const failure of evalFailures) {
    if (!failure || typeof failure.id !== "string" || !failure.id.trim()) continue;
    const lessonText = failure.suggested_lesson?.trim() || `Eval question ${failure.id} failed${failure.dimension ? ` on ${failure.dimension}` : ""}.`;
    const { memoryId: evalMemId } = insertMemory(db3, {
      agentId: agentId2,
      taskContext: `[eval:${failure.id}]${failure.dimension ? ` ${failure.dimension} \u2014` : ""} ${task}`,
      observation: lessonText,
      importance,
      label: "EXPERIENCE",
      tags: ["reflection", "eval", resolvedOutcome],
      failureSignature: failure.failure_signature ?? sig,
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd
    });
    evalFailureIds.push(evalMemId);
  }
  let refinementId = null;
  if (fixRepo) {
    const refinementQuality = resolvedOutcome === "worked" ? "good" : "bad";
    const { refinementId: rid } = insertRefinement(db3, {
      agentId: agentId2,
      reasoning: `Fix in repo (from ${resolvedOutcome} reflection): ${fixRepo}`,
      remember: fixRepo,
      quality: refinementQuality,
      state: "open",
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      files: [...normalizeScopePaths(files, "file", cwd), ...normalizeScopePaths(folders, "dir", cwd)],
      cwd
    });
    refinementId = rid;
  }
  let developerReviewRefinementId = null;
  if (fixInstructions) {
    const { refinementId: rid } = insertRefinement(db3, {
      agentId: agentId2,
      reasoning: `Instructions feedback (from ${resolvedOutcome} reflection on "${task}"): ${fixInstructions}`,
      remember: fixInstructions,
      quality: "instructions",
      state: "open",
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      files: [...normalizeScopePaths(files, "file", cwd), ...normalizeScopePaths(folders, "dir", cwd)],
      cwd
    });
    developerReviewRefinementId = rid;
  }
  try {
    insertHarnessLog(db3, {
      agentId: agentId2,
      eventType: "reflect",
      memoryId,
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      payload: {
        outcome: resolvedOutcome,
        novelty_score: noveltyScore,
        harness_fix: Boolean(fixHarness),
        instructions_feedback: Boolean(fixInstructions),
        refinement_id: refinementId,
        developer_review_refinement_id: developerReviewRefinementId,
        eval_count: evalFailureIds.length,
        workspace_path: scope.workspace_path,
        artifact: scope.artifact
      }
    });
  } catch {
  }
  const result = {
    outcome: resolvedOutcome,
    learning_memory_id: memoryId,
    repo_fix_refinement_id: refinementId,
    harness_fix: Boolean(fixHarness),
    instructions_feedback: Boolean(fixInstructions),
    developer_review_refinement_id: developerReviewRefinementId,
    eval_failure_count: evalFailureIds.length,
    eval_failure_ids: evalFailureIds,
    next: NEXT_MSG,
    novelty_score: noveltyScore,
    similar_memory_ids: similarMemoryIds
  };
  if (duo) {
    result.reflection_duo = {
      advisory: true,
      roles: [
        {
          role: "supporter",
          prompt: `Reviewing "${task}" (outcome: ${resolvedOutcome}): what in this approach worked and should be reinforced or generalized? Name the strongest evidence for keeping it.`
        },
        {
          role: "skeptic",
          prompt: `Reviewing "${task}" (outcome: ${resolvedOutcome}): what evidence is missing or unverified? What alternative explanation or failure mode does this reflection overlook?`
        }
      ]
    };
  }
  return result;
}

// src/maintenance.ts
import { spawnSync as spawnSync3 } from "node:child_process";
import { createHash as createHash2, randomUUID as randomUUID9 } from "node:crypto";
import { isAbsolute as isAbsolute5, resolve as resolve10 } from "node:path";

// src/notifications.ts
import { randomUUID as randomUUID8 } from "node:crypto";

// src/sql/runs.ts
var RUNS_SELECT_PENDING_IDS = `SELECT run_id FROM task_runs WHERE status = 'PENDING' AND agent_id = ? {DYNAMIC_WHERE}`;
var RUNS_SELECT_STATUS = `SELECT agent_id, status FROM task_runs WHERE run_id = ?`;
var RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT = `UPDATE task_runs SET status = ?, updated_at = ? WHERE run_id = ? AND agent_id = ? AND status = 'PENDING'`;
var RUNS_UPDATE_PENDING_TO_FAILED = `UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'PENDING'`;
var RUNS_UPDATE_ACTIVE_TO_FAILED = `UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'`;
var RUN_LOG_INSERT_VERIFIED = `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'VERIFIED', ?, ?)`;
var RUN_LOG_INSERT_ABANDONED = `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`;
var RUN_LOG_INSERT_STALE_ABANDONED = `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'stale active (no live file presence) abandoned by audit-unverified --abandon', ?)`;

// src/sql/sessions.ts
var SESSIONS_UPDATE_END = `UPDATE sessions SET ended_at = ?, summary = ? WHERE session_id = ? RETURNING *`;

// src/sql/signals.ts
var SIGNALS_SELECT_THREAD_ID = "SELECT thread_id FROM signals WHERE signal_id = ?";
var SIGNALS_INSERT = `INSERT INTO signals
   (signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind, subject, body,
    files_json, refs_json, thread_id, reply_to, importance, status, created_at)
   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?)`;
var SIGNALS_SELECT_BASE = "SELECT n.* FROM signals n";
var SIGNALS_SELECT_LEFT_JOIN_READS = "LEFT JOIN signal_reads nr ON nr.signal_id = n.signal_id AND nr.agent_id = ?";
var SIGNALS_SELECT_ORDER_LIMIT = "ORDER BY n.created_at DESC LIMIT ?";
var SIGNALS_DELETE_BY_IDS = (ph) => `DELETE FROM signals WHERE signal_id IN (${ph})`;
var SIGNALS_SELECT_IDS_FOR_PRUNE = "SELECT signal_id FROM signals WHERE";
var SIGNAL_READS_INSERT_IGNORE = "INSERT OR IGNORE INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)";
var SIGNAL_READS_DELETE_BY_SIGNAL_IDS = (ph) => `DELETE FROM signal_reads WHERE signal_id IN (${ph})`;

// src/sql/agents.ts
var AGENTS_UPSERT = `INSERT INTO agents (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)
   ON CONFLICT(agent_id) DO UPDATE SET
     agent_name     = CASE WHEN excluded.agent_name <> '' THEN excluded.agent_name ELSE agent_name END,
     workspace_path = COALESCE(excluded.workspace_path, workspace_path),
     artifact       = COALESCE(excluded.artifact, artifact),
     context        = COALESCE(excluded.context, context),
     last_seen_at   = excluded.last_seen_at`;
var AGENTS_LIST_SELECT = `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
   FROM agents`;
var AGENTS_LIST_CLAUSE_WORKSPACE_PATH = `(workspace_path = ? OR workspace_path IS NULL)`;
var AGENTS_LIST_CLAUSE_ARTIFACT = `(artifact = ? OR artifact IS NULL)`;
var AGENTS_LIST_ORDER = `ORDER BY last_seen_at DESC`;

// src/notifications.ts
function rowToNotification(r) {
  return {
    signal_id: r.signal_id,
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    repo: r.repo,
    ref: r.ref,
    from_agent: r.from_agent,
    to_agent: r.to_agent,
    kind: r.kind,
    subject: r.subject,
    body: r.body,
    // ARCH-7: Use shared parseJsonList helper instead of duplicated inline IIFEs
    files: parseJsonList(r.files_json),
    refs: parseJsonList(r.refs_json),
    thread_id: r.thread_id,
    reply_to: r.reply_to,
    importance: r.importance,
    status: r.status,
    created_at: r.created_at
  };
}
function insertNotification(db3, params) {
  const {
    agentId: agentId2,
    toAgent = null,
    kind,
    subject,
    body = null,
    files = [],
    refIds = [],
    inReplyTo = null,
    importance = 5,
    cwd
  } = params;
  const normalizedKind = normalizeNotificationKind(kind);
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd()
  );
  const signalId = "ntf_" + randomUUID8().replace(/-/g, "");
  const createdAt = utcNow();
  const wsPath = scope.workspace_path ?? process.cwd();
  let threadId;
  if (inReplyTo) {
    const parent = db3.prepare(SIGNALS_SELECT_THREAD_ID).get(inReplyTo);
    if (!parent) {
      throw new Error(`insertNotification: parent signal ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
    }
    threadId = parent.thread_id;
  } else {
    threadId = signalId;
  }
  db3.prepare(SIGNALS_INSERT).run(
    signalId,
    wsPath,
    scope.artifact,
    scope.repo,
    scope.ref,
    agentId2,
    toAgent,
    normalizedKind,
    subject,
    body,
    JSON.stringify(files),
    JSON.stringify(refIds),
    threadId,
    inReplyTo,
    importance,
    createdAt
  );
  return { signal_id: signalId, thread_id: threadId, workspace_path: wsPath, artifact: scope.artifact };
}
function appendSignalScope(where, binds, scope, alias = "n") {
  const prefix = alias ? `${alias}.` : "";
  if (scope.workspace_path) {
    where.push(`(${prefix}workspace_path = ? OR ${prefix}workspace_path IS NULL)`);
    binds.push(scope.workspace_path);
  }
  if (scope.artifact) {
    where.push(`(${prefix}artifact = ? OR ${prefix}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${prefix}repo = ? OR ${prefix}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${prefix}ref = ? OR ${prefix}ref IS NULL)`);
    binds.push(scope.ref);
  }
}
function getNotifications(db3, params) {
  const {
    agentId: agentId2,
    kinds = [],
    threadId = null,
    unreadOnly = true,
    markRead = false,
    limit = 20,
    cwd
  } = params;
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd()
  );
  const where = [];
  const binds = [];
  appendSignalScope(where, binds, scope);
  if (threadId) {
    where.push("n.thread_id = ?");
    binds.push(threadId);
    if (unreadOnly) {
      where.push("n.status = 'open'");
      where.push("nr.signal_id IS NULL");
    }
  } else {
    where.push("(n.to_agent IS NULL OR n.to_agent = ?)");
    binds.push(agentId2);
    where.push("n.from_agent <> ?");
    binds.push(agentId2);
    if (unreadOnly) {
      where.push("n.status = 'open'");
      where.push("nr.signal_id IS NULL");
    }
  }
  if (kinds.length > 0) {
    where.push(`n.kind IN (${kinds.map(() => "?").join(",")})`);
    binds.push(...kinds);
  }
  const whereClause = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const joinClause = unreadOnly ? SIGNALS_SELECT_LEFT_JOIN_READS : "";
  const allBinds = unreadOnly ? [agentId2, ...binds] : binds;
  const sql = `
    ${SIGNALS_SELECT_BASE}
    ${joinClause}
    ${whereClause}
    ${SIGNALS_SELECT_ORDER_LIMIT}
  `;
  const boundedLimit = Math.min(100, Math.max(1, Math.floor(Number.isFinite(limit) ? limit : 20)));
  const rows = db3.prepare(sql).all(...allBinds, boundedLimit);
  const signals = rows.map(rowToNotification);
  if (markRead && signals.length > 0) {
    const now = utcNow();
    const insertRead = db3.prepare(SIGNAL_READS_INSERT_IGNORE);
    for (const n of signals) {
      insertRead.run(n.signal_id, agentId2, now);
    }
  }
  return { count: signals.length, signals, unread_only: unreadOnly };
}
function resolveNotification(db3, params) {
  const { notificationIds = [], threadId = null, cwd, agentId: agentId2 = null } = params;
  const hasExplicitScope = params.workspacePath != null || params.artifact != null;
  const scope = hasExplicitScope ? fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    cwd ?? process.cwd()
  ) : { workspace_path: null, artifact: null, repo: null, ref: null };
  const resolved = [];
  const now = utcNow();
  if (notificationIds.length > 0) {
    const ph = notificationIds.map(() => "?").join(",");
    const where = [`signal_id IN (${ph})`, "status = 'open'"];
    const binds = [...notificationIds];
    appendSignalScope(where, binds, scope, "");
    if (agentId2) {
      where.push("(from_agent = ? OR to_agent = ? OR to_agent IS NULL)");
      binds.push(agentId2, agentId2);
    }
    const rows = db3.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(" AND ")} RETURNING signal_id`
    ).all(now, ...binds);
    resolved.push(...rows.map((r) => r.signal_id));
  }
  if (threadId) {
    const where = ["thread_id = ?", "status = 'open'"];
    const binds = [threadId];
    appendSignalScope(where, binds, scope, "");
    if (agentId2) {
      where.push("(from_agent = ? OR to_agent = ? OR to_agent IS NULL)");
      binds.push(agentId2, agentId2);
    }
    const rows = db3.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(" AND ")} RETURNING signal_id`
    ).all(now, ...binds);
    resolved.push(...rows.map((r) => r.signal_id));
  }
  return { resolved: resolved.length, signal_ids: [...new Set(resolved)] };
}
function signalRecord(n) {
  return { ...n, to_agents: n.to_agent ? [n.to_agent] : [] };
}
function requireSignalText(value, field) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`agent_signal ${field} is required`);
  }
  return value;
}
function acknowledgeNotifications(db3, agentId2, signalIds = [], threadId = null, params = {}) {
  const where = ["status = 'open'", "(to_agent IS NULL OR to_agent = ?)", "from_agent <> ?"];
  const binds = [agentId2, agentId2];
  if (signalIds.length > 0) {
    where.push(`signal_id IN (${signalIds.map(() => "?").join(",")})`);
    binds.push(...signalIds);
  }
  if (threadId) {
    where.push("thread_id = ?");
    binds.push(threadId);
  }
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    params.cwd ?? process.cwd()
  );
  if (signalIds.length === 0) {
    appendSignalScope(where, binds, scope, "");
  }
  const rows = db3.prepare(`SELECT signal_id FROM signals WHERE ${where.join(" AND ")}`).all(...binds);
  const ids = rows.map((r) => r.signal_id);
  const uniqueIds = [...new Set(ids)];
  if (uniqueIds.length === 0) return { acknowledged: 0, signal_ids: [] };
  const now = utcNow();
  const insertRead = db3.prepare(SIGNAL_READS_INSERT_IGNORE);
  let acknowledged = 0;
  for (const id of uniqueIds) {
    const result = insertRead.run(id, agentId2, now);
    acknowledged += result.changes;
  }
  return { acknowledged, signal_ids: uniqueIds };
}
function agentSignal(db3, params) {
  switch (params.action) {
    case "publish":
    case "reply": {
      const toAgents = params.toAgents?.length ? params.toAgents : [null];
      const results = toAgents.map((toAgent) => insertNotification(db3, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        repo: params.repo,
        ref: params.ref,
        toAgent,
        kind: params.action === "reply" ? "reply" : params.kind ?? "fyi",
        subject: requireSignalText(params.subject, "subject"),
        body: params.body ?? null,
        files: params.files ?? [],
        refIds: params.refs ?? [],
        inReplyTo: params.inReplyTo ?? null,
        importance: params.importance ?? 5,
        cwd: params.cwd
      }));
      return {
        action: params.action,
        signal_id: results[0].signal_id,
        signal_ids: results.map((r) => r.signal_id),
        thread_id: results[0].thread_id,
        workspace_path: results[0].workspace_path,
        artifact: results[0].artifact
      };
    }
    case "list": {
      const result = getNotifications(db3, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        repo: params.repo,
        ref: params.ref,
        kinds: params.kinds ?? [],
        threadId: params.threadId ?? null,
        unreadOnly: params.unreadOnly ?? true,
        markRead: params.markRead ?? false,
        limit: params.limit ?? 20,
        cwd: params.cwd
      });
      return {
        action: "list",
        count: result.count,
        signals: result.signals.map(signalRecord),
        unread_only: result.unread_only
      };
    }
    case "resolve": {
      const result = resolveNotification(db3, {
        agentId: params.agentId,
        notificationIds: params.signalIds ?? [],
        threadId: params.threadId ?? null,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        cwd: params.cwd
      });
      return { action: "resolve", ...result };
    }
    case "ack": {
      return {
        action: "ack",
        ...acknowledgeNotifications(db3, params.agentId, params.signalIds ?? [], params.threadId ?? null, {
          workspacePath: params.workspacePath,
          artifact: params.artifact,
          cwd: params.cwd
        })
      };
    }
  }
}
function pruneNotifications(db3, params) {
  const { notificationIds = [], resolvedOnly = false, olderThanDays, dryRun = false, cwd } = params;
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    cwd ?? process.cwd()
  );
  const where = [];
  const binds = [];
  if (notificationIds.length > 0) {
    where.push(`signal_id IN (${notificationIds.map(() => "?").join(",")})`);
    binds.push(...notificationIds);
  }
  if (resolvedOnly) {
    where.push("status = 'resolved'");
  }
  if (olderThanDays != null) {
    const cutoff = new Date(Date.now() - olderThanDays * 864e5).toISOString();
    where.push("created_at < ?");
    binds.push(cutoff);
  }
  if (notificationIds.length === 0) {
    appendSignalScope(where, binds, scope, "");
  }
  if (where.length === 0) {
    return { deleted: 0, signal_ids: [] };
  }
  const whereClause = where.join(" AND ");
  const rows = db3.prepare(
    `${SIGNALS_SELECT_IDS_FOR_PRUNE} ${whereClause}`
  ).all(...binds);
  const ids = rows.map((r) => r.signal_id);
  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, signal_ids: ids };
  }
  if (ids.length > 0) {
    const ph = ids.map(() => "?").join(",");
    db3.prepare(SIGNALS_DELETE_BY_IDS(ph)).run(...ids);
    db3.prepare(SIGNAL_READS_DELETE_BY_SIGNAL_IDS(ph)).run(...ids);
  }
  return { deleted: ids.length, signal_ids: ids };
}

// src/maintenance.ts
var SESSION_CAPTURE_FILE_LIMIT = 20;
var SESSION_CAPTURE_VISIBLE_FILE_LIMIT = 10;
var SESSION_CAPTURE_RUN_DETAIL_LIMIT = 3;
var SESSION_CAPTURE_RUN_FILE_LIMIT = 3;
var SESSION_CAPTURE_TEXT_LIMIT = 120;
var MAX_WAIT_MS = 36e5;
var MAX_RETRY_MS = 3e5;
var DEFAULT_WAIT_MS = 6e4;
var DEFAULT_RETRY_MS = 5e3;
function compactText(value, max = SESSION_CAPTURE_TEXT_LIMIT) {
  const normalized = value.replace(/\s+/g, " ").trim();
  return normalized.length > max ? `${normalized.slice(0, max - 3)}...` : normalized;
}
function listSummary(label, items, visibleLimit = SESSION_CAPTURE_VISIBLE_FILE_LIMIT) {
  if (items.length === 0) return null;
  const shown = items.slice(0, visibleLimit);
  const omitted = items.length - shown.length;
  return `${label}${omitted > 0 ? ` (showing ${shown.length} of ${items.length})` : ""}: ${shown.join(", ")}${omitted > 0 ? `; ${omitted} omitted` : ""}.`;
}
function boundedMs(value, defaultMs, minMs, maxMs) {
  if (value == null) return defaultMs;
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return minMs;
  return Math.min(Math.max(numeric, minMs), maxMs);
}
function pruneStale(db3, params = {}) {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) : params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId2 = typeof params.agent_id === "string" ? params.agent_id : typeof params.agentId === "string" ? params.agentId : null;
  const rawWorkspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const rawTarget = params.target_file ?? params.targetFile;
  const targetFiles = (Array.isArray(rawTarget) ? rawTarget : rawTarget != null ? [rawTarget] : []).map(String).filter(Boolean).map((file) => {
    const base = rawWorkspacePath ? resolve10(rawWorkspacePath) : process.cwd();
    return canonicalizePath(isAbsolute5(file) ? resolve10(file) : resolve10(base, file));
  });
  const now = utcNow();
  const ageCutoff = olderThanMinutes != null && !expiredOnly ? new Date(Date.now() - olderThanMinutes * 6e4).toISOString() : null;
  const conditions = [];
  const binds = [];
  const staleClauses = ["(l.expires_at IS NOT NULL AND l.expires_at < ?)"];
  binds.push(now);
  if (ageCutoff) {
    staleClauses.push("(l.acquired_at < ?)");
    binds.push(ageCutoff);
  }
  conditions.push(`(${staleClauses.join(" OR ")})`);
  if (agentId2) {
    conditions.push("t.agent_id = ?");
    binds.push(agentId2);
  }
  if (targetFiles.length > 0) {
    conditions.push(`l.file_path IN (${targetFiles.map(() => "?").join(",")})`);
    binds.push(...targetFiles);
  }
  if (workspacePath) {
    conditions.push("t.workspace_path = ?");
    binds.push(workspacePath);
  }
  if (artifact2) {
    conditions.push("(t.artifact = ? OR t.artifact IS NULL)");
    binds.push(artifact2);
  }
  const where = conditions.join(" AND ");
  const from = "locks l JOIN task_runs t ON t.run_id = l.run_id";
  let staleLocks = [];
  try {
    staleLocks = db3.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds);
  } catch {
  }
  if (dryRun) {
    return { pruned_locks: 0, dry_run: true, would_prune: staleLocks.length };
  }
  if (staleLocks.length === 0) {
    return { pruned_locks: 0 };
  }
  db3.exec("BEGIN IMMEDIATE");
  try {
    staleLocks = db3.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds);
    if (staleLocks.length === 0) {
      db3.exec("COMMIT");
      return { pruned_locks: 0 };
    }
    const ph = staleLocks.map(() => "?").join(",");
    db3.prepare(`DELETE FROM locks WHERE lock_id IN (${ph})`).run(...staleLocks.map((l) => l.lock_id));
    db3.exec("COMMIT");
  } catch (e) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
  return { pruned_locks: staleLocks.length };
}
function openRefinementCount(db3, params = {}) {
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    params.cwd ?? process.cwd()
  );
  const queryParams = [];
  let sql = "SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";
  if (!params.includeHandoffs) sql += " AND quality NOT IN ('handoff','instructions')";
  if (scope.workspace_path) {
    sql += " AND (workspace_path = ? OR workspace_path IS NULL)";
    queryParams.push(scope.workspace_path);
  }
  if (scope.artifact) {
    sql += " AND (artifact = ? OR artifact IS NULL)";
    queryParams.push(scope.artifact);
  }
  if (scope.repo) {
    sql += " AND (repo = ? OR repo IS NULL)";
    queryParams.push(scope.repo);
  }
  if (scope.ref) {
    sql += " AND (ref = ? OR ref IS NULL)";
    queryParams.push(scope.ref);
  }
  return db3.prepare(sql).get(...queryParams).c;
}
var BRIEFING_LABELS = ["GOTCHA", "BUG", "DECISION", "IMPROVEMENT", "ARCHITECTURE", "SECURITY"];
function notifyGet(db3, params = {}) {
  const wsPath = params.workspace ?? null;
  const artifact2 = normalizeArtifact(params.artifact);
  const format = params.format ?? "json";
  const agentId2 = String(params.agent_id ?? params.agentId ?? "agent");
  const notifyCwd = wsPath ?? params.cwd ?? process.cwd();
  const items = [];
  try {
    const inbox = getNotifications(db3, {
      agentId: agentId2,
      workspacePath: wsPath,
      artifact: artifact2,
      unreadOnly: true,
      markRead: false,
      limit: 5,
      cwd: notifyCwd
    });
    for (const n of inbox.signals) {
      const target = n.to_agent ? `to ${n.to_agent}` : "broadcast";
      const shownFiles = n.files.slice(0, 3);
      const fileSuffix = shownFiles.length > 0 ? ` files=${shownFiles.join(", ")}${n.files.length > shownFiles.length ? ` (+${n.files.length - shownFiles.length})` : ""}` : "";
      const bodySuffix = n.body ? ` \u2014 ${n.body.slice(0, 120)}` : "";
      items.push({
        kind: "notification",
        text: `\u{1F4E8} ${n.kind} from ${n.from_agent} (${target}): ${n.subject}${bodySuffix}${fileSuffix}`,
        importance: n.importance
      });
    }
  } catch {
  }
  try {
    const overrideConds = ["state = 'ACTIVE'", "label = 'OVERRIDE'"];
    const overrideBinds = [];
    if (wsPath) {
      overrideConds.push("(workspace_path = ? OR workspace_path IS NULL)");
      overrideBinds.push(wsPath);
    }
    if (artifact2) {
      overrideConds.push("(artifact = ? OR artifact IS NULL)");
      overrideBinds.push(artifact2);
    }
    const overrideRows = db3.prepare(
      `SELECT memory_id, observation, importance
       FROM memories
       WHERE ${overrideConds.join(" AND ")}
       ORDER BY importance DESC, last_accessed_at DESC
       LIMIT 2`
    ).all(...overrideBinds);
    for (const m of overrideRows) {
      items.push({
        kind: "memory",
        text: `OVERRIDE(${m.importance}): ${m.observation.slice(0, 120)}`,
        importance: m.importance
      });
    }
  } catch {
  }
  try {
    const conditions = [
      "state = 'ACTIVE'",
      "importance >= 6",
      `label IN (${BRIEFING_LABELS.map(() => "?").join(",")})`
    ];
    const bindParams = [...BRIEFING_LABELS];
    if (wsPath) {
      conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
      bindParams.push(wsPath);
    }
    if (artifact2) {
      conditions.push("(artifact = ? OR artifact IS NULL)");
      bindParams.push(artifact2);
    }
    const memRows = db3.prepare(
      `SELECT memory_id, observation, label, importance
       FROM memories
       WHERE ${conditions.join(" AND ")}
       ORDER BY importance DESC, last_accessed_at DESC
       LIMIT 3`
    ).all(...bindParams);
    for (const m of memRows) {
      items.push({
        kind: "memory",
        text: `${m.label}(${m.importance}): ${m.observation.slice(0, 120)}`,
        importance: m.importance
      });
    }
  } catch {
  }
  try {
    const wkConditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
    const wkParams = [];
    if (wsPath) {
      wkConditions.push("(workspace_path = ? OR workspace_path IS NULL)");
      wkParams.push(wsPath);
    }
    if (artifact2) {
      wkConditions.push("(artifact = ? OR artifact IS NULL)");
      wkParams.push(artifact2);
    }
    const topWk = db3.prepare(
      `SELECT failure_signature, count(*) AS freq, avg(importance) AS avg_imp
       FROM memories
       WHERE ${wkConditions.join(" AND ")}
       GROUP BY failure_signature HAVING freq >= 2
       ORDER BY freq * avg_imp DESC LIMIT 1`
    ).get(...wkParams);
    if (topWk) {
      items.push({
        kind: "weakness",
        text: `\u26A0\uFE0F Recurring: ${topWk.failure_signature} (${topWk.freq}x, avg imp ${Math.round(topWk.avg_imp)})`
      });
    }
  } catch {
  }
  try {
    const refCount = openRefinementCount(db3, { workspacePath: wsPath, artifact: artifact2, cwd: notifyCwd });
    if (refCount > 0) {
      items.push({ kind: "refinement", text: `\u{1F4CB} ${refCount} open refinement(s) pending` });
    }
  } catch {
  }
  if (items.length === 0) {
    return { ok: true, count: 0, notifications: [] };
  }
  const result = {
    ok: true,
    count: items.length,
    notifications: items
  };
  if (format === "hook") {
    const hookItems = items.slice(0, 5);
    result.count = hookItems.length;
    result.notifications = hookItems;
    const lines = [
      `\u{1F9E0} Brief (${hookItems.length}${items.length > hookItems.length ? `/${items.length}` : ""}):`,
      ...hookItems.map((i) => `  \u2022 ${i.text}`)
    ];
    const additionalContext = lines.join("\n");
    const sessionId2 = String(params.session_id ?? params.sessionId ?? "-");
    const normalizedScope = fillScope(
      {
        workspace_path: wsPath,
        artifact: artifact2,
        repo: params.repo ?? null,
        ref: params.ref ?? null
      },
      notifyCwd
    );
    const scopeKey = JSON.stringify([
      sessionId2,
      normalizedScope.workspace_path,
      normalizedScope.artifact,
      normalizedScope.repo,
      normalizedScope.ref
    ]);
    const fingerprint = createHash2("sha256").update(additionalContext).digest("hex");
    const delivery = { consumerId: agentId2, channel: "briefing", scopeKey };
    if (getDeliveryFingerprint(db3, delivery) === fingerprint) {
      return { ok: true, count: 0, notifications: [] };
    }
    setDeliveryFingerprint(db3, { ...delivery, fingerprint });
    result.additionalContext = additionalContext;
  }
  return result;
}
function parseGitStatusShortLines(stdout) {
  const files = [];
  for (const rawLine of String(stdout).split("\n")) {
    if (!rawLine || rawLine.length < 4) continue;
    const xy = rawLine.slice(0, 2);
    let pathPart = rawLine.slice(3);
    if (xy.includes("R") || xy.includes("C")) {
      const arrow = pathPart.indexOf(" -> ");
      if (arrow >= 0) pathPart = pathPart.slice(arrow + 4);
    }
    const filePath = pathPart.trim();
    if (filePath) files.push(filePath);
  }
  return files;
}
function gitDirtyFiles(workspacePath) {
  if (!workspacePath) return [];
  try {
    const result = spawnSync3("git", ["-C", workspacePath, "status", "--porcelain=v1"], {
      encoding: "utf8",
      timeout: 5e3
    });
    if (result.status !== 0) return [];
    return parseGitStatusShortLines(String(result.stdout));
  } catch {
    return [];
  }
}
function sessionCapture(db3, params = {}) {
  const agentId2 = String(params.agent_id ?? params.agentId ?? "agent");
  const reason = params.reason ? String(params.reason) : null;
  const workspaceInput = params.workspace ?? params.workspace_path ?? params.workspacePath;
  const rawWorkspacePath = typeof workspaceInput === "string" && workspaceInput.trim() ? resolve10(workspaceInput.trim()) : null;
  const scope = fillScope(
    {
      workspace_path: rawWorkspacePath,
      artifact: normalizeArtifact(params.artifact),
      repo: params.repo ?? null,
      ref: params.ref ?? null
    },
    params.cwd ?? process.cwd()
  );
  const workspacePath = scope.workspace_path ?? rawWorkspacePath ?? process.cwd();
  const runWorkspaceCandidates = [...new Set([workspacePath, rawWorkspacePath].filter((value) => Boolean(value)))];
  const artifact2 = scope.artifact;
  const workspacePlaceholders = runWorkspaceCandidates.map(() => "?").join(",");
  const runRows2 = db3.prepare(
    `SELECT tr.run_id, tr.rationale, tr.test_plan, tr.context_ref, tr.status, tr.created_at, tr.updated_at,
            COALESCE((SELECT json_group_array(rf.file_path)
              FROM run_files rf WHERE rf.run_id = tr.run_id), '[]') AS files_json
     FROM task_runs tr
     WHERE tr.agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path IN (${workspacePlaceholders}) OR workspace_path IS NULL)
       AND (? IS NULL OR artifact = ? OR artifact IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId2, ...runWorkspaceCandidates, artifact2, artifact2);
  const files = [...new Set(runRows2.flatMap((row) => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeRuns = runRows2.filter((row) => row.status === "ACTIVE").length;
  const pendingRuns = runRows2.filter((row) => row.status === "PENDING").length;
  let consolidationOpportunities = 0;
  try {
    const cConds = ["novelty_score IS NOT NULL", "novelty_score < 0.2", "state = 'ACTIVE'"];
    const cBinds = [];
    if (workspacePath) {
      cConds.push("(workspace_path = ? OR workspace_path IS NULL)");
      cBinds.push(workspacePath);
    }
    if (artifact2) {
      cConds.push("(artifact = ? OR artifact IS NULL)");
      cBinds.push(artifact2);
    }
    consolidationOpportunities = db3.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE ${cConds.join(" AND ")}`
    ).get(...cBinds).c;
  } catch {
  }
  if (runRows2.length === 0 && dirtyFiles.length === 0) {
    return {
      ok: true,
      captured: false,
      refinement_id: null,
      pending_runs: 0,
      active_runs: 0,
      files: [],
      dirty_files: [],
      reason,
      consolidation_opportunities: consolidationOpportunities
    };
  }
  const now = utcNow();
  const refinementId = "ref_" + randomUUID9().replace(/-/g, "");
  const allCapturedFiles = [.../* @__PURE__ */ new Set([...files, ...dirtyFiles])];
  const capturedFiles = allCapturedFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const capturedDirtyFiles = dirtyFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const statusSummary = runRows2.slice(0, SESSION_CAPTURE_RUN_DETAIL_LIMIT).map((row) => {
    const rowFiles2 = parseJsonList(row.files_json);
    const shownFiles = rowFiles2.slice(0, SESSION_CAPTURE_RUN_FILE_LIMIT);
    const omittedFiles = rowFiles2.length - shownFiles.length;
    const fileSuffix = rowFiles2.length > 0 ? ` files=${shownFiles.join(", ")}${omittedFiles > 0 ? ` (+${omittedFiles} more)` : ""}` : "";
    const planSuffix = row.context_ref ? ` plan=${row.context_ref}` : "";
    return `${row.status} ${row.run_id}: ${compactText(row.rationale)}; verify=${compactText(row.test_plan)}${planSuffix}${fileSuffix}`;
  });
  const omittedRunDetails = runRows2.length - statusSummary.length;
  const reasoning = [
    `Session capture for ${agentId2}${reason ? ` (${reason})` : ""}.`,
    `Unresolved runs: ${runRows2.length} (${activeRuns} active, ${pendingRuns} pending).`,
    listSummary("Dirty files", dirtyFiles),
    statusSummary.length > 0 ? `Run details: ${statusSummary.join(" | ")}${omittedRunDetails > 0 ? ` | ${omittedRunDetails} more runs omitted` : ""}` : null
  ].filter(Boolean).join(" ");
  const remember = [
    `Review session handoff for ${agentId2}: ${activeRuns} active and ${pendingRuns} pending runs remain.`,
    listSummary("Touched files", allCapturedFiles),
    dirtyFiles.length > 0 ? "Check dirty git state before continuing." : null,
    pendingRuns > 0 ? "Run the recorded verification before claiming completion." : null
  ].filter(Boolean).join(" ");
  const existing = db3.prepare(
    `SELECT refinement_id FROM refinements
      WHERE agent_id = ? AND workspace_path = ? AND artifact IS ? AND repo IS ? AND ref IS ?
        AND quality = 'handoff' AND state IN ('open', 'ongoing')
        AND files_json = ? AND reasoning = ? AND remember = ?
      ORDER BY datetime(updated_at) DESC LIMIT 1`
  ).get(
    agentId2,
    workspacePath,
    artifact2,
    scope.repo,
    scope.ref,
    JSON.stringify(capturedFiles),
    reasoning,
    remember
  );
  if (existing) {
    return {
      ok: true,
      captured: false,
      deduplicated: true,
      refinement_id: existing.refinement_id,
      pending_runs: pendingRuns,
      active_runs: activeRuns,
      files: capturedFiles,
      dirty_files: capturedDirtyFiles,
      file_count: allCapturedFiles.length,
      dirty_file_count: dirtyFiles.length,
      omitted_files: Math.max(0, allCapturedFiles.length - capturedFiles.length),
      omitted_dirty_files: Math.max(0, dirtyFiles.length - capturedDirtyFiles.length),
      reason,
      consolidation_opportunities: consolidationOpportunities
    };
  }
  db3.prepare(
    `INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, repo, ref,
       artifact, files_json, reasoning, remember, quality, state, created_at, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'handoff', 'open', ?, ?)`
  ).run(
    refinementId,
    agentId2,
    workspacePath,
    scope.repo,
    scope.ref,
    artifact2,
    JSON.stringify(capturedFiles),
    reasoning,
    remember,
    now,
    now
  );
  return {
    ok: true,
    captured: true,
    refinement_id: refinementId,
    pending_runs: pendingRuns,
    active_runs: activeRuns,
    files: capturedFiles,
    dirty_files: capturedDirtyFiles,
    file_count: allCapturedFiles.length,
    dirty_file_count: dirtyFiles.length,
    omitted_files: Math.max(0, allCapturedFiles.length - capturedFiles.length),
    omitted_dirty_files: Math.max(0, dirtyFiles.length - capturedDirtyFiles.length),
    reason,
    consolidation_opportunities: consolidationOpportunities
  };
}
function waitForLock(db3, params = {}) {
  const targetFiles = Array.isArray(params.target_files) ? params.target_files : Array.isArray(params.targetFiles) ? params.targetFiles : [];
  const agentId2 = params.agent_id ?? params.agentId ?? "agent";
  const rawWorkspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const waitMs = boundedMs(params.wait_ms ?? params.waitMs, DEFAULT_WAIT_MS, 0, MAX_WAIT_MS);
  const retryMs = boundedMs(params.retry_interval_ms ?? params.retryIntervalMs, DEFAULT_RETRY_MS, 1, MAX_RETRY_MS);
  const start = Date.now();
  if (targetFiles.length === 0) {
    return { ok: true, waited_ms: 0, lock_free: true };
  }
  const root = rawWorkspacePath ? resolve10(rawWorkspacePath) : process.cwd();
  const absTargetFiles = targetFiles.map((file) => canonicalizePath(isAbsolute5(file) ? resolve10(file) : resolve10(root, file)));
  const ph = absTargetFiles.map(() => "?").join(",");
  const scopeClauses = [];
  const scopeBinds = [];
  if (workspacePath) {
    scopeClauses.push("AND ai.workspace_path = ?");
    scopeBinds.push(workspacePath);
  }
  if (artifact2) {
    scopeClauses.push("AND (ai.artifact = ? OR ai.artifact IS NULL)");
    scopeBinds.push(artifact2);
  }
  const lockStmt = db3.prepare(
    `SELECT fl.file_path, ai.agent_id, fl.expires_at
     FROM locks fl
     JOIN task_runs ai ON ai.run_id = fl.run_id
     WHERE fl.file_path IN (${ph})
       AND ai.agent_id <> ?
       AND ai.status = 'ACTIVE'
       ${scopeClauses.join("\n       ")}
       AND (fl.expires_at IS NULL OR fl.expires_at > ?)`
  );
  const sleepBuf = new Int32Array(new SharedArrayBuffer(4));
  const checkLocks = () => {
    evictExpiredLocks(db3);
    const now = (/* @__PURE__ */ new Date()).toISOString();
    return lockStmt.all(...absTargetFiles, agentId2, ...scopeBinds, now);
  };
  function sleepMs(ms) {
    Atomics.wait(sleepBuf, 0, 0, ms);
  }
  let conflicts = checkLocks();
  const waited = () => Date.now() - start;
  while (conflicts.length > 0 && waited() < waitMs) {
    sleepMs(Math.min(retryMs, waitMs - waited()));
    conflicts = checkLocks();
  }
  const elapsed = waited();
  if (conflicts.length === 0) {
    return { ok: true, waited_ms: elapsed, lock_free: true };
  }
  return {
    ok: true,
    waited_ms: elapsed,
    lock_free: false,
    conflicts: conflicts.map((c) => ({ file_path: c.file_path, agent_id: c.agent_id, expires_at: c.expires_at }))
  };
}
function digest(db3, params = {}) {
  const retentionDays = Number(params.retention_days ?? 90);
  const handoffRetentionDays = Number(params.refinement_handoff_retention_days ?? params.refinementHandoffRetentionDays ?? 7);
  const doneRetentionDays = Number(params.refinement_done_retention_days ?? params.refinementDoneRetentionDays ?? 30);
  const rawWorkspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const now = (/* @__PURE__ */ new Date()).toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 864e5).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 864e5).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 864e5).toISOString();
  const memoryScope = [];
  const memoryScopeBinds = [];
  if (workspacePath) {
    memoryScope.push("workspace_path = ?");
    memoryScopeBinds.push(workspacePath);
  }
  if (artifact2) {
    memoryScope.push("artifact = ?");
    memoryScopeBinds.push(artifact2);
  }
  const memoryScopeSql = memoryScope.length > 0 ? ` AND ${memoryScope.join(" AND ")}` : "";
  const refinementScope = [];
  const refinementScopeBinds = [];
  if (workspacePath) {
    refinementScope.push("workspace_path = ?");
    refinementScopeBinds.push(workspacePath);
  }
  if (artifact2) {
    refinementScope.push("artifact = ?");
    refinementScopeBinds.push(artifact2);
  }
  const refinementScopeSql = refinementScope.length > 0 ? ` AND ${refinementScope.join(" AND ")}` : "";
  if (params.dry_run) {
    const wouldArchive = db3.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).get(now, ...memoryScopeBinds).c;
    const wouldPruneOld = db3.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).get(cutoff, ...memoryScopeBinds).c;
    const lockDryRun = pruneStale(db3, {
      ...workspacePath ? { workspace: workspacePath } : {},
      ...artifact2 ? { artifact: artifact2 } : {},
      expired_only: true,
      dry_run: true
    });
    const wouldPruneLocks = lockDryRun.would_prune ?? 0;
    const wouldPruneRefinements = db3.prepare(`SELECT COUNT(*) AS c FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`).get(handoffCutoff, doneCutoff, ...refinementScopeBinds).c;
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      pruned_refinements: 0,
      fts_rebuilt: false,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements
    };
  }
  const archiveRes = db3.prepare(
    `UPDATE memories
     SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
     WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
  ).run(now, now, now, ...memoryScopeBinds);
  const deleteRes = db3.prepare(
    `DELETE FROM memories
     WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
  ).run(cutoff, ...memoryScopeBinds);
  const { pruned_locks } = pruneStale(db3, {
    ...workspacePath ? { workspace: workspacePath } : {},
    ...artifact2 ? { artifact: artifact2 } : {},
    expired_only: true
  });
  const pruneRefinementsRes = db3.prepare(
    `DELETE FROM refinements
     WHERE ((quality = 'handoff' AND updated_at < ?)
        OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`
  ).run(handoffCutoff, doneCutoff, ...refinementScopeBinds);
  let ftsRebuilt = false;
  try {
    if (hasFts(db3)) {
      rebuildFts(db3);
      ftsRebuilt = true;
    }
  } catch {
  }
  checkpointWal(db3);
  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks,
    pruned_refinements: pruneRefinementsRes.changes,
    fts_rebuilt: ftsRebuilt
  };
}
function getWorkspaceStatus(db3, params = {}) {
  const rawWsPath = params.workspace_path ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  evictExpiredLocks(db3);
  const memoryScope = ["state = 'ACTIVE'"];
  const memoryScopeParams = [];
  if (wsPath) {
    memoryScope.push("(workspace_path = ? OR workspace_path IS NULL)");
    memoryScopeParams.push(wsPath);
  }
  if (artifact2) {
    memoryScope.push("(artifact = ? OR artifact IS NULL)");
    memoryScopeParams.push(artifact2);
  }
  const activeMemories = db3.prepare(
    `SELECT COUNT(*) AS c FROM memories WHERE ${memoryScope.join(" AND ")}`
  ).get(...memoryScopeParams).c;
  const runScopeParts = [];
  const runScopeParams = [];
  if (wsPath) {
    runScopeParts.push("workspace_path = ?");
    runScopeParams.push(wsPath);
  }
  if (artifact2) {
    runScopeParts.push("(artifact = ? OR artifact IS NULL)");
    runScopeParams.push(artifact2);
  }
  const runScope = runScopeParts.length > 0 ? ` AND ${runScopeParts.join(" AND ")}` : "";
  const pendingRuns = db3.prepare(
    `SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING'${runScope}`
  ).get(...runScopeParams).c;
  const activeRuns = db3.prepare(
    `SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'${runScope}`
  ).get(...runScopeParams).c;
  const planScopeParts = [];
  const planScopeParams = [];
  if (wsPath) {
    planScopeParts.push("p.workspace_path = ?");
    planScopeParams.push(wsPath);
  }
  if (artifact2) {
    planScopeParts.push("(p.artifact = ? OR p.artifact IS NULL)");
    planScopeParams.push(artifact2);
  }
  const planScope = planScopeParts.length > 0 ? ` AND ${planScopeParts.join(" AND ")}` : "";
  const activePlans = db3.prepare(
    `SELECT COUNT(*) AS c FROM plans p WHERE p.status IN ('DRAFT','ACTIVE','PAUSED')${planScope}`
  ).get(...planScopeParams).c;
  const readyTasks = db3.prepare(`SELECT COUNT(*) AS c FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
    WHERE t.status = 'OPEN'${planScope}
      AND NOT EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id AND c.expires_at > ?)
      AND NOT EXISTS (
        SELECT 1 FROM task_dependencies td JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
      )`).get(...planScopeParams, utcNow()).c;
  const inProgressTasks = db3.prepare(
    `SELECT COUNT(*) AS c FROM tasks t JOIN plans p ON p.plan_id = t.plan_id WHERE t.status = 'IN_PROGRESS'${planScope}`
  ).get(...planScopeParams).c;
  const verifyTasks = db3.prepare(
    `SELECT COUNT(*) AS c FROM tasks t JOIN plans p ON p.plan_id = t.plan_id WHERE t.status = 'VERIFY'${planScope}`
  ).get(...planScopeParams).c;
  const openRefinements = openRefinementCount(db3, {
    workspacePath: wsPath,
    artifact: artifact2,
    repo: params.repo,
    cwd: params.cwd
  });
  const lockWhereParts = [];
  const lockParams = [];
  if (wsPath) {
    lockWhereParts.push("ai.workspace_path = ?");
    lockParams.push(wsPath);
  }
  if (artifact2) {
    lockWhereParts.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    lockParams.push(artifact2);
  }
  const lockWhere = lockWhereParts.length > 0 ? `WHERE ${lockWhereParts.join(" AND ")}` : "";
  const locks = db3.prepare(
    `SELECT fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact, fl.run_id,
            'EXCLUSIVE' AS lock_type, fl.acquired_at, fl.expires_at
     FROM locks fl
     JOIN task_runs ai ON ai.run_id = fl.run_id
     ${lockWhere}
     ORDER BY fl.acquired_at DESC
     LIMIT 50`
  ).all(...lockParams);
  return {
    ok: true,
    active_memories: activeMemories,
    pending_runs: pendingRuns,
    active_runs: activeRuns,
    active_plans: activePlans,
    ready_tasks: readyTasks,
    in_progress_tasks: inProgressTasks,
    verify_tasks: verifyTasks,
    open_refinements: openRefinements,
    locks
  };
}
function exportMemoryDoc(db3, params = {}) {
  const rawWsPath = params.workspace_path ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const now = (/* @__PURE__ */ new Date()).toISOString().slice(0, 10);
  const conds = ["m.state = 'ACTIVE'"];
  const bindParams = [];
  if (wsPath) {
    conds.push("(m.workspace_path = ? OR m.workspace_path IS NULL)");
    bindParams.push(wsPath);
  }
  if (artifact2) {
    conds.push("(m.artifact = ? OR m.artifact IS NULL)");
    bindParams.push(artifact2);
  }
  const rows = db3.prepare(
    `SELECT m.memory_id, m.label, m.importance, m.task_context, m.observation,
            m.tags_json, m.repo, m.ref, m.failure_signature, m.created_at
     FROM memories m
     WHERE ${conds.join(" AND ")}
     ORDER BY m.importance DESC, m.created_at DESC`
  ).all(...bindParams);
  if (rows.length > 0) {
    const refs = db3.prepare(
      `SELECT r.memory_id, r.reference
       FROM memory_refs r
       JOIN memories m ON m.memory_id = r.memory_id
       WHERE ${conds.join(" AND ")}
       ORDER BY r.memory_id, r.ordinal`
    ).all(...bindParams);
    const refsByMemory = /* @__PURE__ */ new Map();
    for (const ref of refs) {
      const list = refsByMemory.get(ref.memory_id) ?? [];
      list.push(ref.reference);
      refsByMemory.set(ref.memory_id, list);
    }
    for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
  }
  const byLabel = {};
  for (const row of rows) {
    const label = row.label ?? "OTHER";
    (byLabel[label] ??= []).push(row);
  }
  const lines = [
    `# Memory Store Report \u2014 ${now}`,
    "",
    `**Total active memories:** ${rows.length}`,
    `**By label:** ${Object.entries(byLabel).map(([l, ms]) => `${l}(${ms.length})`).join(", ")}`,
    ""
  ];
  for (const [label, mems] of Object.entries(byLabel)) {
    lines.push(`## ${label}`, "");
    for (const m of mems) {
      const tags = parseJsonList(m.tags_json);
      lines.push(
        `### \`${m.memory_id}\` \u2014 importance ${m.importance}`,
        `**Context:** ${m.task_context}`,
        `**Observation:** ${m.observation}`
      );
      if (tags.length) lines.push(`**Tags:** ${tags.join(", ")}`);
      if (m.references.length) lines.push(`**References:** ${m.references.join(", ")}`);
      if (m.failure_signature) lines.push(`**Failure signature:** ${m.failure_signature}`);
      if (m.repo) lines.push(`**Repo:** ${m.repo}${m.ref ? ` @ ${m.ref}` : ""}`);
      lines.push(`**Created:** ${m.created_at.slice(0, 10)}`, "");
    }
  }
  return lines.join("\n");
}
function exportHarness(db3, params = {}) {
  const limit = Number(params.limit ?? 10);
  const minImportance = Number(params.min_importance ?? params.minImportance ?? 7);
  const rawWsPath = params.workspace_path ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const harnessOnly = Boolean(params.harness_only ?? params.harnessOnly ?? false);
  const scopeConds = [];
  const scopeParams = [];
  if (wsPath) {
    scopeConds.push("(workspace_path = ? OR workspace_path IS NULL)");
    scopeParams.push(wsPath);
  }
  if (artifact2) {
    scopeConds.push("(artifact = ? OR artifact IS NULL)");
    scopeParams.push(artifact2);
  }
  const scopeSql = scopeConds.length > 0 ? `AND ${scopeConds.join(" AND ")}` : "";
  const harnessRows = db3.prepare(
    `SELECT memory_id, label, importance, observation
     FROM memories
     WHERE state = 'ACTIVE'
       AND tags_json LIKE '%"harness"%'
       ${scopeSql}
     ORDER BY importance DESC, access_count DESC
     LIMIT ?`
  ).all(...scopeParams, limit);
  const memories = [];
  for (const r of harnessRows) {
    memories.push({ memory_id: r.memory_id, label: r.label, importance: r.importance, observation: r.observation, tier: "harness" });
  }
  if (!harnessOnly && memories.length < limit) {
    const harnessIds = new Set(memories.map((m) => m.memory_id));
    const remaining = limit - memories.length;
    const generalRows = db3.prepare(
      `SELECT memory_id, label, importance, observation
       FROM memories
       WHERE state = 'ACTIVE'
         AND importance >= ?
         AND label <> 'EXPERIENCE'
         AND tags_json NOT LIKE '%"harness"%'
         ${scopeSql}
       ORDER BY importance DESC, access_count DESC, last_accessed_at DESC
       LIMIT ?`
    ).all(minImportance, ...scopeParams, remaining * 2);
    for (const r of generalRows) {
      if (!harnessIds.has(r.memory_id) && memories.length < limit) {
        memories.push({ memory_id: r.memory_id, label: r.label, importance: r.importance, observation: r.observation, tier: "general" });
      }
    }
  }
  if (memories.length === 0) {
    return {
      count: 0,
      harness_count: 0,
      markdown: "<!-- No harness or high-importance memories to export -->",
      memories: [],
      next: 'No harness proposals yet. Use octocode-awareness reflect record --fix-harness "<proposal>" after evidence shows a reusable harness gap.'
    };
  }
  const harnessCount = memories.filter((m) => m.tier === "harness").length;
  const lines = [
    "## Agent lessons (generated by octocode-awareness \xB7 reflect export-harness)",
    "",
    "<!-- Tier 1: harness proposals from reflect record --fix-harness: -->",
    ""
  ];
  const harnessMems = memories.filter((m) => m.tier === "harness");
  const generalMems = memories.filter((m) => m.tier === "general");
  for (const m of harnessMems) {
    lines.push(`- **[HARNESS:${m.importance}]** ${m.observation}`);
  }
  if (generalMems.length > 0) {
    lines.push("", "<!-- Tier 2: high-importance general lessons -->", "");
    for (const m of generalMems) {
      lines.push(`- **[${m.label}:${m.importance}]** ${m.observation}`);
    }
  }
  lines.push("");
  return {
    count: memories.length,
    harness_count: harnessCount,
    markdown: lines.join("\n"),
    memories,
    next: "Human review required: apply approved guidance to its owning AGENTS.md, SKILL.md, or doc; run that surface's verification and skill review; then record the outcome with octocode-awareness reflect record. Run repo inject only when workspace projections should refresh."
  };
}

// src/verify.ts
import { randomUUID as randomUUID10 } from "node:crypto";
var VALID_VERIFY_STATUSES = /* @__PURE__ */ new Set(["SUCCESS", "FAILED"]);
function targetFilesForRun(db3, runId) {
  return db3.prepare("SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path").all(runId).map((row) => String(row.file_path));
}
function closeRunFiles(db3, runId, now) {
  db3.prepare("DELETE FROM locks WHERE run_id = ?").run(runId);
  db3.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
    WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, runId);
}
function finishLinkedTask(db3, runId, status, agentId2, now, message) {
  const linked = db3.prepare("SELECT task_id FROM task_runs WHERE run_id = ?").get(runId);
  if (!linked?.task_id) return;
  const taskStatus = status === "SUCCESS" ? "DONE" : "FAILED";
  const updated = db3.prepare(`UPDATE tasks SET status = ?, updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status = 'VERIFY'`).run(taskStatus, now, now, linked.task_id);
  if (updated.changes === 0) return;
  db3.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`).run(
    `tevt_${randomUUID10().replace(/-/g, "")}`,
    linked.task_id,
    runId,
    agentId2,
    status === "SUCCESS" ? "VERIFIED" : "VERIFICATION_FAILED",
    message ?? taskStatus,
    now
  );
}
function abandonLinkedTask(db3, runId, agentId2, now, message) {
  const linked = db3.prepare("SELECT task_id FROM task_runs WHERE run_id = ?").get(runId);
  if (!linked?.task_id) return;
  const updated = db3.prepare(`UPDATE tasks SET status = 'FAILED', updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status IN ('IN_PROGRESS', 'VERIFY')`).run(now, now, linked.task_id);
  if (updated.changes === 0) return;
  db3.prepare("DELETE FROM task_claims WHERE task_id = ?").run(linked.task_id);
  db3.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, 'ABANDONED', ?, ?)`).run(`tevt_${randomUUID10().replace(/-/g, "")}`, linked.task_id, runId, agentId2, message, now);
}
function auditUnverified(db3, params = {}) {
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const where = ["status = 'PENDING'"];
  const binds = [];
  if (params.agentId) {
    where.push("agent_id = ?");
    binds.push(params.agentId);
  }
  if (workspacePath) {
    where.push("workspace_path = ?");
    binds.push(workspacePath);
  }
  const artifact2 = normalizeArtifact(params.artifact);
  if (artifact2) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact2);
  }
  const rows = db3.prepare(
    `SELECT run_id, agent_id, status, test_plan, context_ref, rationale, workspace_path, artifact, created_at
     FROM task_runs
     WHERE ${where.join(" AND ")}
     ORDER BY created_at ASC`
  ).all(...binds);
  const unverified = rows.map((r) => ({
    run_id: r.run_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    context_ref: r.context_ref,
    rationale: r.rationale,
    target_files: targetFilesForRun(db3, r.run_id),
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at
  }));
  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db3.prepare(RUNS_UPDATE_PENDING_TO_FAILED).run(now, intent.run_id);
      closeRunFiles(db3, intent.run_id, now);
      abandonLinkedTask(db3, intent.run_id, intent.agent_id, now, "pending run abandoned by verification audit");
      try {
        db3.prepare(RUN_LOG_INSERT_ABANDONED).run(
          "evt_" + randomUUID10().replace(/-/g, ""),
          intent.run_id,
          intent.agent_id,
          now
        );
      } catch {
      }
    }
  }
  const staleActive = [];
  try {
    const nowIso = utcNow();
    const staleWhere = [
      "ai.status = 'ACTIVE'",
      "EXISTS (SELECT 1 FROM run_files any_rf WHERE any_rf.run_id = ai.run_id)",
      `NOT EXISTS (
        SELECT 1 FROM run_files active_rf
        WHERE active_rf.run_id = ai.run_id AND active_rf.ended_at IS NULL
          AND active_rf.expires_at > ?
      )`,
      `NOT EXISTS (
        SELECT 1 FROM task_claims tc
        WHERE tc.run_id = ai.run_id AND tc.expires_at > ?
      )`
    ];
    const staleBinds = [nowIso, nowIso];
    if (params.agentId) {
      staleWhere.push("ai.agent_id = ?");
      staleBinds.push(params.agentId);
    }
    if (workspacePath) {
      staleWhere.push("ai.workspace_path = ?");
      staleBinds.push(workspacePath);
    }
    if (artifact2) {
      staleWhere.push("(ai.artifact = ? OR ai.artifact IS NULL)");
      staleBinds.push(artifact2);
    }
    const staleRows = db3.prepare(
      `SELECT ai.run_id, ai.agent_id, ai.rationale, ai.context_ref, ai.workspace_path, ai.artifact, ai.created_at
       FROM task_runs ai
       WHERE ${staleWhere.join(" AND ")}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds);
    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        run_id: r.run_id,
        agent_id: r.agent_id,
        status: "ACTIVE",
        rationale: r.rationale,
        context_ref: r.context_ref,
        target_files: targetFilesForRun(db3, r.run_id),
        workspace_path: r.workspace_path,
        artifact: r.artifact,
        created_at: r.created_at,
        age_hours: Math.round(ageMs / 36e5 * 10) / 10
      });
    }
  } catch (e) {
    if (!(e instanceof Error && e.message.includes("no such table"))) throw e;
  }
  if (params.abandon && staleActive.length > 0) {
    const now = utcNow();
    for (const intent of staleActive) {
      db3.prepare(RUNS_UPDATE_ACTIVE_TO_FAILED).run(now, intent.run_id);
      closeRunFiles(db3, intent.run_id, now);
      abandonLinkedTask(db3, intent.run_id, intent.agent_id, now, "stale task run abandoned by verification audit");
      try {
        db3.prepare(RUN_LOG_INSERT_STALE_ABANDONED).run(
          "evt_" + randomUUID10().replace(/-/g, ""),
          intent.run_id,
          intent.agent_id,
          now
        );
      } catch {
      }
    }
  }
  const total = unverified.length + staleActive.length;
  return { ok: true, unverified, stale_active: staleActive, count: total };
}
function markVerified(db3, params) {
  const { agentId: agentId2 = "agent", allPending = false, message } = params;
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const runId = params.runId ?? "";
  const status = params.status ?? "SUCCESS";
  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" \u2014 must be SUCCESS or FAILED`,
      run_id: runId || null
    };
  }
  if (allPending) {
    const dynWhere = [
      workspacePath ? " AND workspace_path = ?" : "",
      artifact2 ? " AND (artifact = ? OR artifact IS NULL)" : ""
    ].join("");
    const selectSql = RUNS_SELECT_PENDING_IDS.replace("{DYNAMIC_WHERE}", dynWhere);
    const selectBinds = [agentId2];
    if (workspacePath) selectBinds.push(workspacePath);
    if (artifact2) selectBinds.push(artifact2);
    db3.exec("BEGIN IMMEDIATE");
    try {
      const rows = db3.prepare(selectSql).all(...selectBinds);
      const now2 = utcNow();
      const ids = [];
      for (const row of rows) {
        const upd = db3.prepare(RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
          status,
          now2,
          row.run_id,
          agentId2
        );
        if (upd.changes === 0) continue;
        closeRunFiles(db3, row.run_id, now2);
        finishLinkedTask(db3, row.run_id, status, agentId2, now2, message);
        ids.push(row.run_id);
        if (message) {
          try {
            db3.prepare(RUN_LOG_INSERT_VERIFIED).run(
              "evt_" + randomUUID10().replace(/-/g, ""),
              row.run_id,
              agentId2,
              message,
              now2
            );
          } catch {
          }
        }
      }
      db3.exec("COMMIT");
      const warning = !workspacePath && !artifact2 && ids.length > 0 ? `marked ${ids.length} pending run(s) across ALL workspaces for agent "${agentId2}" \u2014 no --workspace/--artifact scope given; pass --workspace to limit` : void 0;
      return { ok: true, run_id: null, run_ids: ids, count: ids.length, status, updated_at: now2, ...warning ? { warning } : {} };
    } catch (e) {
      try {
        db3.exec("ROLLBACK");
      } catch {
      }
      throw e;
    }
  }
  if (!runId) {
    return { ok: false, error: "--run-id is required (or use --all-pending)", run_id: null };
  }
  const now = utcNow();
  db3.exec("BEGIN IMMEDIATE");
  try {
    const result = db3.prepare(RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
      status,
      now,
      runId,
      agentId2
    );
    if (result.changes === 0) {
      db3.exec("ROLLBACK");
      const row = db3.prepare(RUNS_SELECT_STATUS).get(runId);
      if (!row) {
        return { ok: false, error: `no run found with run_id=${runId}`, run_id: runId };
      }
      if (row.agent_id !== agentId2) {
        return {
          ok: false,
          error: `run ${runId} belongs to agent "${row.agent_id}", not "${agentId2}"`,
          run_id: runId
        };
      }
      return {
        ok: false,
        error: `run ${runId} has status "${row.status}" \u2014 only PENDING runs can be verified`,
        run_id: runId
      };
    }
    if (message) {
      try {
        db3.prepare(RUN_LOG_INSERT_VERIFIED).run(
          "evt_" + randomUUID10().replace(/-/g, ""),
          runId,
          agentId2,
          message,
          now
        );
      } catch {
      }
    }
    closeRunFiles(db3, runId, now);
    finishLinkedTask(db3, runId, status, agentId2, now, message);
    db3.exec("COMMIT");
    return { ok: true, run_id: runId, status, updated_at: now };
  } catch (e) {
    try {
      db3.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
}

// src/agents.ts
function registerAgent(db3, params) {
  const agentId2 = params.agentId;
  const agentName2 = params.agentName ?? "";
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const context = params.context ?? null;
  const now = utcNow();
  db3.prepare(AGENTS_UPSERT).run(agentId2, agentName2, workspacePath, artifact2, context, now, now);
  return { agent_id: agentId2, agent_name: agentName2, workspace_path: workspacePath, artifact: artifact2, context, registered_at: now, last_seen_at: now };
}
function listAgents(db3, params = {}) {
  try {
    const binds = [];
    let sql = AGENTS_LIST_SELECT;
    const clauses = [];
    if (params.workspacePath) {
      clauses.push(AGENTS_LIST_CLAUSE_WORKSPACE_PATH);
      binds.push(normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? params.workspacePath);
    }
    const artifact2 = normalizeArtifact(params.artifact);
    if (artifact2) {
      clauses.push(AGENTS_LIST_CLAUSE_ARTIFACT);
      binds.push(artifact2);
    }
    if (clauses.length > 0) sql += ` WHERE ${clauses.join(" AND ")}`;
    sql += ` ${AGENTS_LIST_ORDER}`;
    const rows = db3.prepare(sql).all(...binds);
    return { count: rows.length, agents: rows };
  } catch {
    return { count: 0, agents: [] };
  }
}

// src/hooks-install.ts
import {
  closeSync,
  existsSync as existsSync3,
  mkdirSync as mkdirSync3,
  openSync,
  readFileSync as readFileSync2,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync as writeFileSync2
} from "node:fs";
import { randomUUID as randomUUID11 } from "node:crypto";
import { homedir as homedir2 } from "node:os";
import { dirname as dirname4, isAbsolute as isAbsolute6, join as join5, relative as relative3, resolve as resolve11, sep as sep2 } from "node:path";
var WRITE_MATCHER = "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch";
var HOSTS = /* @__PURE__ */ new Set(["claude", "codex", "cursor"]);
var CONFIG_LOCK_WAIT = new Int32Array(new SharedArrayBuffer(4));
var CONFIG_LOCK_RETRY_MS = 25;
var CONFIG_LOCK_TIMEOUT_MS = 1e4;
var CONFIG_LOCK_STALE_MS = 3e4;
function hooksInstallUsage() {
  return `usage: octocode-awareness hooks install|check|remove [options]

Install, check, dry-run, or remove octocode-awareness lifecycle hooks.

Targets:
  --host claude         Write Claude Code hooks to .claude/settings.json (install default).
  --host codex         Write Codex hooks to .codex/hooks.json.
  --host cursor        Write Cursor hooks to .cursor/hooks.json.
  Pi                   No shell install target; use wirePiAwarenessHooks(pi) or @octocodeai/pi-extension.

Options:
  --project-dir <path>  Target a project hook file under <path> (default: cwd).
  --global              Target the user hook file with absolute hook paths.
  --check               Report whether the hooks are installed.
  --strict              With --check, exit 2 if hooks are missing or drifted.
  --dry-run             Print the resulting settings without writing.
  --compact             Minify JSON output when supported.
  --remove              Remove only octocode-awareness hooks.`;
}
function flag(argv, value) {
  return argv.includes(value);
}
function opt(argv, name, fallback) {
  const index = argv.indexOf(name);
  return index >= 0 && argv[index + 1] ? argv[index + 1] : fallback;
}
function fail(message, extra = {}) {
  return { exitCode: 1, payload: { ok: false, error: message, ...extra } };
}
function requestedHost(argv) {
  return opt(argv, "--host", "claude").toLowerCase();
}
function targetConfig(host) {
  switch (host) {
    case "codex":
      return { dir: ".codex", file: "hooks.json" };
    case "cursor":
      return { dir: ".cursor", file: "hooks.json" };
    case "claude":
      return { dir: ".claude", file: "settings.json" };
  }
}
function loadSettings(settingsPath) {
  if (!existsSync3(settingsPath)) return {};
  const raw = readFileSync2(settingsPath, "utf8");
  const parsed = JSON.parse(raw);
  return parsed && typeof parsed === "object" ? parsed : {};
}
function processIsAlive(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code !== "ESRCH";
  }
}
function removeStaleConfigLock(lockPath) {
  try {
    const owner = Number.parseInt(readFileSync2(lockPath, "utf8"), 10);
    const staleByAge = Date.now() - statSync(lockPath).mtimeMs > CONFIG_LOCK_STALE_MS;
    if (processIsAlive(owner) && !staleByAge) return false;
    unlinkSync(lockPath);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return true;
    return false;
  }
}
function acquireConfigLock(settingsPath) {
  mkdirSync3(dirname4(settingsPath), { recursive: true });
  const lockPath = `${settingsPath}.octocode-awareness.lock`;
  const deadline = Date.now() + CONFIG_LOCK_TIMEOUT_MS;
  for (; ; ) {
    try {
      const fd = openSync(lockPath, "wx", 384);
      try {
        writeFileSync2(fd, `${process.pid}
`, "utf8");
      } finally {
        closeSync(fd);
      }
      return () => {
        try {
          unlinkSync(lockPath);
        } catch (error) {
          if (error.code !== "ENOENT") throw error;
        }
      };
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
      if (removeStaleConfigLock(lockPath)) continue;
      if (Date.now() >= deadline) {
        throw new Error(`timed out waiting for concurrent hook update: ${settingsPath}`);
      }
      Atomics.wait(CONFIG_LOCK_WAIT, 0, 0, CONFIG_LOCK_RETRY_MS);
    }
  }
}
function writeSettingsAtomic(settingsPath, settings) {
  mkdirSync3(dirname4(settingsPath), { recursive: true });
  const temporaryPath = `${settingsPath}.tmp-${process.pid}-${randomUUID11()}`;
  try {
    writeFileSync2(temporaryPath, JSON.stringify(settings, null, 2) + "\n");
    renameSync(temporaryPath, settingsPath);
  } finally {
    try {
      unlinkSync(temporaryPath);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
}
function hookCommand(name, params) {
  const abs = join5(params.hookDir, name);
  if (params.host === "codex" || params.host === "cursor" || params.globalMode) return abs;
  const rel = relative3(params.projectDir, abs);
  if (rel && !rel.startsWith("..") && !isAbsolute6(rel)) {
    return "${CLAUDE_PROJECT_DIR}/" + rel.split(sep2).join("/");
  }
  return abs;
}
function specsFor(host, params) {
  const command2 = (name) => hookCommand(name, { host, ...params });
  if (host === "cursor") {
    return [
      { event: "preToolUse", matcher: WRITE_MATCHER, command: command2("pre-edit.sh") },
      { event: "postToolUse", matcher: WRITE_MATCHER, command: command2("post-edit.sh") },
      { event: "stop", command: command2("stop-verify.sh") },
      { event: "subagentStop", command: command2("stop-verify.sh") },
      { event: "sessionEnd", command: command2("session-end.sh") },
      { event: "preCompact", command: command2("session-end.sh") },
      { event: "sessionStart", command: command2("notify-deliver.sh") }
    ];
  }
  if (host === "codex") {
    return [
      { event: "PreToolUse", matcher: WRITE_MATCHER, command: command2("pre-edit.sh") },
      { event: "PostToolUse", matcher: WRITE_MATCHER, command: command2("post-edit.sh") },
      { event: "Stop", command: command2("stop-verify.sh") },
      { event: "SubagentStop", command: command2("stop-verify.sh") },
      { event: "PreCompact", command: command2("session-end.sh") },
      { event: "UserPromptSubmit", command: command2("notify-deliver.sh") }
    ];
  }
  return [
    { event: "PreToolUse", matcher: WRITE_MATCHER, command: command2("pre-edit.sh") },
    { event: "PostToolUse", matcher: WRITE_MATCHER, command: command2("post-edit.sh") },
    { event: "Stop", command: command2("stop-verify.sh") },
    { event: "SubagentStop", command: command2("stop-verify.sh") },
    { event: "SessionEnd", command: command2("session-end.sh") },
    { event: "UserPromptSubmit", command: command2("notify-deliver.sh") }
  ];
}
function obsoleteSpecsFor(host, params) {
  return [{
    event: host === "cursor" ? "preToolUse" : "PreToolUse",
    matcher: WRITE_MATCHER,
    command: hookCommand("harness-guard.sh", { host, ...params })
  }];
}
function entry(host, spec) {
  if (host === "cursor") {
    return {
      command: spec.command,
      timeout: 20,
      ...spec.matcher ? { matcher: spec.matcher } : {}
    };
  }
  return {
    ...spec.matcher ? { matcher: spec.matcher } : {},
    hooks: [{ type: "command", command: spec.command, timeout: 20 }]
  };
}
function awarenessHookName(command2) {
  const normalized = command2?.replace(/\\/g, "/");
  if (!normalized) return null;
  const marker = "/octocode-awareness/scripts/hooks/";
  const index = normalized.lastIndexOf(marker);
  return index >= 0 ? normalized.slice(index + marker.length) : null;
}
function sameAwarenessCommand(actual, expected) {
  if (actual === expected) return true;
  const actualHook = awarenessHookName(actual);
  const expectedHook = awarenessHookName(expected);
  return actualHook !== null && expectedHook !== null && actualHook === expectedHook;
}
function hasCommand(groups, command2) {
  return (groups ?? []).some((group) => sameAwarenessCommand(group.command, command2) || (group.hooks ?? []).some((hook) => sameAwarenessCommand(hook.command, command2)));
}
function matcherMatches(actual, expected) {
  return expected ? actual === expected : actual == null;
}
function isExactHookEntry(host, group, spec) {
  if (host === "cursor") {
    return group.command === spec.command && group.timeout === 20 && matcherMatches(group.matcher, spec.matcher) && !Array.isArray(group.hooks);
  }
  return matcherMatches(group.matcher, spec.matcher) && (group.hooks ?? []).some((hook) => hook.type === "command" && hook.command === spec.command && hook.timeout === 20);
}
function hasExactCommand(groups, host, spec) {
  return (groups ?? []).some((group) => isExactHookEntry(host, group, spec));
}
function matchingCommandCount(groups, command2) {
  let count = 0;
  for (const group of groups ?? []) {
    if (sameAwarenessCommand(group.command, command2)) count += 1;
    count += (group.hooks ?? []).filter((hook) => sameAwarenessCommand(hook.command, command2)).length;
  }
  return count;
}
function hasDriftedCommand(groups, host, spec) {
  for (const group of groups ?? []) {
    if (host === "cursor") {
      if (sameAwarenessCommand(group.command, spec.command) && !isExactHookEntry(host, group, spec)) {
        return true;
      }
      continue;
    }
    for (const hook of group.hooks ?? []) {
      if (!sameAwarenessCommand(hook.command, spec.command)) continue;
      const exact = matcherMatches(group.matcher, spec.matcher) && hook.type === "command" && hook.timeout === 20;
      if (!exact) return true;
    }
  }
  return false;
}
function hookStatusKey(spec) {
  return `${spec.event}:${spec.command.split(/[\\/]/).pop()}`;
}
function removeCommand(groups, command2) {
  let removed = false;
  const out = [];
  for (const group of groups ?? []) {
    if (sameAwarenessCommand(group.command, command2)) {
      removed = true;
      continue;
    }
    if (!Array.isArray(group.hooks)) {
      out.push(group);
      continue;
    }
    const hooks = group.hooks.filter((hook) => {
      if (sameAwarenessCommand(hook.command, command2)) {
        removed = true;
        return false;
      }
      return true;
    });
    if (hooks.length > 0) out.push({ ...group, hooks });
  }
  return { groups: out, removed };
}
function runHooksInstall(argv, options) {
  const hostValue = requestedHost(argv);
  const writes = !flag(argv, "--help") && !flag(argv, "-h") && !flag(argv, "--check") && !flag(argv, "--dry-run") && !(flag(argv, "--global") && argv.includes("--project-dir")) && HOSTS.has(hostValue);
  if (!writes) return runHooksInstallUnlocked(argv, options);
  const host = hostValue;
  const cwd = options.cwd ?? process.cwd();
  const home = options.homeDir ?? homedir2();
  const config = targetConfig(host);
  const settingsPath = flag(argv, "--global") ? join5(home, config.dir, config.file) : join5(resolve11(opt(argv, "--project-dir", cwd)), config.dir, config.file);
  let release;
  try {
    release = acquireConfigLock(settingsPath);
    return runHooksInstallUnlocked(argv, options);
  } catch (error) {
    return fail(`cannot update ${settingsPath}: ${error.message}`);
  } finally {
    release?.();
  }
}
function runHooksInstallUnlocked(argv, options) {
  if (flag(argv, "--help") || flag(argv, "-h")) {
    return { exitCode: 0, text: hooksInstallUsage() + "\n" };
  }
  if (flag(argv, "--global") && argv.includes("--project-dir")) {
    return fail("use either --global or --project-dir, not both");
  }
  if (flag(argv, "--check") && !argv.includes("--host")) {
    return fail("hooks check requires --host claude, --host codex, or --host cursor");
  }
  const hostValue = requestedHost(argv);
  if (!HOSTS.has(hostValue)) {
    return fail("invalid --host; expected claude, codex, or cursor", { host: hostValue });
  }
  const host = hostValue;
  const cwd = options.cwd ?? process.cwd();
  const home = options.homeDir ?? homedir2();
  const globalMode = flag(argv, "--global");
  const projectDir = resolve11(opt(argv, "--project-dir", cwd));
  const config = targetConfig(host);
  const settingsPath = globalMode ? join5(home, config.dir, config.file) : join5(projectDir, config.dir, config.file);
  let settings;
  try {
    settings = loadSettings(settingsPath);
  } catch (error) {
    return fail(`cannot parse ${settingsPath}: ${error.message}`);
  }
  const specs = specsFor(host, {
    globalMode,
    projectDir,
    hookDir: options.hookDir
  });
  const obsoleteSpecs = obsoleteSpecsFor(host, {
    globalMode,
    projectDir,
    hookDir: options.hookDir
  });
  const checks = specs.map((spec) => {
    const groups = settings.hooks?.[spec.event];
    const present = hasCommand(groups, spec.command);
    const exact = hasExactCommand(groups, host, spec);
    const matchingCount = matchingCommandCount(groups, spec.command);
    const drifted = present && (!exact || hasDriftedCommand(groups, host, spec) || matchingCount > 1);
    return {
      key: hookStatusKey(spec),
      event: spec.event,
      hook: spec.command.split(/[\\/]/).pop(),
      installed: exact,
      present,
      matching_count: matchingCount,
      drifted,
      expected: {
        matcher: spec.matcher ?? null,
        command: spec.command,
        timeout: 20,
        shape: host === "cursor" ? "flat" : "nested"
      }
    };
  });
  const hooks = Object.fromEntries(checks.map((check) => [check.key, check.installed]));
  const obsolete = obsoleteSpecs.filter((spec) => hasCommand(settings.hooks?.[spec.event], spec.command)).map(hookStatusKey);
  const status = {
    host,
    settingsPath,
    hooks,
    installed_all: checks.every((check) => check.installed) && obsolete.length === 0,
    missing: checks.filter((check) => !check.present).map((check) => check.key),
    drifted: [...checks.filter((check) => check.drifted).map((check) => check.key), ...obsolete],
    details: Object.fromEntries(checks.map((check) => [check.key, check]))
  };
  if (flag(argv, "--check")) {
    const strict = flag(argv, "--strict");
    return {
      exitCode: strict && (!status.installed_all || status.drifted.length > 0) ? 2 : 0,
      payload: { ok: status.installed_all && status.drifted.length === 0, action: "check", strict, installed: status }
    };
  }
  let changed = false;
  settings.hooks ??= {};
  if (host === "cursor" && !flag(argv, "--remove") && settings.version == null) {
    settings.version = 1;
    changed = true;
  }
  for (const spec of obsoleteSpecs) {
    const result = removeCommand(settings.hooks[spec.event], spec.command);
    if (!result.removed) continue;
    changed = true;
    if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
    else delete settings.hooks[spec.event];
  }
  if (flag(argv, "--remove")) {
    for (const spec of specs) {
      const result = removeCommand(settings.hooks[spec.event], spec.command);
      if (result.removed) {
        changed = true;
        if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
        else delete settings.hooks[spec.event];
      }
    }
    if (Object.keys(settings.hooks).length === 0) delete settings.hooks;
  } else {
    const checksByKey = new Map(checks.map((check) => [check.key, check]));
    for (const spec of specs) {
      const groups = settings.hooks[spec.event] ?? [];
      settings.hooks[spec.event] = groups;
      const check = checksByKey.get(hookStatusKey(spec));
      if (!check?.installed || check.drifted) {
        const pruned = removeCommand(groups, spec.command);
        settings.hooks[spec.event] = pruned.groups;
        settings.hooks[spec.event].push(entry(host, spec));
        changed = true;
      }
    }
  }
  if (flag(argv, "--dry-run")) {
    return {
      exitCode: 0,
      payload: { ok: true, action: "dry-run", host, changed, settingsPath, resultingSettings: settings }
    };
  }
  if (changed) {
    writeSettingsAtomic(settingsPath, settings);
  }
  return {
    exitCode: 0,
    payload: {
      ok: true,
      action: flag(argv, "--remove") ? "remove" : "install",
      host,
      changed,
      settingsPath,
      note: changed ? `${settingsPath.split(/[\\/]/).pop()} updated` : "already up to date - no change"
    }
  };
}

// src/attend.ts
import { existsSync as existsSync5, readFileSync as readFileSync3, statSync as statSync2 } from "node:fs";
import { join as join7, resolve as resolve13 } from "node:path";

// src/repo-context.ts
import { spawnSync as spawnSync4 } from "node:child_process";
import { existsSync as existsSync4, mkdirSync as mkdirSync4, realpathSync as realpathSync2, writeFileSync as writeFileSync3 } from "node:fs";
import { isAbsolute as isAbsolute7, join as join6, relative as relative4, resolve as resolve12 } from "node:path";
import { fileURLToPath as fileURLToPath2 } from "node:url";
var AWARENESS_QUERY_VIEWS = [
  "all",
  "repo-profile",
  "memories",
  "gotchas",
  "lessons",
  "plans",
  "tasks",
  "runs",
  "locks",
  "agents",
  "signals",
  "refinements",
  "files",
  "activity",
  "workboard",
  "developer-review"
];
var SCOPE_CACHE = /* @__PURE__ */ new WeakMap();
var VIEW_SET = new Set(AWARENESS_QUERY_VIEWS);
var CSV_VIEWS = ["memories", "gotchas", "lessons", "plans", "tasks", "runs", "agents", "locks", "signals", "refinements", "files", "activity", "workboard"];
var PROJECTION_MARKDOWN_BUDGETS = {
  "AGENTS.md": { max_lines: 80, role: "agent start summary" },
  "MEMORY.md": { max_lines: 200, role: "active memory index" },
  "GOTCHAS.md": { max_lines: 200, role: "gotcha index" },
  "LEARN.md": { max_lines: 200, role: "lesson/opportunity index" },
  "BOOKMARKS.md": { max_lines: 200, role: "learnable resource index" },
  "DEVELOPER_REVIEW.md": { max_lines: 200, role: "agent feedback to the instruction author" }
};
var ATTEND_COMPACT_BUDGET = { max_lines: 40, max_json_bytes: 2 * 1024 };
var WORKBOARD_BUDGET = { max_rows_per_column: 10 };
var LESSON_LABELS = [
  "DECISION",
  "ARCHITECTURE",
  "WORKFLOW",
  "IMPROVEMENT",
  "DOCS",
  "TEST",
  "BUILD",
  "CONFIG",
  "PERFORMANCE",
  "REFACTOR",
  "API",
  "RELEASE",
  "FEATURE",
  "SUGGESTION",
  "SECURITY",
  "OVERRIDE"
];
function utcNow2() {
  return (/* @__PURE__ */ new Date()).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function normalizeView(view) {
  const normalized = (view ?? "all").trim().toLowerCase().replace(/_/g, "-");
  if (VIEW_SET.has(normalized)) return normalized;
  throw new Error(`unknown octocode-awareness query view "${view}". Expected one of: ${AWARENESS_QUERY_VIEWS.join(", ")}`);
}
function normalizeFormat(format) {
  const normalized = (format ?? "json").trim().toLowerCase();
  if (normalized === "json" || normalized === "table" || normalized === "csv" || normalized === "markdown" || normalized === "html") return normalized;
  throw new Error("--format must be json, table, csv, markdown, or html");
}
function normalizeMode(mode) {
  const normalized = (mode ?? "local").trim().toLowerCase();
  if (normalized === "local" || normalized === "share") return normalized;
  throw new Error("--mode must be local or share");
}
function limitOf(value, fallback = 50, max = 500) {
  if (value == null || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(1, Math.floor(value)));
}
function stringList(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value == null || value === "") return [];
  return [String(value)];
}
function scopeFromParams(params) {
  const cached = SCOPE_CACHE.get(params);
  if (cached) return cached;
  const cwd = params.cwd ? resolve12(params.cwd) : process.cwd();
  const rawWorkspace = params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd;
  const workspacePath = rawWorkspace ? resolve12(String(rawWorkspace)) : null;
  const scope = {
    // Keep the raw resolved path for projection output / echo; the alias set
    // below carries the extra keys used for DB row matching.
    workspacePath,
    workspacePaths: workspacePath ? workspaceAliases(workspacePath, cwd) : [],
    artifact: params.artifact ? String(params.artifact) : null,
    repo: params.repo ? String(params.repo) : null,
    ref: params.ref ? String(params.ref) : null
  };
  SCOPE_CACHE.set(params, scope);
  return scope;
}
function withScope(params, overrides) {
  const derived = { ...params, ...overrides };
  SCOPE_CACHE.set(derived, scopeFromParams(params));
  return derived;
}
function workspaceAliases(workspacePath, cwd) {
  const aliases = /* @__PURE__ */ new Set([workspacePath]);
  try {
    aliases.add(realpathSync2.native(workspacePath));
  } catch {
    try {
      aliases.add(realpathSync2(workspacePath));
    } catch {
    }
  }
  try {
    const gitRoot = normalizeWorkspacePath(workspacePath, cwd ?? workspacePath);
    if (gitRoot) aliases.add(gitRoot);
  } catch {
  }
  return [...aliases];
}
function addNullableScope(where, binds, scope, alias = "") {
  const p = alias ? `${alias}.` : "";
  if (scope.workspacePaths.length > 0) {
    where.push(`(${p}workspace_path IN (${scope.workspacePaths.map(() => "?").join(",")}) OR ${p}workspace_path IS NULL)`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push(`(${p}artifact = ? OR ${p}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${p}repo = ? OR ${p}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${p}ref = ? OR ${p}ref IS NULL)`);
    binds.push(scope.ref);
  }
}
function addExactScope(where, binds, scope, alias = "") {
  const p = alias ? `${alias}.` : "";
  if (scope.workspacePaths.length > 0) {
    where.push(`${p}workspace_path IN (${scope.workspacePaths.map(() => "?").join(",")})`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push(`(${p}artifact = ? OR ${p}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${p}repo = ? OR ${p}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${p}ref = ? OR ${p}ref IS NULL)`);
    binds.push(scope.ref);
  }
}
function addTextFilter(where, binds, query, columns) {
  const q = query?.trim();
  if (!q) return;
  where.push(`LOWER(${columns.map((c) => `COALESCE(${c}, '')`).join(" || ' ' || ")}) LIKE LOWER(?)`);
  binds.push(`%${q}%`);
}
function addStateFilter(where, binds, states, column, normalize = (state) => state) {
  if (states.length === 0) return;
  where.push(`${column} IN (${states.map(() => "?").join(",")})`);
  binds.push(...states.map(normalize));
}
function addLabelsFilter(where, binds, labels, column = "label") {
  if (labels.length === 0) return;
  where.push(`${column} IN (${labels.map(() => "?").join(",")})`);
  binds.push(...labels.map((l) => l.toUpperCase()));
}
function fileRefCandidates(file, workspacePath) {
  const trimmed = file.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith("file:")) return [trimmed];
  const absolute = isAbsolute7(trimmed) ? resolve12(trimmed) : resolve12(workspacePath ?? process.cwd(), trimmed);
  return [`file:${absolute}`, `%${trimmed}%`];
}
function stripLocationSuffix(value) {
  return value.trim().replace(/:(\d+)(?::\d+)?$/, "");
}
function localPathFromReference(reference, workspacePath) {
  const trimmed = reference.trim();
  if (!trimmed) return null;
  let rawPath = null;
  if (/^file:\/\//i.test(trimmed)) {
    try {
      rawPath = fileURLToPath2(trimmed);
    } catch {
      rawPath = trimmed.replace(/^file:\/+/i, "/");
    }
  } else if (/^file:/i.test(trimmed)) {
    rawPath = trimmed.slice("file:".length);
  } else if (/^path:/i.test(trimmed)) {
    rawPath = trimmed.slice("path:".length);
  } else if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) {
    return null;
  } else if (isAbsolute7(trimmed) || trimmed.startsWith("./") || trimmed.startsWith("../") || /^[^/\s]+\/.+/.test(trimmed)) {
    rawPath = trimmed;
  }
  if (!rawPath) return null;
  const clean = stripLocationSuffix(rawPath);
  return isAbsolute7(clean) ? resolve12(clean) : resolve12(workspacePath ?? process.cwd(), clean);
}
function referenceHealth(references, workspacePath) {
  const fileReferences = [];
  const existingFiles = [];
  const missingFiles = [];
  const missingReferences = [];
  for (const reference of references) {
    const localPath = localPathFromReference(reference, workspacePath);
    if (!localPath) continue;
    fileReferences.push(localPath);
    if (existsSync4(localPath)) {
      existingFiles.push(localPath);
    } else {
      missingFiles.push(localPath);
      missingReferences.push(reference);
    }
  }
  return {
    reference_count: references.length,
    file_reference_count: fileReferences.length,
    missing_reference_count: missingReferences.length,
    file_references: [...new Set(fileReferences)],
    existing_files: [...new Set(existingFiles)],
    missing_files: [...new Set(missingFiles)],
    missing_references: [...new Set(missingReferences)]
  };
}
function addMemoryFileFilter(where, binds, file, scope) {
  if (!file) return;
  const candidates = fileRefCandidates(file, scope.workspacePath);
  if (candidates.length === 0) return;
  where.push(`EXISTS (
    SELECT 1 FROM memory_refs r
    WHERE r.memory_id = memories.memory_id
      AND (${candidates.map(() => "r.reference LIKE ?").join(" OR ")})
  )`);
  binds.push(...candidates);
}
function withReferences(db3, rows) {
  if (rows.length === 0) return rows;
  const ids = rows.map((row) => row.memory_id);
  const refs = db3.prepare(
    `SELECT memory_id, reference
       FROM memory_refs
      WHERE memory_id IN (${ids.map(() => "?").join(",")})
      ORDER BY memory_id, ordinal`
  ).all(...ids);
  const map = /* @__PURE__ */ new Map();
  for (const ref of refs) {
    const list = map.get(ref.memory_id) ?? [];
    list.push(ref.reference);
    map.set(ref.memory_id, list);
  }
  for (const row of rows) row.references = map.get(row.memory_id) ?? [];
  return rows;
}
function memoryRows(db3, params, options = {}) {
  const scope = scopeFromParams(params);
  const where = ["state = 'ACTIVE'"];
  const binds = [];
  addNullableScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ["task_context", "observation", "label", "tags_json", "failure_signature"]);
  addMemoryFileFilter(where, binds, params.file, scope);
  if (options.gotchas) {
    where.push("(label = 'GOTCHA' OR failure_signature IS NOT NULL)");
  } else if (options.lessons) {
    where.push(`label IN (${LESSON_LABELS.map(() => "?").join(",")})`);
    binds.push(...LESSON_LABELS);
  } else {
    addLabelsFilter(where, binds, stringList(params.label));
  }
  const since = params.since?.trim();
  if (since) {
    where.push("created_at >= ?");
    binds.push(since);
  }
  const limit = limitOf(params.limit);
  const rows = db3.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, state, label, tags_json,
            workspace_path, artifact, repo, ref, failure_signature, created_at, updated_at
       FROM memories
      WHERE ${where.join(" AND ")}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limit);
  return withReferences(db3, rows).map((row) => {
    const references = row.references ?? [];
    return {
      memory_id: row.memory_id,
      label: row.label,
      importance: row.importance,
      task_context: row.task_context,
      observation: row.observation,
      tags: parseJsonList(row.tags_json),
      references,
      ...referenceHealth(references, scope.workspacePath),
      failure_signature: row.failure_signature,
      agent_id: row.agent_id,
      workspace_path: row.workspace_path,
      artifact: row.artifact,
      repo: row.repo,
      ref: row.ref,
      created_at: row.created_at,
      updated_at: row.updated_at
    };
  });
}
function planRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ["plan_id", "name", "objective", "lead_agent_id", "doc_dir"]);
  addStateFilter(where, binds, stringList(params.state), "status", (state) => state.toUpperCase());
  const since = params.since?.trim();
  if (since) {
    where.push("created_at >= ?");
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT plan_id, name, objective, lead_agent_id, status, workspace_path, artifact,
            doc_dir, created_at, updated_at,
            (SELECT COUNT(*) FROM plan_members pm WHERE pm.plan_id = plans.plan_id) AS member_count,
            (SELECT COUNT(*) FROM tasks t WHERE t.plan_id = plans.plan_id) AS task_count
       FROM plans
       ${sqlWhere}
      ORDER BY datetime(updated_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    plan_id: String(row["plan_id"]),
    name: String(row["name"]),
    objective: String(row["objective"]),
    lead_agent_id: String(row["lead_agent_id"]),
    status: String(row["status"]),
    doc_dir: String(row["doc_dir"]),
    member_count: Number(row["member_count"]),
    task_count: Number(row["task_count"]),
    workspace_path: row["workspace_path"] ?? null,
    artifact: row["artifact"] ?? null,
    created_at: String(row["created_at"]),
    updated_at: String(row["updated_at"])
  }));
}
function taskRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope, "p");
  addTextFilter(where, binds, params.query, ["t.task_id", "t.title", "t.reasoning", "t.acceptance_criteria", "t.created_by", "p.name"]);
  addStateFilter(where, binds, stringList(params.state), "t.status", (state) => state.toUpperCase());
  const agentId2 = params.agentId ?? params.agent_id;
  if (agentId2) {
    where.push("(t.created_by = ? OR c.agent_id = ?)");
    binds.push(agentId2, agentId2);
  }
  const since = params.since?.trim();
  if (since) {
    where.push("t.created_at >= ?");
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT t.*, p.name AS plan_name, p.status AS plan_status, p.workspace_path, p.artifact,
            c.agent_id AS claimed_by, c.run_id, c.expires_at AS claim_expires_at,
            COALESCE((SELECT json_group_array(tp.path) FROM task_paths tp WHERE tp.task_id = t.task_id), '[]') AS paths_json,
            COALESCE((SELECT json_group_array(td.depends_on_task_id) FROM task_dependencies td WHERE td.task_id = t.task_id), '[]') AS dependencies_json,
            CASE WHEN t.status = 'OPEN'
              AND c.task_id IS NULL
              AND NOT EXISTS (
                SELECT 1 FROM task_dependencies td
                JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
                WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
              ) THEN 1 ELSE 0 END AS ready
       FROM tasks t
       JOIN plans p ON p.plan_id = t.plan_id
       LEFT JOIN task_claims c ON c.task_id = t.task_id AND c.expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
       ${sqlWhere}
      ORDER BY t.priority DESC, datetime(t.created_at), t.task_id
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    task_id: String(row["task_id"]),
    plan_id: String(row["plan_id"]),
    plan_name: String(row["plan_name"]),
    plan_status: String(row["plan_status"]),
    title: String(row["title"]),
    reasoning: String(row["reasoning"]),
    acceptance_criteria: String(row["acceptance_criteria"]),
    status: String(row["status"]),
    priority: Number(row["priority"]),
    created_by: String(row["created_by"]),
    paths: parseJsonList(row["paths_json"]),
    dependencies: parseJsonList(row["dependencies_json"]),
    ready: Number(row["ready"]) === 1,
    claimed_by: row["claimed_by"] ?? null,
    run_id: row["run_id"] ?? null,
    claim_expires_at: row["claim_expires_at"] ?? null,
    workspace_path: row["workspace_path"] ?? null,
    artifact: row["artifact"] ?? null,
    created_at: String(row["created_at"]),
    updated_at: String(row["updated_at"]),
    completed_at: row["completed_at"] ?? null
  }));
}
function runRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope, "tr");
  const query = params.query?.trim();
  if (query) {
    where.push(`(LOWER(COALESCE(tr.run_id, '') || ' ' || COALESCE(tr.rationale, '') || ' ' ||
      COALESCE(tr.test_plan, '') || ' ' || COALESCE(tr.context_ref, '') || ' ' || COALESCE(tr.agent_id, '')) LIKE LOWER(?)
      OR EXISTS (SELECT 1 FROM run_files rfq WHERE rfq.run_id = tr.run_id AND LOWER(rfq.file_path) LIKE LOWER(?)))`);
    binds.push(`%${query}%`, `%${query}%`);
  }
  addStateFilter(where, binds, stringList(params.state), "tr.status", (state) => state.toUpperCase());
  const agentId2 = params.agentId ?? params.agent_id;
  if (agentId2) {
    where.push("tr.agent_id = ?");
    binds.push(agentId2);
  }
  const since = params.since?.trim();
  if (since) {
    where.push("tr.created_at >= ?");
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT tr.run_id, tr.task_id, tr.agent_id, tr.session_id, tr.rationale, tr.test_plan,
            tr.context_ref, tr.status, tr.workspace_path, tr.artifact, tr.created_at, tr.updated_at,
            COALESCE((SELECT json_group_array(rf.file_path)
              FROM run_files rf WHERE rf.run_id = tr.run_id), '[]') AS files_json
       FROM task_runs tr ${sqlWhere}
      ORDER BY datetime(tr.created_at) DESC LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    run_id: String(row["run_id"]),
    task_id: row["task_id"] ?? null,
    agent_id: String(row["agent_id"]),
    status: String(row["status"]),
    rationale: String(row["rationale"]),
    test_plan: String(row["test_plan"]),
    context_ref: row["context_ref"] ?? null,
    files: parseJsonList(row["files_json"]),
    workspace_path: row["workspace_path"] ?? null,
    artifact: row["artifact"] ?? null,
    created_at: String(row["created_at"]),
    updated_at: String(row["updated_at"])
  }));
}
function lockRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope, "t");
  addTextFilter(where, binds, params.query, ["l.file_path", "t.agent_id", "t.rationale"]);
  const agentId2 = params.agentId ?? params.agent_id;
  if (agentId2) {
    where.push("t.agent_id = ?");
    binds.push(agentId2);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT l.lock_id, l.file_path, l.run_id, t.agent_id, t.session_id, 'EXCLUSIVE' AS lock_type,
            l.acquired_at, l.expires_at, t.task_id, t.workspace_path, t.artifact, t.status
       FROM locks l
       JOIN task_runs t ON t.run_id = l.run_id
       ${sqlWhere}
      ORDER BY datetime(l.acquired_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    lock_id: String(row["lock_id"]),
    file_path: String(row["file_path"]),
    run_id: String(row["run_id"]),
    task_id: row["task_id"] ?? null,
    agent_id: String(row["agent_id"]),
    lock_type: String(row["lock_type"]),
    run_status: String(row["status"]),
    acquired_at: String(row["acquired_at"]),
    expires_at: row["expires_at"] ?? null,
    workspace_path: row["workspace_path"] ?? null,
    artifact: row["artifact"] ?? null
  }));
}
function agentRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  if (scope.workspacePaths.length > 0) {
    where.push(`(workspace_path IN (${scope.workspacePaths.map(() => "?").join(",")}) OR workspace_path IS NULL)`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(scope.artifact);
  }
  addTextFilter(where, binds, params.query, ["agent_id", "agent_name", "context"]);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
       FROM agents
       ${sqlWhere}
      ORDER BY datetime(last_seen_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows;
}
function signalRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ["subject", "body", "kind", "files_json", "refs_json", "from_agent", "to_agent"]);
  addStateFilter(where, binds, stringList(params.state), "status", (state) => state.toLowerCase());
  const agentId2 = params.agentId ?? params.agent_id;
  if (agentId2) {
    where.push("(from_agent = ? OR to_agent = ? OR to_agent IS NULL)");
    binds.push(agentId2, agentId2);
  }
  const since = params.since?.trim();
  if (since) {
    where.push("created_at >= ?");
    binds.push(since);
  }
  const includeBodies = Boolean(params.includeBodies ?? params.include_bodies);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind,
            subject, body, files_json, refs_json, thread_id, reply_to, importance, status, created_at
       FROM signals
       ${sqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    signal_id: String(row["signal_id"]),
    kind: String(row["kind"]),
    status: String(row["status"]),
    subject: String(row["subject"]),
    body: includeBodies ? row["body"] : summarize(String(row["body"] ?? ""), 160),
    from_agent: String(row["from_agent"]),
    to_agent: row["to_agent"],
    files: parseJsonList(row["files_json"]),
    refs: parseJsonList(row["refs_json"]),
    thread_id: String(row["thread_id"]),
    reply_to: row["reply_to"],
    importance: Number(row["importance"]),
    workspace_path: row["workspace_path"],
    artifact: row["artifact"],
    repo: row["repo"],
    ref: row["ref"],
    created_at: String(row["created_at"])
  }));
}
function refinementRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ["reasoning", "remember", "quality", "state", "files_json", "agent_id"]);
  addStateFilter(where, binds, stringList(params.state), "state", (state) => state.toLowerCase());
  const since = params.since?.trim();
  if (since) {
    where.push("created_at >= ?");
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json,
            reasoning, remember, quality, state, created_at, updated_at
       FROM refinements
       ${sqlWhere}
      ORDER BY
        CASE state WHEN 'open' THEN 0 WHEN 'ongoing' THEN 1 ELSE 2 END,
        datetime(updated_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    refinement_id: String(row["refinement_id"]),
    agent_id: String(row["agent_id"]),
    quality: String(row["quality"]),
    state: String(row["state"]),
    reasoning: String(row["reasoning"]),
    remember: String(row["remember"]),
    files: parseJsonList(row["files_json"]),
    workspace_path: String(row["workspace_path"]),
    artifact: row["artifact"] ?? null,
    repo: row["repo"] ?? null,
    ref: row["ref"] ?? null,
    created_at: String(row["created_at"]),
    updated_at: String(row["updated_at"])
  }));
}
function extractInstructionsFeedback(observation) {
  const marker = "instructions feedback:";
  const idx = observation.toLowerCase().indexOf(marker);
  if (idx === -1) return observation;
  const after = observation.slice(idx + marker.length);
  const end = after.search(/\s\|\s|\)\s*$/);
  return (end === -1 ? after : after.slice(0, end)).trim();
}
function developerReviewRows(db3, params) {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 60, 500);
  const refWhere = ["quality = 'instructions'"];
  const refBinds = [];
  addExactScope(refWhere, refBinds, scope);
  addTextFilter(refWhere, refBinds, params.query, ["reasoning", "remember", "files_json", "agent_id"]);
  addStateFilter(refWhere, refBinds, stringList(params.state), "state", (state) => state.toLowerCase());
  const refRows = db3.prepare(
    `SELECT refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json,
            reasoning, remember, state, created_at, updated_at
       FROM refinements
      WHERE ${refWhere.join(" AND ")}
      ORDER BY CASE state WHEN 'open' THEN 0 WHEN 'ongoing' THEN 1 ELSE 2 END, datetime(updated_at) DESC
      LIMIT ?`
  ).all(...refBinds, limit);
  const rows = refRows.map((row) => ({
    source: "refinement",
    id: String(row["refinement_id"]),
    refinement_id: String(row["refinement_id"]),
    state: String(row["state"]),
    feedback: String(row["remember"]),
    context: String(row["reasoning"]),
    files: parseJsonList(row["files_json"]),
    agent_id: String(row["agent_id"]),
    workspace_path: row["workspace_path"] ?? null,
    artifact: row["artifact"] ?? null,
    repo: row["repo"] ?? null,
    ref: row["ref"] ?? null,
    created_at: String(row["created_at"]),
    updated_at: String(row["updated_at"])
  }));
  const refTexts = refRows.map((row) => String(row["remember"] ?? "").trim()).filter(Boolean);
  const memWhere = ["state = 'ACTIVE'", `tags_json LIKE '%"developer-review"%'`];
  const memBinds = [];
  addNullableScope(memWhere, memBinds, scope);
  addTextFilter(memWhere, memBinds, params.query, ["task_context", "observation"]);
  const memRows = db3.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, created_at, updated_at
       FROM memories
      WHERE ${memWhere.join(" AND ")}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...memBinds, limit);
  for (const row of memRows) {
    const observation = String(row["observation"] ?? "");
    if (refTexts.some((text) => text && observation.includes(text))) continue;
    rows.push({
      source: "memory",
      id: String(row["memory_id"]),
      memory_id: String(row["memory_id"]),
      state: "recorded",
      feedback: extractInstructionsFeedback(observation),
      context: String(row["task_context"] ?? ""),
      importance: Number(row["importance"] ?? 0),
      files: [],
      agent_id: String(row["agent_id"]),
      created_at: String(row["created_at"]),
      updated_at: row["updated_at"] ?? null
    });
  }
  return rows.slice(0, limit);
}
function trackFile(map, filePath, source, date, workspacePath) {
  const resolved = localPathFromReference(filePath, workspacePath);
  const clean = resolved ?? stripLocationSuffix(filePath.startsWith("file:") ? filePath.slice("file:".length) : filePath);
  if (!clean) return;
  const fileExists = existsSync4(clean);
  const row = map.get(clean) ?? {
    file_path: clean,
    file_exists: fileExists,
    missing_file: !fileExists,
    memories: 0,
    gotchas: 0,
    tasks: 0,
    runs: 0,
    locks: 0,
    refinements: 0,
    signals: 0,
    edits: 0,
    last_seen_at: null
  };
  row["file_exists"] = Boolean(row["file_exists"]) || fileExists;
  row["missing_file"] = !Boolean(row["file_exists"]);
  const current = Number(row[source] ?? 0);
  row[source] = current + 1;
  if (date && (!row["last_seen_at"] || String(date) > String(row["last_seen_at"]))) row["last_seen_at"] = date;
  map.set(clean, row);
}
function fileRows2(db3, params) {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 80, 500);
  const files = /* @__PURE__ */ new Map();
  const memoryWhere = ["m.state = 'ACTIVE'", "r.reference LIKE 'file:%'"];
  const memoryBinds = [];
  addNullableScope(memoryWhere, memoryBinds, scope, "m");
  addTextFilter(memoryWhere, memoryBinds, params.query, ["r.reference", "m.task_context", "m.observation"]);
  const memoryRefs = db3.prepare(
    `SELECT r.reference, m.label, m.created_at
       FROM memory_refs r
       JOIN memories m ON m.memory_id = r.memory_id
      WHERE ${memoryWhere.join(" AND ")}
      ORDER BY datetime(m.created_at) DESC
      LIMIT ?`
  ).all(...memoryBinds, 1e3);
  for (const ref of memoryRefs) {
    trackFile(files, ref.reference, "memories", ref.created_at, scope.workspacePath);
    if (ref.label === "GOTCHA") trackFile(files, ref.reference, "gotchas", ref.created_at, scope.workspacePath);
  }
  for (const row of taskRows(db3, withScope(params, { limit: 500 }))) {
    for (const file of row["paths"]) trackFile(files, file, "tasks", String(row["created_at"]), scope.workspacePath);
  }
  for (const row of runRows(db3, withScope(params, { limit: 500 }))) {
    for (const file of row["files"]) trackFile(files, file, "runs", String(row["created_at"]), scope.workspacePath);
  }
  for (const row of lockRows(db3, withScope(params, { limit: 500 }))) {
    trackFile(files, String(row["file_path"]), "locks", String(row["acquired_at"]), scope.workspacePath);
  }
  for (const row of refinementRows(db3, withScope(params, { limit: 500 }))) {
    for (const file of row["files"]) trackFile(files, file, "refinements", String(row["updated_at"]), scope.workspacePath);
  }
  for (const row of signalRows(db3, withScope(params, { limit: 500 }))) {
    for (const file of row["files"]) trackFile(files, file, "signals", String(row["created_at"]), scope.workspacePath);
  }
  const editWhere = [];
  const editBinds = [];
  addExactScope(editWhere, editBinds, scope);
  addTextFilter(editWhere, editBinds, params.query, ["file_path", "old_file_path", "operation", "agent_id"]);
  const editSqlWhere = editWhere.length > 0 ? `WHERE ${editWhere.join(" AND ")}` : "";
  const edits = db3.prepare(
    `SELECT file_path, old_file_path, operation, agent_id, created_at
       FROM edit_log
       ${editSqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...editBinds, 1e3);
  for (const edit of edits) {
    trackFile(files, edit.file_path, "edits", edit.created_at, scope.workspacePath);
    if (edit.old_file_path) trackFile(files, edit.old_file_path, "edits", edit.created_at, scope.workspacePath);
  }
  return [...files.values()].sort((a, b) => {
    const scoreA = (a["missing_file"] ? 20 : 0) + Number(a["locks"] ?? 0) * 10 + Number(a["gotchas"] ?? 0) * 6 + Number(a["memories"] ?? 0) * 4 + Number(a["tasks"] ?? 0) * 3 + Number(a["runs"] ?? 0) + Number(a["edits"] ?? 0);
    const scoreB = (b["missing_file"] ? 20 : 0) + Number(b["locks"] ?? 0) * 10 + Number(b["gotchas"] ?? 0) * 6 + Number(b["memories"] ?? 0) * 4 + Number(b["tasks"] ?? 0) * 3 + Number(b["runs"] ?? 0) + Number(b["edits"] ?? 0);
    return scoreB - scoreA || String(b["last_seen_at"] ?? "").localeCompare(String(a["last_seen_at"] ?? ""));
  }).slice(0, limit);
}
function countWhere(db3, table, where, binds) {
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const row = db3.prepare(`SELECT COUNT(*) AS count FROM ${table} ${sqlWhere}`).get(...binds);
  return row.count;
}
function repoProfileRows(db3, params) {
  const scope = scopeFromParams(params);
  const memWhere = ["state = 'ACTIVE'"];
  const memBinds = [];
  addNullableScope(memWhere, memBinds, scope);
  const taskWhere = [];
  const taskBinds = [];
  addExactScope(taskWhere, taskBinds, scope, "p");
  const planWhere = [];
  const planBinds = [];
  addExactScope(planWhere, planBinds, scope);
  const runWhere = [];
  const runBinds = [];
  addExactScope(runWhere, runBinds, scope);
  const lockWhere = [];
  const lockBinds = [];
  addExactScope(lockWhere, lockBinds, scope, "t");
  const refinementWhere = ["state IN ('open','ongoing')"];
  const refinementBinds = [];
  addExactScope(refinementWhere, refinementBinds, scope);
  const signalWhere = ["status = 'open'"];
  const signalBinds = [];
  addExactScope(signalWhere, signalBinds, scope);
  const trackedFiles = fileRows2(db3, withScope(params, { limit: 500 }));
  return [
    { metric: "active_memories", count: countWhere(db3, "memories", memWhere, memBinds) },
    { metric: "gotchas", count: memoryRows(db3, withScope(params, { view: "gotchas", limit: 500 }), { gotchas: true }).length },
    { metric: "lessons", count: memoryRows(db3, withScope(params, { view: "lessons", limit: 500 }), { lessons: true }).length },
    { metric: "plans", count: countWhere(db3, "plans", planWhere, planBinds) },
    { metric: "tasks", count: countWhere(db3, "tasks t JOIN plans p ON p.plan_id = t.plan_id", taskWhere, taskBinds) },
    { metric: "runs", count: countWhere(db3, "task_runs", runWhere, runBinds) },
    { metric: "active_locks", count: countWhere(db3, "locks l JOIN task_runs t ON t.run_id = l.run_id", lockWhere, lockBinds) },
    { metric: "open_refinements", count: countWhere(db3, "refinements", refinementWhere, refinementBinds) },
    { metric: "open_signals", count: countWhere(db3, "signals", signalWhere, signalBinds) },
    { metric: "known_agents", count: agentRows(db3, withScope(params, { limit: 500 })).length },
    { metric: "tracked_files", count: trackedFiles.length },
    { metric: "missing_file_refs", count: trackedFiles.filter((row) => row["missing_file"]).length },
    { metric: "developer_review", count: developerReviewRows(db3, withScope(params, { limit: 500 })).length }
  ];
}
function activityRows(db3, params) {
  const limit = limitOf(params.limit);
  const rows = [];
  for (const row of memoryRows(db3, withScope(params, { limit }))) {
    rows.push({
      kind: "memory",
      id: String(row["memory_id"]),
      title: `${row["label"]}: ${summarize(String(row["task_context"]), 80)}`,
      detail: summarize(String(row["observation"]), 180),
      agent_id: String(row["agent_id"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of taskRows(db3, withScope(params, { limit }))) {
    rows.push({
      kind: "task",
      id: String(row["task_id"]),
      title: `${row["status"]}: ${summarize(String(row["title"]), 100)}`,
      detail: summarize(String(row["reasoning"]), 180),
      agent_id: String(row["claimed_by"] ?? row["created_by"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of runRows(db3, withScope(params, { limit }))) {
    rows.push({
      kind: "run",
      id: String(row["run_id"]),
      title: `${row["status"]}: ${summarize(String(row["rationale"]), 100)}`,
      detail: summarize(String(row["test_plan"]), 180),
      agent_id: String(row["agent_id"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of signalRows(db3, withScope(params, { limit }))) {
    rows.push({
      kind: "signal",
      id: String(row["signal_id"]),
      title: `${row["kind"]}: ${summarize(String(row["subject"]), 100)}`,
      detail: summarize(String(row["body"] ?? ""), 180),
      agent_id: String(row["from_agent"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of refinementRows(db3, withScope(params, { limit }))) {
    rows.push({
      kind: "refinement",
      id: String(row["refinement_id"]),
      title: `${row["state"]}: ${summarize(String(row["remember"]), 100)}`,
      detail: summarize(String(row["reasoning"]), 180),
      agent_id: String(row["agent_id"]),
      created_at: String(row["updated_at"])
    });
  }
  return rows.sort((a, b) => String(b["created_at"]).localeCompare(String(a["created_at"]))).slice(0, limit);
}
function rowFiles(row) {
  const raw = row["files"] ?? row["paths"];
  return Array.isArray(raw) ? raw.map(String) : [];
}
function displayPath(filePath, workspacePath) {
  if (!workspacePath) return filePath.replace(/\\/g, "/");
  const rel = relative4(workspacePath, filePath);
  return rel && rel !== ".." && !rel.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) && !isAbsolute7(rel) ? rel.replace(/\\/g, "/") : filePath.replace(/\\/g, "/");
}
function filesUnderWorkRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = ["tr.status = 'ACTIVE'", "rf.ended_at IS NULL", "rf.expires_at > ?"];
  const binds = [utcNow2()];
  addExactScope(where, binds, scope, "tr");
  addTextFilter(where, binds, params.query, ["rf.file_path", "tr.agent_id", "tr.rationale", "rf.reason_override", "t.title", "p.name"]);
  const agentId2 = params.agentId ?? params.agent_id;
  if (agentId2) {
    where.push("tr.agent_id = ?");
    binds.push(agentId2);
  }
  if (params.file?.trim()) {
    const root = scope.workspacePath ?? process.cwd();
    const filePath = isAbsolute7(params.file) ? resolve12(params.file) : resolve12(root, params.file);
    where.push("rf.file_path = ?");
    binds.push(filePath);
  }
  const rows = db3.prepare(
    `SELECT rf.file_path, rf.run_id, tr.agent_id, tr.task_id,
            COALESCE(rf.reason_override, t.reasoning, tr.rationale) AS reason,
            t.plan_id, p.name AS plan_name, tr.workspace_path,
            CASE WHEN l.lock_id IS NULL THEN 0 ELSE 1 END AS locked,
            l.expires_at AS lock_expires_at
       FROM run_files rf
       JOIN task_runs tr ON tr.run_id = rf.run_id
       LEFT JOIN tasks t ON t.task_id = tr.task_id
       LEFT JOIN plans p ON p.plan_id = t.plan_id
       LEFT JOIN locks l ON l.run_id = rf.run_id AND l.file_path = rf.file_path
         AND (l.expires_at IS NULL OR l.expires_at > ?)
      WHERE ${where.join(" AND ")}
      ORDER BY rf.file_path, datetime(rf.heartbeat_at) DESC, tr.agent_id, rf.run_id`
  ).all(utcNow2(), ...binds);
  const grouped = /* @__PURE__ */ new Map();
  for (const row of rows) {
    const path2 = String(row["file_path"]);
    const peers = grouped.get(path2) ?? [];
    peers.push(row);
    grouped.set(path2, peers);
  }
  return [...grouped.entries()].map(([filePath, peers]) => {
    const shown = peers.slice(0, 3);
    const lock = peers.find((peer) => Number(peer["locked"]) === 1);
    const workspacePath = String(peers[0]?.["workspace_path"] ?? scope.workspacePath ?? "") || null;
    return {
      item_type: "file",
      id: filePath,
      path: displayPath(filePath, workspacePath),
      peer_count: peers.length,
      agents: shown.map((peer) => String(peer["agent_id"])),
      run_ids: shown.map((peer) => String(peer["run_id"])),
      task_ids: shown.flatMap((peer) => peer["task_id"] == null ? [] : [String(peer["task_id"])]),
      plan_ids: shown.flatMap((peer) => peer["plan_id"] == null ? [] : [String(peer["plan_id"])]),
      plans: shown.flatMap((peer) => peer["plan_name"] == null ? [] : [String(peer["plan_name"])]),
      reasons: shown.map((peer) => summarize(String(peer["reason"] ?? ""), 80)),
      omitted_peer_count: Math.max(0, peers.length - shown.length),
      locked: Boolean(lock),
      lock_agent_id: lock == null ? null : String(lock["agent_id"]),
      lock_expires_at: lock?.["lock_expires_at"] ?? null
    };
  });
}
function pushLimited(columns, counts, column, row, limit) {
  counts[column] = (counts[column] ?? 0) + 1;
  const rows = columns[column] ?? [];
  if (rows.length < limit) {
    rows.push({ column, ...row });
    columns[column] = rows;
  }
}
function workboardRows(db3, params) {
  const limit = limitOf(params.limit, 10, 50);
  const columns = {
    Inbox: [],
    Verify: [],
    Ready: [],
    Claimed: [],
    FilesUnderWork: [],
    RecentDone: [],
    MemoryReview: [],
    DeveloperReview: [],
    ProjectionHealth: []
  };
  const counts = {};
  for (const row of filesUnderWorkRows(db3, withScope(params, { limit: 200 }))) {
    pushLimited(columns, counts, "FilesUnderWork", row, limit);
  }
  const openSignals = signalRows(db3, withScope(params, { state: ["open"], limit: 200, includeBodies: false }));
  for (const row of openSignals) {
    pushLimited(columns, counts, "Inbox", {
      item_type: "signal",
      id: String(row["signal_id"]),
      title: `${row["kind"]}: ${summarize(String(row["subject"]), 100)}`,
      detail: summarize(String(row["body"] ?? ""), 180),
      agent_id: String(row["from_agent"]),
      status: String(row["status"]),
      raw_ids: [String(row["signal_id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"])
    }, limit);
  }
  const handoffs = refinementRows(db3, withScope(params, { state: ["open", "ongoing"], limit: 200 })).filter((row) => String(row["quality"]) === "handoff");
  for (const row of handoffs) {
    pushLimited(columns, counts, "Inbox", {
      item_type: "refinement",
      id: String(row["refinement_id"]),
      title: summarize(String(row["remember"]), 100),
      detail: summarize(String(row["reasoning"]), 180),
      agent_id: String(row["agent_id"]),
      status: String(row["state"]),
      quality: String(row["quality"]),
      raw_ids: [String(row["refinement_id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"])
    }, limit);
  }
  for (const row of taskRows(db3, withScope(params, { state: ["VERIFY"], limit: 500 }))) {
    pushLimited(columns, counts, "Verify", {
      item_type: "task",
      id: String(row["task_id"]),
      title: summarize(String(row["title"]), 120),
      detail: summarize(String(row["acceptance_criteria"]), 180),
      plan_id: String(row["plan_id"]),
      agent_id: String(row["claimed_by"] ?? row["created_by"]),
      status: String(row["status"]),
      raw_ids: [String(row["task_id"]), ...row["run_id"] ? [String(row["run_id"])] : []],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"])
    }, limit);
  }
  for (const row of runRows(db3, withScope(params, { state: ["PENDING"], limit: 500 })).filter((row2) => row2["task_id"] == null)) {
    pushLimited(columns, counts, "Verify", {
      item_type: "run",
      id: String(row["run_id"]),
      title: summarize(String(row["rationale"]), 120),
      detail: summarize(String(row["test_plan"]), 180),
      agent_id: String(row["agent_id"]),
      status: String(row["status"]),
      raw_ids: [String(row["run_id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"])
    }, limit);
  }
  for (const row of taskRows(db3, withScope(params, { state: ["OPEN"], limit: 500 })).filter((row2) => row2["ready"] === true)) {
    pushLimited(columns, counts, "Ready", {
      item_type: "task",
      id: String(row["task_id"]),
      title: summarize(String(row["title"]), 120),
      detail: summarize(String(row["reasoning"]), 180),
      plan_id: String(row["plan_id"]),
      agent_id: String(row["created_by"]),
      status: String(row["status"]),
      priority: Number(row["priority"]),
      raw_ids: [String(row["task_id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"])
    }, limit);
  }
  for (const row of taskRows(db3, withScope(params, { state: ["IN_PROGRESS"], limit: 500 }))) {
    pushLimited(columns, counts, "Claimed", {
      item_type: "task",
      id: String(row["task_id"]),
      title: summarize(String(row["title"]), 120),
      detail: summarize(String(row["reasoning"]), 180),
      plan_id: String(row["plan_id"]),
      agent_id: String(row["claimed_by"]),
      status: String(row["status"]),
      raw_ids: [String(row["task_id"]), ...row["run_id"] ? [String(row["run_id"])] : []],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"]),
      expires_at: row["claim_expires_at"] ?? null
    }, limit);
  }
  for (const row of taskRows(db3, withScope(params, { state: ["DONE", "FAILED", "CANCELLED"], limit: 200 }))) {
    pushLimited(columns, counts, "RecentDone", {
      item_type: "task",
      id: String(row["task_id"]),
      title: `${row["status"]}: ${summarize(String(row["title"]), 100)}`,
      detail: summarize(String(row["acceptance_criteria"]), 180),
      plan_id: String(row["plan_id"]),
      agent_id: String(row["created_by"]),
      status: String(row["status"]),
      raw_ids: [String(row["task_id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"])
    }, limit);
  }
  for (const row of memoryRows(db3, withScope(params, { limit: 200 }))) {
    const failureSignature = String(row["failure_signature"] ?? "");
    const refs = Array.isArray(row["references"]) ? row["references"] : [];
    const missingRefs = Array.isArray(row["missing_references"]) ? row["missing_references"] : [];
    const tags = Array.isArray(row["tags"]) ? row["tags"] : [];
    const reviewReasons = [
      refs.length === 0 ? "missing_refs" : null,
      missingRefs.length > 0 ? "stale_file_refs" : null,
      failureSignature ? "failure_signature" : null,
      tags.includes("anti-bloat") ? "policy_memory" : null
    ].filter((reason) => Boolean(reason));
    if (reviewReasons.length === 0) continue;
    pushLimited(columns, counts, "MemoryReview", {
      item_type: "memory",
      id: String(row["memory_id"]),
      title: `${row["label"]}:${row["importance"]} ${summarize(String(row["task_context"]), 100)}`,
      detail: summarize(String(row["observation"]), 180),
      agent_id: String(row["agent_id"]),
      status: "review",
      reasons: reviewReasons,
      missing_reference_count: missingRefs.length,
      missing_references: missingRefs,
      raw_ids: [String(row["memory_id"])],
      files: Array.isArray(row["file_references"]) ? row["file_references"] : refs.filter((ref) => ref.startsWith("file:")).map((ref) => ref.slice("file:".length)),
      created_at: String(row["created_at"]),
      updated_at: row["updated_at"] ?? null
    }, limit);
  }
  for (const row of developerReviewRows(db3, withScope(params, { state: ["open", "ongoing"], limit: 200 }))) {
    pushLimited(columns, counts, "DeveloperReview", {
      item_type: String(row["source"]) === "refinement" ? "refinement" : "memory",
      id: String(row["id"]),
      title: summarize(String(row["feedback"]), 120),
      detail: summarize(String(row["context"]), 180),
      agent_id: String(row["agent_id"]),
      status: String(row["state"]),
      raw_ids: [String(row["id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: row["updated_at"] ?? null
    }, limit);
  }
  const profile = Object.fromEntries(repoProfileRows(db3, params).map((row) => [String(row["metric"]), Number(row["count"] ?? 0)]));
  const activeMemories = Number(profile["active_memories"] ?? 0);
  const taskCount = Number(profile["tasks"] ?? 0);
  const openRefinements = Number(profile["open_refinements"] ?? 0);
  const openSignalCount = Number(profile["open_signals"] ?? 0);
  const missingFileRefs = Number(profile["missing_file_refs"] ?? 0);
  const projectionWarnings2 = [
    missingFileRefs > 0 ? "missing_file_refs" : null,
    activeMemories > 200 ? "active_memories_over_200" : null,
    taskCount > 500 ? "task_rows_over_500" : null,
    openRefinements > 40 ? "open_refinements_over_40" : null
  ].filter((warning) => Boolean(warning));
  pushLimited(columns, counts, "ProjectionHealth", {
    item_type: "projection",
    id: "projection-health",
    title: projectionWarnings2.length > 0 ? "Projection/bloat review suggested" : "Projection health nominal",
    detail: projectionWarnings2.join(", ") || "No profile threshold warnings.",
    status: projectionWarnings2.length > 0 ? "review" : "ok",
    count: projectionWarnings2.length,
    raw_ids: [],
    files: [],
    active_memories: activeMemories,
    missing_file_refs: missingFileRefs,
    tasks: taskCount,
    open_refinements: openRefinements,
    open_signals: openSignalCount,
    created_at: utcNow2()
  }, limit);
  return Object.entries(columns).flatMap(([column, rows]) => {
    const total = counts[column] ?? rows.length;
    return rows.map((row) => ({
      ...row,
      column_total: total,
      omitted_count: Math.max(0, total - rows.length)
    }));
  });
}
function rowsForView(db3, view, params) {
  switch (view) {
    case "repo-profile":
      return repoProfileRows(db3, params);
    case "memories":
      return memoryRows(db3, params);
    case "gotchas":
      return memoryRows(db3, params, { gotchas: true });
    case "lessons":
      return memoryRows(db3, params, { lessons: true });
    case "plans":
      return planRows(db3, params);
    case "tasks":
      return taskRows(db3, params);
    case "runs":
      return runRows(db3, params);
    case "locks":
      return lockRows(db3, params);
    case "agents":
      return agentRows(db3, params);
    case "signals":
      return signalRows(db3, params);
    case "refinements":
      return refinementRows(db3, params);
    case "files":
      return fileRows2(db3, params);
    case "activity":
      return activityRows(db3, params);
    case "workboard":
      return workboardRows(db3, params);
    case "developer-review":
      return developerReviewRows(db3, params);
    case "all":
      return [];
  }
}
function queryAwareness(db3, params = {}) {
  const view = normalizeView(params.view);
  const scope = scopeFromParams(params);
  const generatedAt = utcNow2();
  const filters = {
    query: params.query ?? null,
    limit: limitOf(params.limit),
    agent_id: params.agentId ?? params.agent_id ?? null,
    state: stringList(params.state),
    label: stringList(params.label),
    file: params.file ?? null,
    since: params.since ?? null
  };
  if (view === "all") {
    const sections = {};
    for (const section of AWARENESS_QUERY_VIEWS) {
      if (section === "all") continue;
      const rows3 = rowsForView(db3, section, params);
      sections[section] = { count: rows3.length, rows: rows3 };
    }
    const rows2 = Object.entries(sections).map(([name, section]) => ({ section: name, count: section.count }));
    return {
      ok: true,
      view,
      generated_at: generatedAt,
      workspace_path: scope.workspacePath,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      count: rows2.length,
      rows: rows2,
      sections,
      filters
    };
  }
  const rows = rowsForView(db3, view, params);
  return {
    ok: true,
    view,
    generated_at: generatedAt,
    workspace_path: scope.workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    count: rows.length,
    rows,
    filters
  };
}
function developerReviewDoc(db3, params = {}) {
  const rows = developerReviewRows(db3, params);
  const open = rows.filter((row) => String(row["state"]) !== "done").length;
  return {
    rows,
    open,
    resolved: rows.length - open,
    markdown: renderDeveloperReviewDoc(rows, PROJECTION_MARKDOWN_BUDGETS["DEVELOPER_REVIEW.md"].max_lines)
  };
}
function formatAwarenessQueryResult(result, format) {
  const normalized = normalizeFormat(format);
  if (normalized === "json") return JSON.stringify(result, null, 2);
  if (normalized === "csv") return toCsv(result.rows);
  if (normalized === "table") return toTable(result.rows);
  if (normalized === "html") return renderAwarenessHtml(result);
  return toMarkdown(result);
}
function renderAwarenessHtml(result) {
  const title = `Octocode Awareness: ${result.view}`;
  const sectionNames = result.sections ? Object.keys(result.sections) : [result.view];
  const sections = result.sections ? Object.entries(result.sections).map(([name, section]) => renderHtmlSection(name, section.rows)).join("\n") : renderHtmlSection(result.view, result.rows);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(title)}</title>
  <style>
    :root { color-scheme: light dark; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: Canvas; color: CanvasText; }
    header { padding: 24px 28px 12px; border-bottom: 1px solid color-mix(in srgb, CanvasText 16%, transparent); }
    .controls { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 16px; align-items: center; }
    .controls input, .controls select, .controls label { font: inherit; font-size: 13px; }
    .controls input, .controls select { min-height: 34px; padding: 6px 8px; border: 1px solid color-mix(in srgb, CanvasText 24%, transparent); border-radius: 6px; background: Canvas; color: CanvasText; }
    .controls input { min-width: min(420px, 100%); flex: 1 1 260px; }
    .controls label { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; color: color-mix(in srgb, CanvasText 76%, transparent); }
    main { padding: 20px 28px 40px; display: grid; gap: 28px; }
    h1 { margin: 0 0 8px; font-size: 24px; letter-spacing: 0; }
    h2 { margin: 0 0 12px; font-size: 18px; letter-spacing: 0; }
    .meta { color: color-mix(in srgb, CanvasText 62%, transparent); font-size: 13px; }
    table { width: 100%; border-collapse: collapse; font-size: 13px; }
    th, td { text-align: left; vertical-align: top; padding: 8px 10px; border-bottom: 1px solid color-mix(in srgb, CanvasText 12%, transparent); }
    th { font-weight: 650; white-space: nowrap; }
    th button { all: unset; cursor: pointer; }
    th button::after { content: " v"; color: color-mix(in srgb, CanvasText 46%, transparent); }
    td { max-width: 460px; overflow-wrap: anywhere; }
    section { overflow-x: auto; }
    section[hidden], tr[hidden] { display: none; }
    code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
  </style>
</head>
<body>
  <header>
    <h1>${escapeHtml(title)}</h1>
    <div class="meta">Generated ${escapeHtml(result.generated_at)} for <code>${escapeHtml(result.workspace_path ?? "global")}</code></div>
    <div class="controls">
      <input id="global-filter" type="search" placeholder="Filter rows" autocomplete="off">
      <select id="section-filter" aria-label="Section">
        <option value="">All sections</option>
        ${sectionNames.map((name) => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join("")}
      </select>
      <label><input id="missing-filter" type="checkbox"> Missing files</label>
    </div>
  </header>
  <main>
    ${sections}
  </main>
  <script>
    const filterInput = document.querySelector('#global-filter');
    const sectionFilter = document.querySelector('#section-filter');
    const missingFilter = document.querySelector('#missing-filter');
    function applyFilters() {
      const text = filterInput.value.trim().toLowerCase();
      const wantedSection = sectionFilter.value;
      const onlyMissing = missingFilter.checked;
      for (const section of document.querySelectorAll('section[data-section]')) {
        const sectionMatches = !wantedSection || section.dataset.section === wantedSection;
        let visible = 0;
        for (const row of section.querySelectorAll('tbody tr')) {
          const rowMatches = (!text || row.textContent.toLowerCase().includes(text)) && (!onlyMissing || row.dataset.missing === 'true');
          row.hidden = !(sectionMatches && rowMatches);
          if (!row.hidden) visible++;
        }
        section.hidden = !sectionMatches || visible === 0;
      }
    }
    for (const control of [filterInput, sectionFilter, missingFilter]) control.addEventListener('input', applyFilters);
    for (const button of document.querySelectorAll('th button[data-key]')) {
      button.addEventListener('click', () => {
        const table = button.closest('table');
        const tbody = table.querySelector('tbody');
        const index = Array.from(button.closest('tr').children).indexOf(button.closest('th'));
        const direction = button.dataset.direction === 'asc' ? 'desc' : 'asc';
        button.dataset.direction = direction;
        const rows = Array.from(tbody.querySelectorAll('tr'));
        rows.sort((a, b) => {
          const av = a.children[index]?.textContent.trim() ?? '';
          const bv = b.children[index]?.textContent.trim() ?? '';
          const an = Number(av);
          const bn = Number(bv);
          const cmp = Number.isFinite(an) && Number.isFinite(bn) ? an - bn : av.localeCompare(bv);
          return direction === 'asc' ? cmp : -cmp;
        });
        for (const row of rows) tbody.appendChild(row);
        applyFilters();
      });
    }
    applyFilters();
  </script>
</body>
</html>
`;
}
function resolveWorkspaceOutputPath(output, workspacePath, defaultPath) {
  const target = output?.trim() || defaultPath;
  return isAbsolute7(target) ? resolve12(target) : resolve12(workspacePath, target);
}
function injectRepoContext(db3, params = {}) {
  const scope = scopeFromParams(params);
  const workspacePath = scope.workspacePath ?? process.cwd();
  const rawOutDir = params.outDir ?? params.out_dir;
  const outDir = resolveWorkspaceOutputPath(rawOutDir, workspacePath, join6(workspacePath, ".octocode"));
  const mode = normalizeMode(params.mode);
  const includeView = params.includeView ?? params.include_view ?? true;
  const check = params.check ?? true;
  const queryParams = {
    ...params,
    workspacePath,
    limit: limitOf(params.limit, 50, 500),
    view: "all"
  };
  SCOPE_CACHE.set(queryParams, scope);
  const all = queryAwareness(db3, queryParams);
  const filesWritten = [];
  const writtenContent = {};
  const warnings = [];
  function write(relPath, content) {
    const full = join6(outDir, relPath);
    mkdirSync4(join6(full, ".."), { recursive: true });
    writeFileSync3(full, content, "utf8");
    writtenContent[relPath] = content;
    filesWritten.push(full);
  }
  const sections = all.sections ?? {};
  const counts = Object.fromEntries(Object.entries(sections).map(([name, section]) => [name, section.count]));
  write("AGENTS.md", renderRepoAgentsMd(all));
  write("MEMORY.md", renderRowsDoc("Memory", sections["memories"]?.rows ?? [], "Active awareness memories for this repo.", PROJECTION_MARKDOWN_BUDGETS["MEMORY.md"].max_lines));
  write("GOTCHAS.md", renderRowsDoc("Gotchas", sections["gotchas"]?.rows ?? [], "Failures, traps, and sharp edges agents should check before editing.", PROJECTION_MARKDOWN_BUDGETS["GOTCHAS.md"].max_lines));
  write("LEARN.md", renderRowsDoc("Learning And Opportunities", sections["lessons"]?.rows ?? [], "Decisions, architecture notes, workflows, and improvement ideas.", PROJECTION_MARKDOWN_BUDGETS["LEARN.md"].max_lines));
  write("BOOKMARKS.md", renderBookmarksDoc(sections["memories"]?.rows ?? []));
  write("DEVELOPER_REVIEW.md", renderDeveloperReviewDoc(sections["developer-review"]?.rows ?? [], PROJECTION_MARKDOWN_BUDGETS["DEVELOPER_REVIEW.md"].max_lines));
  for (const view of CSV_VIEWS) {
    write(join6("awareness", "csv", `${view}.csv`), toCsv(sections[view]?.rows ?? []));
  }
  if (includeView) {
    write(join6("awareness", "index.html"), renderAwarenessHtml(all));
  }
  write(join6("references", "repo-map.md"), renderReferenceDoc("Repo Map", [
    "Generated overview of awareness-tracked files and activity.",
    "Use `.octocode/awareness/csv/files.csv` when filtering or sorting by file path.",
    "Use the live command `octocode-awareness query files --workspace <repo>` when freshness matters."
  ], sections["files"]?.rows ?? []));
  write(join6("references", "commands.md"), renderReferenceDoc("Awareness Commands", [
    "`octocode-awareness query <view>` reads the SQLite store for agents and scripts.",
    "`octocode-awareness query all --format html --out .octocode/awareness/index.html` writes a static human browser view; use `npx @octocodeai/octocode-awareness` only when no local CLI exists.",
    "`octocode-awareness repo inject --out .octocode` regenerates these Markdown, CSV, and HTML projections."
  ]));
  write(join6("references", "testing.md"), renderReferenceDoc("Testing And Verification", [
    "Treat generated memories as leads. Verify current files and command output before acting.",
    "Release locks with `verify mark` or `lock release --verified` after declared tests actually run.",
    "Record new durable failures with `reflect record --failure-signature` or `memory record --label GOTCHA`."
  ]));
  write(join6("references", "architecture.md"), renderReferenceDoc("Architecture Notes", [
    "The SQLite awareness DB is canonical. Files under `.octocode/` are generated projections.",
    "Keep workspace AGENTS.md concise and point agents here for repo-specific memory indexes.",
    "Do not edit generated CSV/Markdown snapshots by hand; regenerate after important memory changes."
  ]));
  if (check) {
    const ignored = gitCheckIgnored(workspacePath, outDir);
    if (ignored.ignored) {
      warnings.push(`generated path is gitignored: ${relative4(workspacePath, outDir) || outDir}; remove the ignore intentionally if this repo should share .octocode`);
    }
    if (mode === "share" && ignored.ignored) {
      warnings.push("mode=share requested, but git currently ignores the generated .octocode path");
    }
  }
  const projectionBudgets = Object.fromEntries(Object.entries(PROJECTION_MARKDOWN_BUDGETS).map(([relPath, budget]) => {
    const actualLines = lineCount(writtenContent[relPath] ?? "");
    return [relPath, {
      ...budget,
      actual_lines: actualLines,
      within_budget: actualLines <= budget.max_lines
    }];
  }));
  for (const [relPath, budget] of Object.entries(projectionBudgets)) {
    if (!budget.within_budget) warnings.push(`projection budget exceeded: ${relPath} has ${budget.actual_lines}/${budget.max_lines} lines`);
  }
  const generatedAt = utcNow2();
  const manifestRelPath = join6("awareness", "manifest.json");
  const manifestFiles = [
    ...filesWritten.map((file) => relative4(workspacePath, file)),
    relative4(workspacePath, join6(outDir, manifestRelPath))
  ];
  const manifest = {
    schema_version: 1,
    generated_at: generatedAt,
    generator: "@octocodeai/octocode-awareness repo inject",
    mode,
    workspace_path: workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    source: {
      canonical: "~/.octocode/memory/awareness.sqlite3",
      projection: ".octocode"
    },
    policy: {
      gitignore_modified: false,
      share_decision: "user-owned"
    },
    counts,
    budgets: {
      markdown: projectionBudgets,
      workboard: WORKBOARD_BUDGET,
      attend_compact: ATTEND_COMPACT_BUDGET
    },
    files: manifestFiles,
    warnings
  };
  write(manifestRelPath, JSON.stringify(manifest, null, 2) + "\n");
  return {
    ok: true,
    generated_at: generatedAt,
    workspace_path: workspacePath,
    out_dir: outDir,
    mode,
    count: filesWritten.length,
    files: filesWritten,
    warnings,
    manifest
  };
}
function renderRepoAgentsMd(all) {
  const sections = all.sections ?? {};
  const profile = sections["repo-profile"]?.rows ?? [];
  const counts = Object.fromEntries(profile.map((row) => [String(row["metric"]), row["count"] ?? 0]));
  const gotchas = (sections["gotchas"]?.rows ?? []).slice(0, 5);
  const lessons = (sections["lessons"]?.rows ?? []).slice(0, 5);
  const fileWorkRows = (sections["workboard"]?.rows ?? []).filter((row) => row["column"] === "FilesUnderWork");
  const fileWork = fileWorkRows.slice(0, 3);
  const locks = (sections["locks"]?.rows ?? []).slice(0, 3);
  const lockTotal = (sections["locks"]?.rows ?? []).length;
  const projectionWarnings2 = [
    Number(counts["missing_file_refs"] ?? 0) > 0 ? `Missing/stale file refs (${counts["missing_file_refs"]}) \u2014 use \`query files --format table\` before trusting bookmarks.` : null,
    Number(counts["active_memories"] ?? 0) > 200 ? `Active memories high (${counts["active_memories"]}) \u2014 prefer recall/CSV over full Markdown.` : null,
    Number(counts["tasks"] ?? 0) > 500 ? `Task history high (${counts["tasks"]}) \u2014 use \`query workboard\`.` : null,
    Number(counts["open_refinements"] ?? 0) > 40 ? `Open refinements high (${counts["open_refinements"]}) \u2014 filter CSV before promoting.` : null
  ].filter((item) => Boolean(item));
  const lines = [
    "# Octocode Awareness Map",
    "",
    "<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->",
    "",
    "Digested awareness entrypoint. Root `AGENTS.md` should point here. SQLite is canonical; this folder is a capped wiki.",
    "",
    "## How To Use",
    "",
    "- Live: `octocode-awareness attend|work list|query|memory recall|workspace status --workspace <repo>`.",
    "- Wiki leads below are projections, not proof. After inject, append a root `AGENTS.md` \u2192 `.octocode/AGENTS.md` pointer if missing.",
    "",
    "## Snapshot",
    "",
    `- Memories ${counts["active_memories"] ?? 0} \xB7 Gotchas ${counts["gotchas"] ?? 0} \xB7 Lessons ${counts["lessons"] ?? 0} \xB7 MissingFiles ${counts["missing_file_refs"] ?? 0} \xB7 Locks ${counts["active_locks"] ?? 0} \xB7 Refinements ${counts["open_refinements"] ?? 0} \xB7 Signals ${counts["open_signals"] ?? 0} \xB7 DevReview ${counts["developer_review"] ?? 0}`,
    "",
    "## Retro Files Map",
    "",
    "Generated retrospective projections in this folder (SQLite is canonical; regenerate, don't hand-edit). File work, locks, signals, and tasks live only in the DB \u2014 use live `attend`, `work`, or `query`.",
    "",
    "- Gotchas \u2192 `.octocode/GOTCHAS.md` \xB7 live `query gotchas` / `memory recall`",
    "- Lessons \u2192 `.octocode/LEARN.md` \xB7 live `query lessons`",
    "- Memory index \u2192 `.octocode/MEMORY.md` \xB7 live `memory recall --smart`",
    "- Bookmarks \u2192 `.octocode/BOOKMARKS.md` \xB7 Files/missing refs \u2192 live `query files` or `awareness/csv/files.csv` \xB7 Workboard \u2192 live `query workboard`",
    "- Developer review \u2192 `.octocode/DEVELOPER_REVIEW.md` \xB7 live `reflect developer-review` \u2014 agent feedback on the instructions themselves",
    "",
    "## Read Before Editing",
    "",
    "- Run `attend`, then inspect `FilesUnderWork` before editing. Record ordinary work with `work start`; overlap is allowed and visible.",
    "- Exclusive locks are reserved for sensitive files. An active exclusive lock blocks conflicting work.",
    "- Read GOTCHAS + LEARN; run `query files --format table` or filter `awareness/csv/files.csv` for affected and missing paths.",
    "- Prefer live `attend` / `work list` / `query` when freshness matters; `repo inject` after important memories.",
    "",
    "## Projection Health",
    "",
    "- Canonical DB: `~/.octocode/memory/awareness.sqlite3`. Manifest: `.octocode/awareness/manifest.json`.",
    ...projectionWarnings2.map((warning) => `- ${warning}`),
    ""
  ];
  if (fileWork.length > 0) {
    lines.push("## Files Under Work", "");
    for (const row of fileWork) {
      const locked = row["locked"] ? " \xB7 exclusive lock" : "";
      lines.push(`- ${row["path"]} \u2014 ${row["peer_count"]} worker(s): ${row["agents"]} \xB7 ${summarize(String(row["reasons"] ?? "reason not recorded"), 100)}${locked}`);
    }
    if (fileWorkRows.length > fileWork.length) lines.push(`- \u2026and ${fileWorkRows.length - fileWork.length} more (live: \`work list\` or \`query workboard\`)`);
    lines.push("");
  }
  if (locks.length > 0) {
    lines.push("## Active Exclusive Locks", "");
    for (const lock of locks) lines.push(`- ${lock["file_path"]} \u2014 ${lock["agent_id"]}`);
    if (lockTotal > locks.length) lines.push(`- \u2026and ${lockTotal - locks.length} more (live: \`query locks\`)`);
    lines.push("");
  }
  if (gotchas.length > 0) {
    lines.push("## Top Gotchas", "");
    for (const row of gotchas) lines.push(`- [${row["importance"]}] ${summarize(String(row["observation"]), 140)}`);
    lines.push("");
  }
  if (lessons.length > 0) {
    lines.push("## Top Lessons", "");
    for (const row of lessons) lines.push(`- [${row["label"]}:${row["importance"]}] ${summarize(String(row["observation"]), 140)}`);
    lines.push("");
  }
  lines.push("## References", "");
  lines.push("- `.octocode/MEMORY.md` \xB7 `.octocode/GOTCHAS.md` \xB7 `.octocode/LEARN.md` \xB7 `.octocode/BOOKMARKS.md` \xB7 `.octocode/DEVELOPER_REVIEW.md`");
  lines.push("- `.octocode/awareness/manifest.json` \xB7 `.octocode/references/`");
  lines.push("");
  return lines.join("\n");
}
function renderRowsDoc(title, rows, description, maxLines) {
  const ranked = [...rows].sort((a, b) => {
    const imp = Number(b["importance"] ?? 0) - Number(a["importance"] ?? 0);
    if (imp !== 0) return imp;
    return String(a["memory_id"] ?? a["plan_id"] ?? a["task_id"] ?? a["run_id"] ?? "").localeCompare(String(b["memory_id"] ?? b["plan_id"] ?? b["task_id"] ?? b["run_id"] ?? ""));
  });
  const lines = [
    `# ${title}`,
    "",
    "<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->",
    "",
    description,
    "",
    `Count: ${rows.length}`,
    ""
  ];
  let omitted = 0;
  for (const row of ranked) {
    const id = String(row["memory_id"] ?? row["refinement_id"] ?? row["plan_id"] ?? row["task_id"] ?? row["run_id"] ?? row["signal_id"] ?? row["file_path"] ?? "item");
    const label = row["label"] ? `[${row["label"]}:${row["importance"] ?? ""}] ` : "";
    const titleText = row["task_context"] ?? row["subject"] ?? row["remember"] ?? row["name"] ?? row["title"] ?? row["rationale"] ?? row["file_path"] ?? id;
    const block = [`## ${label}${summarize(String(titleText), 100)}`];
    if (row["observation"]) block.push("", summarize(String(row["observation"]), 500));
    if (row["failure_signature"]) block.push("", `Failure signature: \`${row["failure_signature"]}\``);
    const refs = Array.isArray(row["references"]) ? row["references"] : [];
    if (refs.length > 0) block.push("", `Refs: ${refs.join(", ")}`);
    const missingRefs = Array.isArray(row["missing_references"]) ? row["missing_references"] : [];
    if (missingRefs.length > 0) block.push("", `Missing refs: ${missingRefs.join(", ")}`);
    block.push("", `Source id: \`${id}\``, "");
    const needsOmittedLine = omitted === 0 && rows.length > 0;
    const reserve = maxLines ? needsOmittedLine ? 3 : 1 : 0;
    if (maxLines && lines.length + block.length + reserve > maxLines) {
      omitted++;
      continue;
    }
    lines.push(...block);
  }
  if (omitted > 0) {
    const note = `Omitted by projection cap: ${omitted}. Use CSV/HTML/query views for full rows.`;
    if (!maxLines || lines.length + 2 <= maxLines) lines.push(note, "");
  }
  return lines.join("\n");
}
function bookmarkKind(reference) {
  const lower = reference.toLowerCase();
  if (/^(github|gh|repo):/.test(lower) || lower.includes("github.com/")) return "Repos";
  if (/^https?:\/\//.test(lower)) return "URLs";
  if (/^(file|path):/.test(lower) || lower.startsWith("/") || lower.startsWith("./")) return "Files";
  if (/^(doc|docs|paper|book|resource|skill):/.test(lower)) return "Docs";
  if (/^[a-z][a-z0-9+.-]*:/.test(lower)) return "URIs";
  return "Other";
}
function renderBookmarksDoc(memoryRows2) {
  const byRef = /* @__PURE__ */ new Map();
  for (const row of memoryRows2) {
    const refs = Array.isArray(row["references"]) ? row["references"] : [];
    const missingRefs = new Set(Array.isArray(row["missing_references"]) ? row["missing_references"] : []);
    const sourceId = String(row["memory_id"] ?? "memory");
    const label = `${row["label"] ?? "MEMORY"}:${row["importance"] ?? ""}`.replace(/:$/, "");
    const title = summarize(String(row["task_context"] ?? row["observation"] ?? sourceId), 90);
    for (const rawRef of refs) {
      const ref = rawRef.trim();
      if (!ref) continue;
      const entry2 = byRef.get(ref) ?? { kind: bookmarkKind(ref), sourceIds: [], labels: [], titles: [], missing: false };
      if (!entry2.sourceIds.includes(sourceId)) entry2.sourceIds.push(sourceId);
      if (!entry2.labels.includes(label)) entry2.labels.push(label);
      if (!entry2.titles.includes(title)) entry2.titles.push(title);
      entry2.missing = entry2.missing || missingRefs.has(ref);
      byRef.set(ref, entry2);
    }
  }
  const entries = [...byRef.entries()].sort((a, b) => a[1].kind.localeCompare(b[1].kind) || b[1].sourceIds.length - a[1].sourceIds.length || a[0].localeCompare(b[0])).slice(0, 80);
  const omitted = Math.max(0, byRef.size - entries.length);
  const lines = [
    "# Bookmarks",
    "",
    "<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->",
    "",
    "Learnable resource leads from awareness memory references: URLs, repos, file paths, docs, papers, skills, and other URIs.",
    "SQLite remains canonical; verify each bookmark against current source or primary material before relying on it.",
    "",
    `Count: ${byRef.size}`,
    omitted > 0 ? `Omitted by cap: ${omitted}` : null,
    ""
  ].filter((line) => line !== null);
  let currentKind = "";
  for (const [ref, entry2] of entries) {
    if (entry2.kind !== currentKind) {
      currentKind = entry2.kind;
      lines.push(`## ${currentKind}`, "");
    }
    const sourceText = entry2.sourceIds.slice(0, 3).join(", ");
    const titleText = entry2.titles.slice(0, 2).join(" | ");
    const labelText = entry2.labels.slice(0, 3).join(", ");
    const health = entry2.missing ? " [missing file]" : "";
    lines.push(`- \`${ref}\`${health} - ${labelText}; source: ${sourceText}; ${titleText}`);
  }
  lines.push("");
  return lines.join("\n");
}
function renderDeveloperReviewDoc(rows, maxLines) {
  const open = rows.filter((row) => String(row["state"]) !== "done");
  const resolved = rows.filter((row) => String(row["state"]) === "done");
  const lines = [
    "# Developer Review",
    "",
    "<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->",
    "",
    "Feedback from agents to the human who authors this repo's agent instructions",
    "(root `AGENTS.md`, `.octocode/AGENTS.md`, SKILL.md, system prompt, task briefs).",
    "Each item is where the *instructions* \u2014 not the code, not the harness \u2014 were ambiguous,",
    "wrong, over-constraining, or missing context. Recorded via `reflect record --fix-instructions`.",
    "",
    `Open: ${open.length} \xB7 Resolved: ${resolved.length}`,
    ""
  ];
  if (rows.length === 0) {
    lines.push(
      "No instruction feedback yet.",
      "",
      "Agents: when your instructions cost you time or a wrong turn, record it:",
      '`octocode-awareness reflect record --outcome partial --task "<what you did>" \\',
      '  --fix-instructions "<what the instructions should have said>"`',
      ""
    );
    return lines.join("\n");
  }
  function renderSection(title, sectionRows) {
    if (sectionRows.length === 0) return;
    lines.push(`## ${title} (${sectionRows.length})`, "");
    for (const row of sectionRows) {
      const source = String(row["source"]);
      const state = String(row["state"]);
      const agent = String(row["agent_id"] ?? "agent");
      const files = rowFiles(row);
      const block = [`- ${summarize(String(row["feedback"]), 240)}`];
      const meta = [`from ${agent}`, `via ${source}${state && state !== "recorded" ? `:${state}` : ""}`, `id \`${row["id"]}\``];
      if (files.length > 0) meta.push(`files: ${files.slice(0, 3).join(", ")}${files.length > 3 ? ` +${files.length - 3}` : ""}`);
      block.push(`  - ${meta.join(" \xB7 ")}`);
      if (maxLines && lines.length + block.length + 1 > maxLines) {
        lines.push(`- \u2026more omitted by projection cap. Use \`query developer-review\` or \`reflect developer-review\`.`, "");
        return;
      }
      lines.push(...block);
    }
    lines.push("");
  }
  renderSection("Open", open);
  renderSection("Resolved", resolved);
  return lines.join("\n");
}
function renderReferenceDoc(title, bullets, rows = []) {
  const lines = [
    `# ${title}`,
    "",
    "<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->",
    "",
    ...bullets.map((item) => `- ${item}`),
    ""
  ];
  if (rows.length > 0) {
    lines.push("## Top Rows", "");
    for (const row of rows.slice(0, 25)) {
      const primary = row["file_path"] ?? row["title"] ?? row["metric"] ?? JSON.stringify(row);
      lines.push(`- ${primary}`);
    }
    lines.push("");
  }
  return lines.join("\n");
}
function renderHtmlSection(name, rows) {
  return `<section data-section="${escapeHtml(name)}">
  <h2>${escapeHtml(name)} (${rows.length})</h2>
  ${rows.length === 0 ? '<p class="meta">No rows.</p>' : `<table>${renderHtmlTable(rows)}</table>`}
</section>`;
}
function renderHtmlTable(rows) {
  const keys = keysForRows(rows).slice(0, 12);
  const header = `<thead><tr>${keys.map((key) => `<th><button type="button" data-key="${escapeHtml(key)}">${escapeHtml(key)}</button></th>`).join("")}</tr></thead>`;
  const body = rows.map((row) => {
    const missing = row["missing_file"] === true || Number(row["missing_reference_count"] ?? 0) > 0 || Array.isArray(row["missing_references"]) && row["missing_references"].length > 0;
    return `<tr data-missing="${missing ? "true" : "false"}">${keys.map((key) => `<td>${escapeHtml(cellToString(row[key]))}</td>`).join("")}</tr>`;
  }).join("\n");
  return `${header}<tbody>${body}</tbody>`;
}
function keysForRows(rows) {
  const keys = [];
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      if (!keys.includes(key)) keys.push(key);
    }
  }
  return keys;
}
function toCsv(rows) {
  if (rows.length === 0) return "";
  const keys = keysForRows(rows);
  return [
    keys.map(csvCell).join(","),
    ...rows.map((row) => keys.map((key) => csvCell(cellToString(row[key]))).join(","))
  ].join("\n") + "\n";
}
function toTable(rows) {
  if (rows.length === 0) return "No rows.\n";
  const keys = keysForRows(rows).slice(0, 10);
  const widths = keys.map((key) => Math.min(40, Math.max(key.length, ...rows.map((row) => cellToString(row[key]).length))));
  const line = keys.map((key, i) => key.padEnd(widths[i] ?? key.length)).join("  ");
  const sep3 = widths.map((w) => "-".repeat(w)).join("  ");
  const body = rows.map((row) => keys.map((key, i) => truncate(cellToString(row[key]), widths[i] ?? 40).padEnd(widths[i] ?? 40)).join("  "));
  return [line, sep3, ...body].join("\n") + "\n";
}
function toMarkdown(result) {
  const lines = [
    `# Awareness ${result.view}`,
    "",
    `Generated: ${result.generated_at}`,
    `Workspace: ${result.workspace_path ?? "global"}`,
    ""
  ];
  if (result.sections) {
    for (const [name, section] of Object.entries(result.sections)) {
      lines.push(`## ${name} (${section.count})`, "", markdownRows(section.rows), "");
    }
  } else {
    lines.push(markdownRows(result.rows), "");
  }
  return lines.join("\n");
}
function markdownRows(rows) {
  if (rows.length === 0) return "_No rows._";
  return rows.map((row) => {
    const id = row["memory_id"] ?? row["plan_id"] ?? row["task_id"] ?? row["run_id"] ?? row["signal_id"] ?? row["refinement_id"] ?? row["file_path"] ?? row["metric"] ?? "row";
    const label = row["label"] ? `[${cellToString(row["label"])}:${cellToString(row["importance"])}] ` : "";
    const title = row["task_context"] ?? row["subject"] ?? row["remember"] ?? row["name"] ?? row["title"] ?? row["rationale"] ?? row["metric"] ?? "";
    const text = row["observation"] ?? row["objective"] ?? row["reasoning"] ?? row["count"] ?? "";
    const extras = [];
    if (row["failure_signature"]) extras.push(`failure=${cellToString(row["failure_signature"])}`);
    if (Array.isArray(row["references"]) && row["references"].length > 0) extras.push(`refs=${row["references"].join(", ")}`);
    if (Array.isArray(row["missing_references"]) && row["missing_references"].length > 0) extras.push(`missing=${row["missing_references"].join(", ")}`);
    const suffix = extras.length > 0 ? ` (${extras.join("; ")})` : "";
    return `- \`${cellToString(id)}\` ${label}${summarize(cellToString(title), 100)} - ${summarize(cellToString(text), 220)}${suffix}`;
  }).join("\n");
}
function csvCell(value) {
  const s = String(value ?? "");
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}
function cellToString(value) {
  if (Array.isArray(value)) return value.join("; ");
  if (value == null) return "";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}
function truncate(value, width) {
  if (value.length <= width) return value;
  if (width <= 3) return ".".repeat(width);
  return value.slice(0, width - 3) + "...";
}
function summarize(value, max) {
  const flat = value.replace(/\s+/g, " ").trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + "...";
}
function lineCount(value) {
  if (!value) return 0;
  return value.split(/\r\n|\r|\n/).length;
}
function escapeHtml(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}
function gitCheckIgnored(cwd, path2) {
  const candidate = isAbsolute7(path2) ? relative4(cwd, path2) : path2;
  const result = spawnSync4("git", ["check-ignore", "-q", candidate], { cwd, encoding: "utf8" });
  return { ignored: result.status === 0 };
}

// src/attend.ts
var TEAM_NORMS = [
  "evidence-first",
  "bounded",
  "cooperative",
  "non-destructive",
  "verify-before-policy"
];
var ORGAN_REFERENCE = [
  {
    organ: "senses",
    role: "read live state",
    commands: ["workspace status", "query repo-profile"],
    guardrail: "Live DB beats stale projections."
  },
  {
    organ: "attention",
    role: "select a small packet",
    commands: ["attend", "query workboard", "memory recall"],
    guardrail: "Show gaps, not dumps."
  },
  {
    organ: "memory",
    role: "durable lessons",
    commands: ["memory record", "memory recall", "reflect record"],
    guardrail: "Memories are leads until verified."
  },
  {
    organ: "immune_pruning",
    role: "tag weak/stale evidence",
    commands: ["memory forget --dry-run", "maintenance digest --dry-run", "query workboard"],
    guardrail: "Report before deleting."
  },
  {
    organ: "corpus_bridge",
    role: "coordinate agents",
    commands: ["plan list", "task ready", "task claim", "work start", "work list", "signal publish", "lock acquire", "verify audit"],
    guardrail: "SQLite is canonical."
  },
  {
    organ: "drive",
    role: "goal/gaps/resources",
    commands: ["attend --explain-organ", "query workboard"],
    guardrail: "Collective state, not persona."
  }
];
function limitOf2(value, fallback = 10, max = 50) {
  if (value == null || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(1, Math.floor(value)));
}
function stringList2(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value == null || value === "") return [];
  return [String(value)];
}
function summarize2(value, max) {
  const flat = value.replace(/\s+/g, " ").trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + "...";
}
function profileMap(rows) {
  return Object.fromEntries(rows.map((row) => [String(row["metric"]), Number(row["count"] ?? 0)]));
}
function groupWorkboard(rows) {
  const groups = {};
  for (const row of rows) {
    const column = String(row["column"] ?? "Other");
    const list = groups[column] ?? [];
    list.push(row);
    groups[column] = list;
  }
  return groups;
}
function compactRow(row) {
  if (row["item_type"] === "file") {
    return Object.fromEntries([
      "path",
      "peer_count",
      "omitted_peer_count",
      "locked",
      "lock_agent_id"
    ].flatMap((key) => row[key] == null ? [] : [[key, row[key]]]));
  }
  const next = {};
  for (const key of ["item_type", "id", "plan_id", "status", "agent_id", "priority"]) {
    const value = row[key];
    if (value != null) next[key] = value;
  }
  if (typeof row["title"] === "string") next["title"] = summarize2(row["title"], 60);
  return next;
}
function compactWorkboard(grouped, limit) {
  const actionable = ["Inbox", "Ready", "Claimed", "Verify", "FilesUnderWork"];
  return Object.fromEntries(actionable.flatMap((column) => {
    const rows = grouped[column] ?? [];
    return rows.length === 0 ? [] : [[column, rows.slice(0, limit).map(compactRow)]];
  }));
}
function uniqueStrings(values) {
  return [...new Set(values.filter(Boolean))];
}
function lineCount2(path2) {
  if (!existsSync5(path2)) return null;
  try {
    return readFileSync3(path2, "utf8").split(/\r?\n/).length;
  } catch {
    return null;
  }
}
function projectionStats(workspacePath) {
  return ["AGENTS.md", "MEMORY.md", "GOTCHAS.md", "LEARN.md", "BOOKMARKS.md", join7("awareness", "manifest.json")].map((file) => {
    const path2 = join7(workspacePath, ".octocode", file);
    let mtimeMs = null;
    try {
      mtimeMs = existsSync5(path2) ? statSync2(path2).mtimeMs : null;
    } catch {
    }
    return { file: `.octocode/${file.replace(/\\/g, "/")}`, lines: lineCount2(path2), mtime_ms: mtimeMs };
  });
}
function manifestWarnings(workspacePath, stats) {
  const manifestPath = join7(workspacePath, ".octocode", "awareness", "manifest.json");
  if (!existsSync5(manifestPath)) return [".octocode/awareness/manifest.json missing; run repo inject when projection context is needed"];
  try {
    const manifest = JSON.parse(readFileSync3(manifestPath, "utf8"));
    const warnings = [];
    const files = manifest.files ?? [];
    if (!files.some((file) => file.endsWith("/BOOKMARKS.md") || file.endsWith("\\BOOKMARKS.md") || file === "BOOKMARKS.md")) {
      warnings.push("manifest missing BOOKMARKS.md; regenerate repo projection");
    }
    const markdownBudgets = manifest.budgets?.markdown ?? {};
    for (const [file, budget] of Object.entries(markdownBudgets)) {
      if (budget.within_budget === false) warnings.push(`manifest budget exceeded for ${file}`);
    }
    if (manifest.generated_at) {
      const generatedMs = new Date(manifest.generated_at).getTime();
      if (Number.isFinite(generatedMs) && stats.some((stat) => stat.file !== ".octocode/awareness/manifest.json" && stat.mtime_ms != null && stat.mtime_ms > generatedMs + 1e3)) {
        warnings.push("manifest older than generated projection files; regenerate repo projection");
      }
    }
    return warnings;
  } catch {
    return [".octocode/awareness/manifest.json unreadable; regenerate repo projection"];
  }
}
function projectionWarnings(workspacePath, stats) {
  const budgets = {
    ".octocode/AGENTS.md": 80,
    ".octocode/MEMORY.md": 200,
    ".octocode/GOTCHAS.md": 200,
    ".octocode/LEARN.md": 200,
    ".octocode/BOOKMARKS.md": 200
  };
  const markdownWarnings = stats.flatMap((stat) => {
    const budget = budgets[stat.file];
    if (stat.lines == null) return [`${stat.file} missing; run repo inject when projection context is needed`];
    if (budget != null && stat.lines > budget) return [`${stat.file} has ${stat.lines} lines over budget ${budget}`];
    return [];
  });
  return [...markdownWarnings, ...manifestWarnings(workspacePath, stats)];
}
function evidenceTrust(references) {
  if (references.length === 0) return "needs_refs";
  if (references.some((ref) => ref.includes(".octocode/") || ref.startsWith("http"))) return "generated_or_external_lead";
  return "verified_lead";
}
function resourceLeads(query, workspacePath) {
  const haystack = query.toLowerCase();
  const leads = [];
  const add = (source, why, verification = "lead_to_verify") => {
    leads.push({ source, why, verification });
  };
  if (/(awareness|homeostatic|attend|workboard|memory|wiki|task|reflection|drive|motivation|resource|creative|personality)/.test(haystack)) {
    add(
      join7(workspacePath, ".octocode", "rfc", "homeostatic-awareness-loop", "RFC.md"),
      "RFC goals and decision for the awareness loop"
    );
    add(
      join7(workspacePath, ".octocode", "rfc", "homeostatic-awareness-loop", "IMPLEMENTATION.md"),
      "dependency-ordered build plan for workboard, attend, drive_state, and digest"
    );
    add(
      join7(workspacePath, "packages", "octocode-awareness", "skills", "octocode-awareness", "references", "homeostatic-loop.md"),
      "compact agent-facing organ and drive map"
    );
  }
  if (/(role.?dialogue|self.?reflection|tutor|student|builder|tester|alter.?ego|debate|duo)/.test(haystack)) {
    add(
      join7(workspacePath, "packages", "octocode-awareness", "skills", "octocode-awareness", "references", "self-reflection-dialogue.md"),
      "role-dialogue pattern for hard ideas without persona bloat"
    );
  }
  if (leads.length === 0) {
    add(join7(workspacePath, ".octocode", "AGENTS.md"), "generated repo context entrypoint, if present");
    add(join7(workspacePath, "AGENTS.md"), "workspace-level agent instructions");
  }
  return leads.slice(0, 4);
}
function chooseMode(query, evidenceCount, verifyCount, gapCount) {
  if (verifyCount > 0 && gapCount === 0) return "exploit";
  if (evidenceCount === 0 || /(design|rfc|brainstorm|research|unknown|approach|why|how)/i.test(query)) return verifyCount > 0 ? "mixed" : "explore";
  return gapCount > 0 ? "mixed" : "exploit";
}
function attendAwareness(db3, params = {}) {
  const cwd = params.cwd ? resolve13(params.cwd) : process.cwd();
  const workspacePath = resolve13(String(params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd));
  const limit = limitOf2(params.limit);
  const query = String(params.query ?? "").trim();
  const files = stringList2(params.file);
  const includeBodies = Boolean(params.includeBodies ?? params.include_bodies);
  const explainOrgan = Boolean(params.explainOrgan ?? params.explain_organ);
  const compact2 = Boolean(params.compact);
  const packetLimit = compact2 ? 1 : limit;
  const scope = {
    workspacePath,
    artifact: params.artifact ?? null,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    query: query || null,
    limit,
    includeBodies,
    cwd
  };
  const profileResult = queryAwareness(db3, { ...scope, view: "repo-profile" });
  const profile = profileMap(profileResult.rows);
  const workboardResult = queryAwareness(db3, { ...scope, view: "workboard", query: null });
  const rawWorkboard = groupWorkboard(workboardResult.rows);
  const handoffRows = (rawWorkboard["Inbox"] ?? []).filter((row) => row["item_type"] === "refinement" && row["quality"] === "handoff").slice(0, packetLimit).map((row) => compact2 ? compactRow(row) : row);
  const workboard = compact2 ? compactWorkboard(rawWorkboard, packetLimit) : rawWorkboard;
  const verificationTargets = (rawWorkboard["Verify"] ?? []).slice(0, packetLimit);
  const readyTasks = rawWorkboard["Ready"] ?? [];
  const claimedTasks = (rawWorkboard["Claimed"] ?? []).filter((row) => row["item_type"] === "task");
  const projectionHealth = projectionStats(workspacePath);
  const bloatWarnings = projectionWarnings(workspacePath, projectionHealth);
  const outputBloatWarnings = compact2 ? bloatWarnings.map((warning) => warning.replace(/\.octocode\//g, "").replace(/ has /g, " ").replace(/ lines over budget /g, ">").replace(/ lines/g, "l")) : bloatWarnings;
  const memoryQuery = query || files.join(" ");
  const recall = memoryQuery ? getMemory(db3, {
    query: memoryQuery,
    limit: Math.min(5, limit),
    minImportance: 1,
    smart: true,
    workspacePath,
    artifact: params.artifact ?? null,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    files,
    explain: true,
    cwd
  }) : { count: 0, memories: [], mode: "lexical", sort: "smart", as_of: null, global_only: false, states: ["ACTIVE"] };
  const evidence = recall.memories.slice(0, packetLimit).map((memory) => {
    const allReferences = memory.references ?? [];
    const references = compact2 ? allReferences.slice(0, 1) : allReferences;
    const why = [
      query ? `matched query "${summarize2(query, 80)}"` : null,
      files.length > 0 ? `scoped to ${files.join(", ")}` : null,
      `importance ${memory.importance}`,
      memory.failure_signature ? "has failure signature" : null
    ].filter((item) => Boolean(item));
    return {
      kind: "memory",
      id: memory.memory_id,
      label: memory.label,
      importance: memory.importance,
      title: summarize2(memory.task_context, compact2 ? 60 : 120),
      summary: summarize2(memory.observation, compact2 ? 120 : 240),
      references,
      ...compact2 ? {} : {
        reference_count: allReferences.length,
        omitted_reference_count: Math.max(0, allReferences.length - references.length)
      },
      why_selected: compact2 ? why.slice(0, 2) : why,
      trust: evidenceTrust(allReferences)
    };
  });
  const trustWarnings = evidence.filter((item) => item.trust !== "verified_lead").map((item) => `${item.id}: ${item.trust}`);
  const gaps = [
    query ? null : "No query supplied; packet is a general workspace briefing.",
    evidence.length === 0 && memoryQuery ? `No memory evidence selected for "${summarize2(memoryQuery, 80)}".` : null,
    verificationTargets.length === 0 ? null : `${verificationTargets.length} verification target(s) need attention.`,
    bloatWarnings.length === 0 ? null : `${bloatWarnings.length} projection/bloat warning(s) present.`
  ].filter((gap) => Boolean(gap));
  const mode = chooseMode(query, evidence.length, verificationTargets.length, gaps.length);
  const resourceLeadRows = resourceLeads(query || memoryQuery, workspacePath).slice(0, compact2 ? 2 : limit).map((lead) => {
    const source = lead.source ?? "";
    return compact2 && source.startsWith(`${workspacePath}/`) ? { ...lead, source: source.slice(workspacePath.length + 1) } : lead;
  });
  const alternatives = mode === "explore" || mode === "mixed" ? [
    { option: "derive_view_first", why: "Prefer read-only DB projections before new canonical storage." },
    { option: "narrow_scope", why: "Use query/file filters if the packet is too broad." }
  ] : [];
  const compactProjectionHealth = compact2 ? projectionHealth.map((item) => ({ file: item.file, lines: item.lines })) : projectionHealth;
  const organState = {
    senses: {
      ...compact2 ? {} : { profile },
      projection_health: compactProjectionHealth
    },
    attention: {
      selected_evidence: evidence.length,
      workboard_items: workboardResult.count,
      ready_tasks: readyTasks.length,
      claimed_tasks: claimedTasks.length,
      compact_budget: compact2 ? "<=8KB JSON" : "unbounded caller output"
    },
    memory: {
      active_memories: profile["active_memories"] ?? 0,
      gotchas: profile["gotchas"] ?? 0,
      lessons: profile["lessons"] ?? 0,
      recall_mode: recall.mode
    },
    error_signals: {
      verification_targets: verificationTargets.length,
      trust_warnings: trustWarnings.length
    },
    pruning_candidates: {
      memory_review: workboard["MemoryReview"]?.length ?? 0,
      projection_warnings: bloatWarnings.length
    },
    bridge: {
      inbox: workboard["Inbox"]?.length ?? 0,
      handoffs: handoffRows.length,
      open_refinements: profile["open_refinements"] ?? 0,
      open_signals: profile["open_signals"] ?? 0,
      plans: profile["plans"] ?? 0,
      tasks: profile["tasks"] ?? 0
    },
    projection: {
      warnings: outputBloatWarnings
    }
  };
  const signalIds = uniqueStrings((workboard["Inbox"] ?? []).filter((row) => row["item_type"] === "signal").map((row) => String(row["id"])));
  const handoffIds = uniqueStrings(handoffRows.map((row) => String(row["id"])));
  const refinementIds = uniqueStrings(Object.values(workboard).flat().filter((row) => row["item_type"] === "refinement").map((row) => String(row["id"])));
  const agentIds = uniqueStrings(Object.values(workboard).flat().map((row) => String(row["agent_id"] ?? "")));
  const sourceRefs = evidence.flatMap((item) => item.references);
  const driveState = {
    goal: query || "general workspace awareness",
    mode,
    learning_gaps: gaps,
    resource_leads: resourceLeadRows,
    alternatives,
    team_norms: TEAM_NORMS,
    transactive_map: {
      ready_task_ids: readyTasks.map((row) => String(row["id"])).slice(0, compact2 ? 3 : 12),
      claimed_task_ids: claimedTasks.map((row) => String(row["id"])).slice(0, compact2 ? 3 : 12),
      memory_ids: evidence.map((item) => item.id),
      signal_ids: signalIds.slice(0, compact2 ? 3 : 12),
      signal_id_count: signalIds.length,
      handoff_ids: handoffIds.slice(0, compact2 ? 3 : 12),
      handoff_id_count: handoffIds.length,
      refinement_ids: refinementIds.slice(0, compact2 ? 4 : 12),
      refinement_id_count: refinementIds.length,
      agent_ids: agentIds.slice(0, compact2 ? 6 : 24),
      agent_id_count: agentIds.length,
      source_refs: sourceRefs.slice(0, compact2 ? 5 : 12),
      source_ref_count: sourceRefs.length
    }
  };
  const next = verificationTargets.length > 0 ? 'octocode-awareness verify audit --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact; then verify mark --all-pending after the declared test plan' : readyTasks.length > 0 ? `octocode-awareness task claim --task-id ${String(readyTasks[0]?.["id"])} --agent-id "$OCTOCODE_AGENT_ID" --compact` : bloatWarnings.length > 0 ? 'octocode-awareness memory forget --workspace "$PWD" --dry-run --compact; then repo inject --workspace "$PWD" --compact to regenerate capped projections (digest does not shrink markdown)' : evidence.length > 0 ? "Treat evidence as leads; re-check cited files, then work start before edits" : 'octocode-awareness attend --workspace "$PWD" --query "<narrower task>" --compact; or query workboard / workspace status';
  if (compact2) {
    const columnCount = (column) => {
      const rows = rawWorkboard[column] ?? [];
      return Number(rows[0]?.["column_total"] ?? rows.length);
    };
    return {
      ok: true,
      schema_version: 3,
      generated_at: profileResult.generated_at,
      workspace_path: workspacePath,
      artifact: params.artifact ?? null,
      repo: params.repo ?? null,
      ref: params.ref ?? null,
      counts: {
        Inbox: columnCount("Inbox"),
        Ready: columnCount("Ready"),
        Claimed: columnCount("Claimed"),
        Verify: columnCount("Verify"),
        FilesUnderWork: columnCount("FilesUnderWork")
      },
      workboard,
      evidence,
      next
    };
  }
  const result = {
    ok: true,
    schema_version: 3,
    generated_at: profileResult.generated_at,
    workspace_path: workspacePath,
    artifact: params.artifact ?? null,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    profile,
    organ_state: organState,
    drive_state: driveState,
    workboard,
    evidence,
    gaps,
    bloat_warnings: outputBloatWarnings,
    verification_targets: verificationTargets,
    trust_warnings: trustWarnings,
    trace: [
      { step: "repo-profile", count: profileResult.count },
      { step: "workboard", count: workboardResult.count },
      { step: "memory-recall", count: evidence.length, note: memoryQuery ? void 0 : "skipped-empty-query" },
      { step: "projection-health", count: projectionHealth.length }
    ],
    next
  };
  if (explainOrgan) result.organ_reference = ORGAN_REFERENCE;
  return result;
}

// bin/hook-runner.ts
import { createHash as createHash3 } from "node:crypto";
import { mkdirSync as mkdirSync5, readFileSync as readFileSync4, renameSync as renameSync2, unlinkSync as unlinkSync2, writeFileSync as writeFileSync4 } from "node:fs";
import { basename as basename3, dirname as dirname5, join as join8, relative as relative5, resolve as resolve14 } from "node:path";
import { fileURLToPath as fileURLToPath3 } from "node:url";

// src/sessions.ts
import { randomUUID as randomUUID12 } from "node:crypto";
function endSession(db3, params) {
  const now = utcNow();
  const result = db3.prepare(SESSIONS_UPDATE_END).get(
    now,
    params.summary ?? null,
    params.sessionId
  );
  return result ?? null;
}

// src/pi-hooks.ts
import path from "node:path";
import { spawnSync as spawnSync5 } from "node:child_process";
import { randomUUID as randomUUID13 } from "node:crypto";
import { realpathSync as realpathSync3 } from "node:fs";
var _sessionStartupToken = randomUUID13().slice(0, 8);
function addPathValue(paths, value) {
  if (typeof value === "string" && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
  }
}
function addApplyPatchPaths(paths, command2) {
  if (typeof command2 !== "string") return;
  for (const line of command2.split("\n")) {
    const addUpdDel = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
    if (addUpdDel) {
      paths.push(addUpdDel[1].trim());
      continue;
    }
    const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
    if (moveTo) paths.push(moveTo[1].trim());
  }
}
function objectOrEmpty(value) {
  return value && typeof value === "object" ? value : {};
}
function firstString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}
function addQueryPaths(paths, value) {
  if (!Array.isArray(value)) return;
  for (const query of value) {
    const payload = objectOrEmpty(query);
    addPathValue(paths, payload.path);
    addPathValue(paths, payload.filePath);
    addPathValue(paths, payload.file_path);
    addPathValue(paths, payload.paths);
    addPathValue(paths, payload.filePaths);
    addPathValue(paths, payload.file_paths);
  }
}
function extractPiWriteTargetPaths(toolName2, input = {}, options = {}) {
  const normalizedToolName = String(toolName2 ?? "").toLowerCase();
  const isWriteTool = Boolean(options.assumeWrite) || [
    "write",
    "edit",
    "multi_edit",
    "multiedit",
    "notebookedit",
    "notebook_edit",
    "apply_patch",
    "applypatch"
  ].includes(normalizedToolName);
  const payload = objectOrEmpty(input);
  const command2 = typeof input === "string" ? input : firstString(payload.command, payload.patch);
  if (!isWriteTool) {
    const patchPaths = [];
    addApplyPatchPaths(patchPaths, command2);
    return [...new Set(patchPaths)];
  }
  const paths = [];
  addPathValue(paths, payload.path);
  addPathValue(paths, payload.filePath);
  addPathValue(paths, payload.file_path);
  addPathValue(paths, payload.paths);
  addPathValue(paths, payload.filePaths);
  addPathValue(paths, payload.file_paths);
  addQueryPaths(paths, payload.queries);
  addApplyPatchPaths(paths, command2);
  return [...new Set(paths)];
}
function canonicalPath(input) {
  const resolved = path.resolve(input);
  try {
    return realpathSync3(resolved);
  } catch {
    const missingParts = [];
    let cursor = resolved;
    while (true) {
      const parent = path.dirname(cursor);
      if (parent === cursor) return resolved;
      missingParts.unshift(path.basename(cursor));
      cursor = parent;
      try {
        return path.join(realpathSync3(cursor), ...missingParts);
      } catch {
        continue;
      }
    }
  }
}
function resolvePiTargetPath(file, cwd) {
  return canonicalPath(path.isAbsolute(file) ? file : path.resolve(cwd, file));
}
function isInsidePath(candidate, root) {
  const resolvedCandidate = canonicalPath(candidate);
  const resolvedRoot = canonicalPath(root);
  const rel = path.relative(resolvedRoot, resolvedCandidate);
  return rel === "" || Boolean(rel && !rel.startsWith("..") && !path.isAbsolute(rel));
}
function gitBranchOf(dir) {
  try {
    const result = spawnSync5("git", ["-C", dir, "rev-parse", "--abbrev-ref", "HEAD"], {
      encoding: "utf8",
      timeout: 5e3
    });
    return result.status === 0 ? String(result.stdout).trim() : null;
  } catch {
    return null;
  }
}
function evaluateHarnessGuard(params) {
  const { targetFiles, skillRoot, cwd } = params;
  const env = params.env ?? process.env;
  if (!skillRoot) return null;
  if (targetFiles.length === 0) return null;
  const insideSkill = targetFiles.some((file) => isInsidePath(resolvePiTargetPath(file, cwd), skillRoot));
  if (!insideSkill) return null;
  if (env.OCTOCODE_ALLOW_HARNESS_APPLY !== "1") {
    return "octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1.";
  }
  const branch = gitBranchOf(skillRoot);
  if (branch === "main" || branch === "master") {
    return `octocode-awareness: harness self-fix is never allowed on ${branch}. Create a dedicated branch first.`;
  }
  if (!branch || branch === "HEAD") {
    if (env.OCTOCODE_HARNESS_BRANCH_OK !== "1") {
      return "octocode-awareness: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge.";
    }
  }
  return null;
}

// bin/hook-runner.ts
function readStdin() {
  return new Promise((resolve16) => {
    let raw = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      raw += chunk;
    });
    process.stdin.on("end", () => resolve16(raw));
    process.stdin.on("error", () => resolve16(raw));
  });
}
function parsePayload(raw) {
  try {
    const parsed = JSON.parse(raw || "{}");
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return raw.trim() ? { input: raw } : {};
  }
}
function objectOrEmpty2(value) {
  return value && typeof value === "object" ? value : {};
}
function payloadInput(payload) {
  return payload.tool_input ?? payload.input ?? payload.args ?? payload;
}
function payloadForFileExtraction(payload) {
  const input = payloadInput(payload);
  const inputObj = objectOrEmpty2(input);
  if (inputObj === payload) return input;
  if (Object.keys(inputObj).length === 0) return input;
  return { ...payload, ...inputObj };
}
var warnedFallbackAgentId = false;
function firstString2(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}
function agentId(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  const explicit = firstString2(
    process.env.OCTOCODE_AGENT_ID,
    payload.agent_id,
    payload.agentId,
    input.agent_id,
    input.agentId,
    payload.session_id,
    payload.sessionId,
    input.session_id,
    input.sessionId
  );
  if (explicit) return explicit;
  const host = firstString2(
    process.env.OCTOCODE_AGENT_HOST,
    payload.host,
    payload.client,
    payload.source,
    payload.context
  ) ?? "shell";
  const scope = `${host}\0${workspace(payload) ?? process.cwd()}`;
  const suffix = createHash3("sha1").update(scope).digest("hex").slice(0, 12);
  const fallback = `hook:${host.replace(/[^a-zA-Z0-9_.:-]/g, "_")}:${suffix}`;
  if (!warnedFallbackAgentId) {
    warnedFallbackAgentId = true;
    console.error(`octocode-awareness: OCTOCODE_AGENT_ID or host session id missing; using fallback agent id "${fallback}". Set OCTOCODE_AGENT_ID for reliable multi-agent awareness.`);
  }
  return fallback;
}
function sessionId(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  return firstString2(
    payload.session_id,
    payload.sessionId,
    input.session_id,
    input.sessionId
  );
}
function toolName(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  return firstString2(
    payload.tool_name,
    payload.toolName,
    payload.name,
    input.tool_name,
    input.toolName
  ) ?? "";
}
function autoClaimRationale(payload, files) {
  const tool = toolName(payload);
  const names = files.map((f) => f.split("/").pop() || f);
  const shown = names.slice(0, 3).join(", ");
  const extra = names.length > 3 ? ` +${names.length - 3} more` : "";
  const action = tool ? `${tool}` : "edit";
  return `auto: ${action} ${shown}${extra} (lifecycle hook)`;
}
function agentName(payload) {
  const value = process.env.OCTOCODE_AGENT_NAME ?? payload.agent_name ?? payload.agentName ?? payload.agent_display_name ?? payload.agentDisplayName;
  return typeof value === "string" && value.trim() ? value.trim() : "";
}
function workspace(payload) {
  const value = payload.cwd ?? payload.workspace ?? payload.workspacePath;
  return typeof value === "string" && value.trim() ? value.trim() : null;
}
function artifact(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  const value = process.env.OCTOCODE_ARTIFACT ?? process.env.OCTOCODE_PACKAGE ?? process.env.OCTOCODE_SERVICE ?? payload.artifact ?? payload.package ?? payload.service ?? input.artifact ?? input.package ?? input.service;
  return typeof value === "string" && value.trim() ? value.trim() : null;
}
function hookReason(payload) {
  return typeof payload.reason === "string" ? payload.reason : "";
}
function isStopHookActive(payload) {
  return Boolean(payload.stop_hook_active);
}
function extractFiles(payload) {
  const input = payloadForFileExtraction(payload);
  const inputObj = objectOrEmpty2(input);
  const toolName2 = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? "";
  return extractPiWriteTargetPaths(toolName2, input, { assumeWrite: true });
}
function resolveHookPath(file, cwd = process.cwd()) {
  return canonicalizePath(resolve14(cwd, file));
}
function db() {
  return connectDb(resolveDbPath(null));
}
function hookRunStateDir() {
  const stateDir = join8(dirname5(resolveDbPath(null)), "hook-state", "runs");
  mkdirSync5(stateDir, { recursive: true });
  return stateDir;
}
function hookRunStateFile(key) {
  return join8(hookRunStateDir(), `${key}.json`);
}
function readHookRunEntries(key) {
  try {
    const parsed = JSON.parse(readFileSync4(hookRunStateFile(key), "utf8"));
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}
function writeHookRunEntries(key, entries) {
  const file = hookRunStateFile(key);
  if (entries.length === 0) {
    try {
      unlinkSync2(file);
    } catch {
    }
    return;
  }
  const tempFile = `${file}.${process.pid}.${Date.now()}.tmp`;
  writeFileSync4(tempFile, JSON.stringify(entries, null, 2) + "\n", "utf8");
  renameSync2(tempFile, file);
}
function hookEventId(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  return firstString2(
    payload.tool_use_id,
    payload.toolUseId,
    payload.tool_call_id,
    payload.toolCallId,
    payload.event_id,
    payload.eventId,
    payload.id,
    input.tool_use_id,
    input.toolUseId,
    input.tool_call_id,
    input.toolCallId,
    input.event_id,
    input.eventId,
    input.id
  );
}
function hookRunKey(payload, files, cwd) {
  const explicitId = hookEventId(payload);
  const identity = {
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve14(cwd),
    artifact: artifact(payload),
    event: explicitId,
    files: explicitId ? [] : files.map((file) => resolveHookPath(file, cwd)).sort()
  };
  return createHash3("sha1").update(JSON.stringify(identity)).digest("hex");
}
function recordHookRun(payload, files, cwd, runId) {
  const key = hookRunKey(payload, files, cwd);
  const entries = readHookRunEntries(key);
  entries.push({
    runId,
    files: files.map((file) => resolveHookPath(file, cwd)),
    createdAt: (/* @__PURE__ */ new Date()).toISOString()
  });
  writeHookRunEntries(key, entries.slice(-20));
}
function consumeHookRun(payload, files, cwd) {
  const key = hookRunKey(payload, files, cwd);
  const entries = readHookRunEntries(key);
  const entry2 = entries.shift();
  writeHookRunEntries(key, entries);
  return entry2?.runId ?? null;
}
function activeRunForFiles(database, params) {
  const absFiles = params.files.map((file) => resolveHookPath(file, params.workspacePath));
  if (absFiles.length === 0) return null;
  const rows = listWork(database, {
    agentId: params.agentId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    activeOnly: true
  }).files.filter((entry2) => params.origins.includes(entry2.origin));
  const byRun = /* @__PURE__ */ new Map();
  for (const row of rows) {
    const paths = byRun.get(row.run_id) ?? /* @__PURE__ */ new Set();
    paths.add(row.file_path);
    byRun.set(row.run_id, paths);
  }
  const matches = [...byRun].filter(([, paths]) => absFiles.every((file) => paths.has(file)));
  return matches.length === 1 ? matches[0][0] : null;
}
function runOrigin(database, runId) {
  const row = database.prepare("SELECT origin FROM task_runs WHERE run_id = ?").get(runId);
  return row?.origin ?? null;
}
function peerStateDir() {
  const stateDir = join8(dirname5(resolveDbPath(null)), "hook-state", "peers");
  mkdirSync5(stateDir, { recursive: true });
  return stateDir;
}
function peerStateKey(payload, files, cwd) {
  return createHash3("sha1").update(JSON.stringify({
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve14(cwd),
    artifact: artifact(payload),
    files: files.map((file) => resolveHookPath(file, cwd)).sort()
  })).digest("hex");
}
function peerFingerprint(peers) {
  return createHash3("sha1").update(JSON.stringify(peers.map((peer) => ({
    agent: peer.agent_id,
    file: peer.file_path,
    task: peer.task_id,
    origin: peer.origin,
    rationale: peer.rationale,
    exclusive: peer.exclusive
  })).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))))).digest("hex");
}
function peerLabel(peer) {
  const work = peer.task_id ?? peer.origin;
  const reason = peer.rationale.replace(/\s+/g, " ").trim().slice(0, 40);
  return `${peer.agent_id}:${work}${reason ? `(${reason})` : ""}`;
}
function emitPeerDelta(payload, files, cwd, allPeers) {
  const targetSet = new Set(files.map((file) => resolveHookPath(file, cwd)));
  const peers = allPeers.filter((peer) => peer.agent_id !== agentId(payload) && targetSet.has(peer.file_path));
  const key = peerStateKey(payload, files, cwd);
  const stateFile = join8(peerStateDir(), `${key}.txt`);
  const fingerprint = peerFingerprint(peers);
  let previous = null;
  try {
    previous = readFileSync4(stateFile, "utf8").trim();
  } catch {
  }
  if (previous === fingerprint) return;
  writeFileSync4(stateFile, fingerprint, "utf8");
  if (peers.length === 0) return;
  const shown = peers.slice(0, 3).map(peerLabel).join("; ");
  const omitted = peers.length > 3 ? ` +${peers.length - 3}` : "";
  const canonicalWorkspace = canonicalizePath(cwd);
  const targets = files.slice(0, 2).map((file) => relative5(canonicalWorkspace, resolveHookPath(file, cwd)) || basename3(file)).join(",");
  const message = `AWARE ${targets} | peers ${shown}${omitted}`;
  process.stdout.write(`${JSON.stringify({ additionalContext: message })}
`);
}
function hookAgentContext(payload, hookName) {
  const value = process.env.OCTOCODE_AGENT_CONTEXT ?? process.env.OCTOCODE_AGENT_HOST ?? payload.context ?? payload.host ?? payload.client ?? payload.source;
  return typeof value === "string" && value.trim() ? value.trim() : hookName;
}
function registerHookAgent(database, payload, hookName) {
  try {
    registerAgent(database, {
      agentId: agentId(payload),
      agentName: agentName(payload),
      workspacePath: workspace(payload),
      artifact: artifact(payload),
      context: hookAgentContext(payload, hookName)
    });
  } catch {
  }
}
function scopeArgs(payload) {
  const ws = workspace(payload);
  const art = artifact(payload);
  return {
    ...ws ? { workspacePath: ws } : {},
    ...art ? { artifact: art } : {}
  };
}
async function runPreEdit(payload) {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const hookWorkspace = workspace(payload) ?? process.cwd();
  const guardReason = evaluateHarnessGuard({
    targetFiles: files,
    skillRoot: process.env.OCTOCODE_SKILL_ROOT,
    cwd: hookWorkspace
  });
  if (guardReason) {
    console.error(`${guardReason} Edit blocked.`);
    return 2;
  }
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:pre-edit");
    const hookAgentId = agentId(payload);
    const hookArtifact = artifact(payload);
    const activeClaim = activeTaskClaimForAgent(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact
    });
    const explicitRunId = activeClaim ? null : activeRunForFiles(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      files,
      origins: ["WORK"]
    });
    const result = explicitRunId ? { ok: true, ...touchWork(database, {
      agentId: hookAgentId,
      runId: explicitRunId,
      targetFiles: files,
      ttlMs: 10 * 6e4
    }) } : startWork(database, {
      agentId: hookAgentId,
      sessionId: sessionId(payload),
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      runId: activeClaim?.run_id,
      rationale: autoClaimRationale(payload, files),
      testPlan: "post-edit verification",
      targetFiles: files,
      origin: "HOOK",
      source: "HOOK",
      ttlMs: 10 * 6e4
    });
    if (!result.ok) {
      const detail = result.conflicts.slice(0, 3).map((conflict) => `${relative5(hookWorkspace, conflict.file_path)} (${conflict.agent_id})`).join(", ");
      console.error(`octocode-awareness: exclusive file work blocks this edit${detail ? `: ${detail}` : ""}.`);
      return 2;
    }
    recordHookRun(payload, files, hookWorkspace, result.run.run_id);
    emitPeerDelta(payload, files, hookWorkspace, result.peers);
    return 0;
  } catch (error) {
    console.error(`octocode-awareness pre-flight warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
    return 0;
  }
}
async function runPostEdit(payload) {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:post-edit");
    const hookAgentId = agentId(payload);
    const hookWorkspace = workspace(payload) ?? process.cwd();
    const hookArtifact = artifact(payload);
    const correlatedRunId = consumeHookRun(payload, files, hookWorkspace) ?? activeTaskClaimForAgent(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact
    })?.run_id ?? activeRunForFiles(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      files,
      origins: ["WORK", "HOOK"]
    });
    if (!correlatedRunId) {
      console.error("octocode-awareness post-edit warning (continuing): could not identify a unique work run; leaving presence for expiry.");
      return 0;
    }
    const origin = runOrigin(database, correlatedRunId);
    if (origin === "HOOK") {
      endWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files
      });
    } else {
      touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
        ttlMs: 10 * 6e4
      });
    }
    for (const file of files) {
      insertEditLog(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        filePath: resolveHookPath(file, hookWorkspace),
        operation: "update",
        workspacePath: hookWorkspace,
        artifact: hookArtifact
      });
    }
  } catch (error) {
    console.error(`octocode-awareness post-edit warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}
async function runHarnessGuard(payload) {
  const reason = evaluateHarnessGuard({
    targetFiles: extractFiles(payload),
    skillRoot: process.env.OCTOCODE_SKILL_ROOT,
    cwd: process.cwd()
  });
  if (reason) {
    console.error(`${reason} Edit blocked.`);
    return 2;
  }
  return 0;
}
async function runStopVerify(payload) {
  if (process.env.OCTOCODE_NO_VERIFY_GATE === "1" || isStopHookActive(payload)) return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:stop-verify");
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      const details = [
        ...report.unverified.map((run) => `${run.status}:${run.run_id}: ${run.test_plan}`),
        ...report.stale_active.map((run) => `STALE:${run.run_id}: ${run.rationale}`)
      ];
      const shown = details.slice(0, 3);
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : "";
      console.error(`octocode-awareness: concluding with unverified work. ${shown.join("; ")}${omitted}`);
      return 2;
    }
  } catch (error) {
    console.error(`octocode-awareness verify warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}
function maybeRunDigest(payload) {
  if (process.env.OCTOCODE_NO_DIGEST === "1") return;
  if (process.env.OCTOCODE_NOTIFY_RUN_DIGEST !== "1") return;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 36e5 : 4 * 36e5;
  const memoryHome2 = process.env.OCTOCODE_MEMORY_HOME || `${process.env.HOME ?? ""}/.octocode/memory`;
  const markerPath = join8(memoryHome2, ".last-digest-epoch-ms");
  try {
    const database = db();
    let last = 0;
    try {
      last = Number(readFileSync4(markerPath, "utf8").trim() || 0);
    } catch {
      last = 0;
    }
    const now = Date.now();
    if (!last || now - last >= intervalMs) {
      mkdirSync5(memoryHome2, { recursive: true });
      writeFileSync4(markerPath, String(now), "utf8");
      digest(database, { workspace: workspace(payload), memoryHome: memoryHome2 });
    }
  } catch (error) {
    console.error(`octocode-awareness digest warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
}
async function runNotifyDeliver(payload) {
  if (process.env.OCTOCODE_NO_NOTIFY === "1") return 0;
  maybeRunDigest(payload);
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:notify-deliver");
    const result = notifyGet(database, {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? void 0,
      artifact: artifact(payload) ?? void 0,
      format: "hook"
    });
    if (result.additionalContext) {
      process.stdout.write(JSON.stringify({
        additionalContext: result.additionalContext
      }) + "\n");
    }
  } catch (error) {
    console.error(`octocode-awareness session-capture warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}
async function runSessionEnd(payload) {
  if (process.env.OCTOCODE_NO_SESSION_CAPTURE === "1" || hookReason(payload) === "clear") return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:session-end");
    sessionCapture(database, {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? void 0,
      artifact: artifact(payload) ?? void 0,
      reason: hookReason(payload) || void 0
    });
    const sid = sessionId(payload);
    if (sid) endSession(database, { sessionId: sid });
  } catch {
  }
  return 0;
}
async function runHookCommand(command2, rawPayload) {
  if (command2 === "help" || command2 === "--help" || command2 === "-h") {
    process.stdout.write("usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n");
    return 0;
  }
  const payload = parsePayload(rawPayload ?? await readStdin());
  switch (command2) {
    case "pre-edit":
      return runPreEdit(payload);
    case "post-edit":
      return runPostEdit(payload);
    case "harness-guard":
      return runHarnessGuard(payload);
    case "stop-verify":
      return runStopVerify(payload);
    case "notify-deliver":
      return runNotifyDeliver(payload);
    case "session-end":
      return runSessionEnd(payload);
    default:
      console.error(`unknown hook command: ${command2}`);
      return 1;
  }
}
async function main() {
  return runHookCommand(process.argv[2] ?? "help");
}
var isMain = process.argv[1] ? fileURLToPath3(import.meta.url) === resolve14(process.argv[1]) : false;
var invokedAsHookRunner = process.argv[1] ? /^hook-runner\.(js|mjs|ts)$/.test(basename3(process.argv[1])) : false;
if (isMain && invokedAsHookRunner) {
  process.exitCode = await main();
}

// bin/awareness.ts
var MAX_CLI_TTL_SECONDS = 10 * 60;
var MAX_CLI_WAIT_SECONDS = 60 * 60;
var MAX_CLI_RETRY_INTERVAL_SECONDS = 5 * 60;
var MEMORY_SORTS = /* @__PURE__ */ new Set(["smart", "score", "importance", "recent", "accessed"]);
var ARRAY_FLAGS = /* @__PURE__ */ new Set([
  "tag",
  "tags",
  "reference",
  "file",
  "fix_file",
  "target_file",
  "supersedes",
  "label",
  "state",
  "memory_id",
  "refinement_id",
  "signal_id",
  "ref_id",
  "run_id",
  "regex",
  "file_regex",
  "to_agent",
  "kind",
  "path",
  "depends_on"
]);
function parseArgs(argv) {
  const result = { _: [] };
  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];
    if (arg === "--") {
      result._.push(...argv.slice(i + 1));
      break;
    }
    if (arg.startsWith("--no-")) {
      result[arg.slice(5).replace(/-/g, "_")] = false;
      i++;
      continue;
    }
    if (arg.startsWith("--")) {
      const key = arg.slice(2).replace(/-/g, "_");
      const next = argv[i + 1];
      if (next === void 0 || next.startsWith("--")) {
        result[key] = true;
        i++;
        continue;
      }
      i += 2;
      if (ARRAY_FLAGS.has(key)) {
        const cur = result[key];
        result[key] = Array.isArray(cur) ? [...cur, next] : [next];
      } else {
        result[key] = next;
      }
      continue;
    }
    result._.push(arg);
    i++;
  }
  return result;
}
var GLOBAL_FLAGS = ["db", "compact", "help"];
var NUMERIC_FLAGS = /* @__PURE__ */ new Set([
  "limit",
  "min_importance",
  "max_importance",
  "min_count",
  "min_edits",
  "min_lines",
  "older_than_days",
  "retention_days",
  "refinement_handoff_retention_days",
  "refinement_done_retention_days",
  "priority",
  "lease_minutes"
]);
var VALUE_REQUIRED_FLAGS = /* @__PURE__ */ new Set([
  "query",
  "observation",
  "lesson",
  "task",
  "task_context",
  "subject",
  "body",
  "rationale",
  "reasoning",
  "remember",
  "message",
  "fix_repo",
  "fix_harness",
  "fix_instructions",
  "in_reply_to",
  "thread_id",
  "name",
  "objective",
  "title",
  "acceptance",
  "blocked_reason",
  "path"
]);
var KNOWN_FLAGS = {
  "tell-memory": ["agent_id", "task_context", "observation", "importance", "label", "tag", "reference", "supersedes", "failure_signature", "valid_from", "valid_to", "workspace", "artifact", "repo", "ref", "file", "file_tree_fingerprint"],
  "get-memory": ["query", "limit", "min_importance", "label", "tag", "smart", "workspace", "artifact", "repo", "ref", "state", "sort", "global_only", "strict_scope", "as_of", "reference", "regex", "file_regex", "file", "explain", "semantic", "full"],
  "forget": ["memory_id", "tag", "tags", "before", "max_importance", "workspace", "artifact", "repo", "ref", "dry_run"],
  "reflect": ["agent_id", "task", "outcome", "lesson", "worked", "didnt_work", "fix_repo", "fix_file", "fix_harness", "fix_instructions", "failure_signature", "importance", "judgment_note", "duo", "eval_failure_json", "workspace", "artifact", "repo", "ref"],
  "refine-set": ["agent_id", "reasoning", "remember", "quality", "state", "workspace", "artifact", "repo", "ref", "file", "refinement_id"],
  "refine-get": ["workspace", "artifact", "repo", "ref", "quality", "include_handoffs", "state", "limit"],
  "refine-delete": ["refinement_id", "workspace", "artifact", "dry_run"],
  "pre-flight-intent": ["agent_id", "workspace", "artifact", "run_id", "rationale", "test_plan", "context_ref", "target_file", "file", "ttl_minutes", "ttl_seconds", "wait_seconds", "retry_interval", "strict_agent_id"],
  "release-file-lock": ["agent_id", "run_id", "target_file", "file", "status", "verified", "verified_note", "workspace", "artifact"],
  "status": ["workspace", "artifact", "limit"],
  "init": [],
  "self-test": [],
  "prune-stale-locks": ["older_than_minutes", "expired_only", "agent_id", "target_file", "workspace", "artifact", "dry_run"],
  "audit-unverified": ["agent_id", "workspace", "artifact", "abandon"],
  "verify": ["run_id", "all_pending", "agent_id", "status", "message", "workspace", "artifact"],
  "mine-weakness": ["agent_id", "workspace", "artifact", "min_count", "limit", "cwd"],
  "doc-staleness": ["agent_id", "workspace", "artifact", "targets_json", "min_edits", "min_lines", "propose", "session_id"],
  "docs-catalog": ["action", "name", "full"],
  "export-harness": ["limit", "min_importance", "workspace", "artifact"],
  "developer-review": ["workspace", "artifact", "repo", "ref", "state", "limit", "format", "query"],
  "query": ["view", "query", "limit", "format", "out", "workspace", "artifact", "repo", "ref", "agent_id", "state", "label", "file", "since", "include_bodies"],
  "attend": ["query", "limit", "workspace", "artifact", "repo", "ref", "file", "include_bodies", "explain_organ"],
  "repo-inject": ["query", "limit", "out", "out_dir", "workspace", "artifact", "repo", "ref", "mode", "check", "include_view", "include_bodies"],
  "agent-registry": ["action", "agent_id", "agent_name", "workspace", "artifact", "context", "limit"],
  "agent-signal": ["action", "agent_id", "workspace", "artifact", "repo", "ref", "kind", "subject", "body", "to_agent", "file", "ref_id", "importance", "in_reply_to", "thread_id", "signal_id", "all", "unread_only", "mark_read", "limit", "format", "include_bodies"],
  "notify-prune": ["signal_id", "resolved", "older_than_days", "dry_run", "workspace", "artifact"],
  "session-capture": ["agent_id", "workspace", "artifact", "repo", "ref", "reason", "cwd"],
  "wait-for-lock": ["agent_id", "target_file", "file", "workspace", "artifact", "wait_seconds", "retry_interval"],
  "digest": ["retention_days", "refinement_handoff_retention_days", "refinement_done_retention_days", "dry_run", "export_doc", "workspace", "artifact"],
  "hook-run": [],
  "hooks-install": ["host", "project_dir", "global", "check", "strict", "dry_run", "remove"],
  "schema": ["examples"],
  "plan-command": ["action", "plan_id", "name", "objective", "lead_agent_id", "agent_id", "workspace", "artifact", "status", "path", "title"],
  "task-command": ["action", "task_id", "plan_id", "title", "reasoning", "acceptance", "path", "created_by", "agent_id", "priority", "depends_on", "run_id", "lease_minutes", "message", "blocked_reason", "test_plan", "status", "next"],
  "work-command": ["action", "agent_id", "session_id", "workspace", "artifact", "run_id", "rationale", "test_plan", "context_ref", "target_file", "file", "exclusive", "ttl_minutes", "ttl_seconds", "all", "full"]
};
function validateFlags(command2, args2) {
  const known = KNOWN_FLAGS[command2];
  if (!known) return [];
  const allowed = /* @__PURE__ */ new Set([...known, ...GLOBAL_FLAGS]);
  return Object.keys(args2).filter((k) => k !== "_" && !allowed.has(k));
}
function validateFlagValues(args2) {
  for (const key of Object.keys(args2)) {
    if (key === "_") continue;
    const value = args2[key];
    if (NUMERIC_FLAGS.has(key)) {
      const n = typeof value === "string" ? Number(value) : NaN;
      if (value === true || !Number.isInteger(n)) {
        die(`--${key.replace(/_/g, "-")} expects an integer`, { got: value === true ? "flag with no value" : String(value) });
      }
    } else if (VALUE_REQUIRED_FLAGS.has(key) && value === true) {
      die(`--${key.replace(/_/g, "-")} expects a value (it was followed by another flag)`);
    }
  }
}
function parseBoundedSeconds(args2, key, min, max) {
  const raw = args2[key];
  if (raw == null || raw === false) return null;
  const flag2 = `--${key.replace(/_/g, "-")}`;
  const value = Number(String(raw));
  if (!Number.isInteger(value)) die(`${flag2} must be an integer`);
  if (value < min) die(`${flag2} must be >= ${min}`);
  if (value > max) die(`${flag2} must be <= ${max}`);
  return value;
}
function extractGlobalDb(argv) {
  let dbPath2 = null;
  const filtered = [];
  let i = 0;
  while (i < argv.length) {
    if (argv[i] === "--db" && i + 1 < argv.length) {
      dbPath2 = argv[i + 1];
      i += 2;
    } else {
      filtered.push(argv[i]);
      i++;
    }
  }
  return { dbPath: dbPath2, filtered };
}
var COMMAND_ROUTES = {
  "memory record": { command: "tell-memory" },
  "memory recall": { command: "get-memory" },
  "memory forget": { command: "forget" },
  "workspace status": { command: "status" },
  "lock acquire": { command: "pre-flight-intent" },
  "lock release": { command: "release-file-lock" },
  "lock wait": { command: "wait-for-lock" },
  "lock prune": { command: "prune-stale-locks" },
  "plan create": { command: "plan-command", prepend: ["--action", "create"] },
  "plan list": { command: "plan-command", prepend: ["--action", "list"] },
  "plan show": { command: "plan-command", prepend: ["--action", "show"] },
  "plan join": { command: "plan-command", prepend: ["--action", "join"] },
  "plan doc": { command: "plan-command", prepend: ["--action", "doc"] },
  "plan status": { command: "plan-command", prepend: ["--action", "status"] },
  "task create": { command: "task-command", prepend: ["--action", "create"] },
  "task list": { command: "task-command", prepend: ["--action", "list"] },
  "task ready": { command: "task-command", prepend: ["--action", "ready"] },
  "task show": { command: "task-command", prepend: ["--action", "show"] },
  "task claim": { command: "task-command", prepend: ["--action", "claim"] },
  "task heartbeat": { command: "task-command", prepend: ["--action", "heartbeat"] },
  "task submit": { command: "task-command", prepend: ["--action", "submit"] },
  "task release": { command: "task-command", prepend: ["--action", "release"] },
  "task depend": { command: "task-command", prepend: ["--action", "depend"] },
  "work start": { command: "work-command", prepend: ["--action", "start"] },
  "work touch": { command: "work-command", prepend: ["--action", "touch"] },
  "work end": { command: "work-command", prepend: ["--action", "end"] },
  "work list": { command: "work-command", prepend: ["--action", "list"] },
  "work show": { command: "work-command", prepend: ["--action", "show"] },
  "verify mark": { command: "verify" },
  "verify audit": { command: "audit-unverified" },
  "refinement set": { command: "refine-set" },
  "refinement get": { command: "refine-get" },
  "refinement delete": { command: "refine-delete" },
  "signal publish": { command: "agent-signal", prepend: ["--action", "publish"] },
  "signal list": { command: "agent-signal", prepend: ["--action", "list"] },
  "signal reply": { command: "agent-signal", prepend: ["--action", "reply"] },
  "signal ack": { command: "agent-signal", prepend: ["--action", "ack"] },
  "signal resolve": { command: "agent-signal", prepend: ["--action", "resolve"] },
  "signal prune": { command: "notify-prune" },
  "agent register": { command: "agent-registry", prepend: ["--action", "register"] },
  "agent list": { command: "agent-registry", prepend: ["--action", "list"] },
  "session capture": { command: "session-capture" },
  "reflect record": { command: "reflect" },
  "reflect mine-weakness": { command: "mine-weakness" },
  "reflect export-harness": { command: "export-harness" },
  "reflect developer-review": { command: "developer-review" },
  "docs list": { command: "docs-catalog", prepend: ["--action", "list"] },
  "docs show": { command: "docs-catalog", prepend: ["--action", "show"] },
  "docs staleness": { command: "doc-staleness" },
  "maintenance digest": { command: "digest" },
  "maintenance init": { command: "init" },
  "maintenance self-test": { command: "self-test" },
  "repo inject": { command: "repo-inject" }
};
var SINGLE_COMMANDS = /* @__PURE__ */ new Set(["query", "attend", "schema"]);
var UNKNOWN_COMMAND = "__unknown__";
function normalizeToken(value) {
  return value?.replace(/_/g, "-");
}
function selectCommand(argv) {
  const [firstRaw, secondRaw, thirdRaw, ...tail] = argv;
  const first = normalizeToken(firstRaw);
  if (!first) return { command: void 0, rest: [] };
  if (first.startsWith("-")) {
    if (first === "--compact" && argv.length > 1) {
      const sel = selectCommand(argv.slice(1));
      if (sel.command && sel.command !== UNKNOWN_COMMAND) {
        return { command: sel.command, rest: [...sel.rest, "--compact"] };
      }
    }
    return argv.every((arg) => arg === "--compact") ? { command: void 0, rest: argv } : { command: UNKNOWN_COMMAND, rest: argv };
  }
  const second = normalizeToken(secondRaw);
  if (first === "hook" && second === "run") {
    return { command: "hook-run", rest: thirdRaw ? [thirdRaw, ...tail] : tail };
  }
  if (first === "hooks" && second) {
    if (second === "install") return { command: "hooks-install", rest: thirdRaw ? [thirdRaw, ...tail] : tail };
    if (second === "check") return { command: "hooks-install", rest: ["--check", ...thirdRaw ? [thirdRaw, ...tail] : tail] };
    if (second === "remove") return { command: "hooks-install", rest: ["--remove", ...thirdRaw ? [thirdRaw, ...tail] : tail] };
  }
  if (first === "schema") {
    return { command: "schema", rest: secondRaw ? [secondRaw, ...thirdRaw ? [thirdRaw, ...tail] : tail] : [] };
  }
  if (second) {
    const route = COMMAND_ROUTES[`${first} ${second}`];
    if (route) return { command: route.command, rest: [...route.prepend ?? [], ...thirdRaw ? [thirdRaw, ...tail] : tail] };
  }
  if (SINGLE_COMMANDS.has(first)) {
    return { command: first, rest: secondRaw ? [secondRaw, ...thirdRaw ? [thirdRaw, ...tail] : tail] : [] };
  }
  return { command: UNKNOWN_COMMAND, rest: argv };
}
function packageSkillScriptPath(...segments) {
  const here = dirname6(fileURLToPath4(import.meta.url));
  const candidates = [
    join9(here, "..", "skills", "octocode-awareness", "scripts"),
    // dist/skills/ — bundled, preferred
    join9(here, "..", "..", "skills", "octocode-awareness", "scripts"),
    // <packageRoot>/skills/ — source fallback
    here
    // dist/bin/ — last resort
  ];
  const scriptsDir = candidates.find(
    (candidate) => existsSync6(join9(candidate, "schema.mjs")) || existsSync6(join9(candidate, "hooks"))
  ) ?? candidates[0];
  return join9(scriptsDir, ...segments);
}
function valuesFor(args2, key) {
  const value = args2[key];
  if (value === void 0 || value === false) return [];
  return Array.isArray(value) ? value.map(String) : [String(value)];
}
function firstValue(args2, key) {
  return valuesFor(args2, key)[0];
}
function flagBool(value, fallback) {
  if (value === void 0) return fallback;
  if (value === false) return false;
  if (value === true) return true;
  const normalized = String(value).trim().toLowerCase();
  if (normalized === "false" || normalized === "0" || normalized === "no") return false;
  if (normalized === "true" || normalized === "1" || normalized === "yes") return true;
  return Boolean(value);
}
function emit(payload, exitCode2 = 0, opts2 = {}) {
  payload["ok"] = payload["ok"] ?? exitCode2 === 0;
  const compact2 = opts2.compact === true || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
  process.stdout.write((compact2 ? JSON.stringify(payload) : JSON.stringify(payload, null, 2)) + "\n");
  return exitCode2;
}
function die(message, extras = {}) {
  const compact2 = process.argv.includes("--compact") || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
  process.stdout.write(JSON.stringify({ ok: false, error: message, ...extras }, null, compact2 ? 0 : 2) + "\n");
  process.exit(1);
}
function cmdTellMemory(db3, args2, dbPath2, opts2) {
  const agentId2 = String(args2["agent_id"] ?? "agent");
  const taskContext = String(args2["task_context"] ?? "");
  const observation = String(args2["observation"] ?? "");
  const importanceLevel = args2["importance"];
  if (!taskContext) die("--task-context is required");
  if (!observation) die("--observation is required");
  const imp = parseInt(String(importanceLevel), 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die("--importance must be 1\u201310");
  const rawTag = args2["tag"];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawRef = args2["reference"];
  const references = Array.isArray(rawRef) ? rawRef : rawRef ? [String(rawRef)] : [];
  const rawFile = args2["file"];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];
  const workspaceForFiles = args2["workspace"] ? String(args2["workspace"]) : void 0;
  const fileReferences = files.map((file) => {
    const trimmed = file.trim();
    if (!trimmed) return null;
    if (trimmed.startsWith("file:")) return trimmed;
    const normalized = normalizeFilePath(trimmed, workspaceForFiles);
    return normalized ? `file:${normalized}` : null;
  }).filter((file) => Boolean(file));
  const rawSup = args2["supersedes"];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args2["label"];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? "");
  const { memory, superseded, noveltyScore, similarMemoryIds } = insertMemory(db3, {
    agentId: agentId2,
    taskContext,
    observation,
    importance: imp,
    label,
    tags,
    references: [...references, ...fileReferences],
    supersedes,
    failureSignature: args2["failure_signature"] ? String(args2["failure_signature"]) : null,
    validFrom: args2["valid_from"] ? String(args2["valid_from"]) : null,
    validTo: args2["valid_to"] ? String(args2["valid_to"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    fileTreeFingerprint: args2["file_tree_fingerprint"] ? String(args2["file_tree_fingerprint"]) : null
  });
  const payload = { db_path: dbPath2, memory, superseded };
  if (supersedes.length === 0 && noveltyScore < 0.5 && similarMemoryIds.length > 0) {
    payload["consolidation"] = {
      novelty_score: noveltyScore,
      similar_memory_ids: similarMemoryIds,
      hint: "low novelty \u2014 review the similar memories; re-record with --supersedes <id> to replace one, or forget this one if redundant"
    };
  }
  const embedCmd = resolveEmbedCommand();
  if (embedCmd) {
    try {
      const text = `${taskContext}
${observation}`.trim();
      const { embedding, model } = runHostEmbedder(text, { command: embedCmd });
      storeEmbedding(db3, memory.memory_id, embedding, model);
      payload["embedding"] = { stored: true, model, dims: embedding.length };
    } catch (err) {
      payload["embedding"] = {
        stored: false,
        warning: err instanceof Error ? err.message : String(err)
      };
    }
  }
  return emit(payload, 0, opts2);
}
function cmdGetMemory(db3, args2, dbPath2, opts2) {
  const rawLabel = args2["label"];
  const labelArr = Array.isArray(rawLabel) ? rawLabel : rawLabel ? [String(rawLabel)] : void 0;
  const rawTag = args2["tag"];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawState = args2["state"];
  const states = rawState ? Array.isArray(rawState) ? rawState : [String(rawState)] : void 0;
  const rawReference = args2["reference"];
  const references = Array.isArray(rawReference) ? rawReference : rawReference ? [String(rawReference)] : [];
  const rawRegex = args2["regex"];
  const regex = Array.isArray(rawRegex) ? rawRegex : rawRegex ? [String(rawRegex)] : [];
  const rawFileRegex = args2["file_regex"];
  const fileRegex = Array.isArray(rawFileRegex) ? rawFileRegex : rawFileRegex ? [String(rawFileRegex)] : [];
  const rawGetFiles = args2["file"];
  const getFiles = Array.isArray(rawGetFiles) ? rawGetFiles : rawGetFiles ? [String(rawGetFiles)] : [];
  const sort = String(args2["sort"] ?? "smart");
  if (!MEMORY_SORTS.has(sort)) {
    die(`--sort must be one of: ${[...MEMORY_SORTS].join(", ")}`);
  }
  const result = getMemory(db3, {
    query: String(args2["query"] ?? ""),
    limit: parseInt(String(args2["limit"] ?? "3"), 10),
    minImportance: parseInt(String(args2["min_importance"] ?? "1"), 10),
    label: labelArr,
    tags,
    smart: args2["smart"] === true || args2["smart"] === "true",
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    states,
    sort,
    globalOnly: Boolean(args2["global_only"]),
    strictScope: Boolean(args2["strict_scope"]),
    asOf: args2["as_of"] ? String(args2["as_of"]) : null,
    references,
    regex,
    fileRegex,
    files: getFiles,
    explain: Boolean(args2["explain"])
  });
  const payload = { db_path: dbPath2, ...result };
  const fullMemory = Boolean(args2["full"]) || Boolean(args2["explain"]);
  if (!fullMemory && Array.isArray(payload["memories"])) {
    payload["memories"] = payload["memories"].map((memory) => projectMemoryLean(memory));
    payload["projection"] = "lean";
  }
  if (args2["semantic"]) {
    const embedCmd = resolveEmbedCommand();
    const queryText = String(args2["query"] ?? "").trim();
    if (!embedCmd) {
      payload["warnings"] = [
        "semantic ranking is unavailable in the CLI (set OCTOCODE_EMBED_CMD or use library storeEmbedding()/searchByEmbedding()); results use lexical FTS + decay."
      ];
    } else if (!queryText) {
      payload["warnings"] = [
        "semantic ranking skipped: --query is required when OCTOCODE_EMBED_CMD is set; results use lexical FTS + decay."
      ];
    } else {
      try {
        const { embedding, model } = runHostEmbedder(queryText, { command: embedCmd });
        const limit = parseInt(String(args2["limit"] ?? "3"), 10);
        const hits = searchByEmbedding(db3, embedding, Math.max(limit, 1), 0, model);
        if (hits.length === 0) {
          payload["warnings"] = [
            `OCTOCODE_EMBED_CMD ran (model=${model}) but no stored embeddings matched; results use lexical FTS + decay. Record memories while OCTOCODE_EMBED_CMD is set to populate vectors.`
          ];
        } else {
          const simById = new Map(hits.map((hit) => [hit.memory_id, hit.similarity]));
          const scoped = lexicalSearch(
            db3,
            "",
            hits.length,
            parseInt(String(args2["min_importance"] ?? "1"), 10),
            tags,
            labelArr ?? [],
            states ?? ["ACTIVE"],
            {
              workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
              artifact: args2["artifact"] ? String(args2["artifact"]) : null,
              repo: args2["repo"] ? String(args2["repo"]) : null,
              ref: args2["ref"] ? String(args2["ref"]) : null,
              strictScope: Boolean(args2["strict_scope"]),
              globalOnly: Boolean(args2["global_only"]),
              asOf: args2["as_of"] ? String(args2["as_of"]) : null,
              candidateMemoryIds: hits.map((hit) => hit.memory_id)
            }
          );
          const ranked = scoped.map((memory) => {
            const similarity = simById.get(memory.memory_id) ?? 0;
            memory.score = similarity;
            memory.lexical = similarity;
            return memory;
          }).sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
          if (ranked.length === 0) {
            payload["warnings"] = [
              `OCTOCODE_EMBED_CMD ran (model=${model}) and matched embeddings, but none passed the scope/label/importance filters; results use lexical FTS + decay.`
            ];
          } else {
            bumpAccess(db3, ranked.map((memory) => memory.memory_id));
            delete payload["judgment_required"];
            delete payload["judgment_reason"];
            const semanticMemories = ranked.slice(0, limit);
            payload["memories"] = fullMemory ? semanticMemories : semanticMemories.map((memory) => projectMemoryLean(memory));
            if (!fullMemory) payload["projection"] = "lean";
            payload["count"] = Math.min(ranked.length, limit);
            payload["mode"] = "semantic";
            payload["embedding_model"] = model;
          }
        }
      } catch (err) {
        payload["warnings"] = [
          `semantic ranking failed (${err instanceof Error ? err.message : String(err)}); results use lexical FTS + decay.`
        ];
      }
    }
  }
  return emit(payload, 0, opts2);
}
function cmdRefineSet(db3, args2, dbPath2, opts2) {
  const rawState = args2["state"];
  const stateVal = Array.isArray(rawState) ? rawState[0] : String(rawState ?? "open");
  const rawFile = args2["file"];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];
  const rawRefId = args2["refinement_id"];
  const refinementId = Array.isArray(rawRefId) ? rawRefId[0] : rawRefId ? String(rawRefId) : null;
  if (refinementId && refinementId !== "true") {
    const update = updateRefinement(db3, {
      refinementId,
      ...args2["state"] !== void 0 ? { state: stateVal } : {},
      ...args2["quality"] !== void 0 ? { quality: String(args2["quality"]) } : {},
      ...args2["reasoning"] !== void 0 ? { reasoning: String(args2["reasoning"]) } : {},
      ...args2["remember"] !== void 0 ? { remember: String(args2["remember"]) } : {},
      ...rawFile !== void 0 ? { files } : {}
    });
    if (!update.updated) die(`refinement not found: ${refinementId}`);
    return emit({ db_path: dbPath2, updated: true, refinement: update.refinement }, 0, opts2);
  }
  const reasoning = String(args2["reasoning"] ?? "");
  const remember = String(args2["remember"] ?? "");
  if (!reasoning) die("--reasoning is required");
  if (!remember) die("--remember is required");
  const { refinement } = insertRefinement(db3, {
    agentId: String(args2["agent_id"] ?? "agent"),
    reasoning,
    remember,
    quality: String(args2["quality"] ?? "good"),
    state: stateVal ?? "open",
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    files
  });
  return emit({ db_path: dbPath2, refinement }, 0, opts2);
}
function cmdRefineGet(db3, args2, dbPath2, opts2) {
  const rawState = args2["state"];
  const states = rawState ? Array.isArray(rawState) ? rawState : [String(rawState)] : void 0;
  const result = getRefinements(db3, {
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    quality: args2["quality"] ? String(args2["quality"]) : void 0,
    includeHandoffs: Boolean(args2["include_handoffs"]),
    states,
    limit: parseInt(String(args2["limit"] ?? "10"), 10)
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdReflect(db3, args2, dbPath2, opts2) {
  if (!args2["task"]) die("--task is required");
  let evalFailures = [];
  if (args2["eval_failure_json"]) {
    try {
      const parsed = JSON.parse(String(args2["eval_failure_json"]));
      if (!Array.isArray(parsed)) throw new Error("expected a JSON array");
      evalFailures = parsed;
    } catch (err) {
      die(`--eval-failure-json must be a JSON array of {id, dimension?, failure_signature?, suggested_lesson?}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
  const result = reflect(db3, {
    agentId: String(args2["agent_id"] ?? "agent"),
    task: String(args2["task"]),
    outcome: args2["outcome"] != null ? String(args2["outcome"]) : "partial",
    lesson: args2["lesson"] ? String(args2["lesson"]) : null,
    worked: args2["worked"] ? String(args2["worked"]) : null,
    didntWork: args2["didnt_work"] ? String(args2["didnt_work"]) : null,
    fixRepo: args2["fix_repo"] ? String(args2["fix_repo"]) : null,
    fixHarness: args2["fix_harness"] ? String(args2["fix_harness"]) : null,
    fixInstructions: args2["fix_instructions"] ? String(args2["fix_instructions"]) : null,
    failureSignature: args2["failure_signature"] ? String(args2["failure_signature"]) : null,
    importance: args2["importance"] ? parseInt(String(args2["importance"]), 10) : null,
    judgmentNote: args2["judgment_note"] ? String(args2["judgment_note"]) : null,
    duo: Boolean(args2["duo"]),
    evalFailures,
    files: Array.isArray(args2["fix_file"]) ? args2["fix_file"] : [],
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null
  });
  return emit({ ...result, db_path: dbPath2 }, 0, opts2);
}
function cmdPreFlightIntent(db3, args2, dbPath2, opts2) {
  const envAgentId = process.env.OCTOCODE_AGENT_ID?.trim() || "";
  const argAgentId = args2["agent_id"] ? String(args2["agent_id"]).trim() : "";
  const strictAgentId = Boolean(args2["strict_agent_id"]) || process.env.OCTOCODE_STRICT_AGENT_ID === "1";
  if (!argAgentId && !envAgentId) {
    const msg = "lock acquire: set --agent-id or OCTOCODE_AGENT_ID so CLI and hooks share one identity";
    if (strictAgentId) die(msg);
    console.error(`octocode-awareness: warning: ${msg}`);
  }
  const rawTarget = args2["target_file"] ?? args2["file"];
  const targetFiles = Array.isArray(rawTarget) ? rawTarget : rawTarget ? [String(rawTarget)] : [];
  if (targetFiles.length === 0) die("lock acquire requires at least one --target-file");
  const ttlMinutes = args2["ttl_minutes"] ? parseInt(String(args2["ttl_minutes"]), 10) : null;
  const ttlSeconds = args2["ttl_seconds"] ? parseInt(String(args2["ttl_seconds"]), 10) : null;
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1)) die("--ttl-minutes must be >= 1");
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1)) die("--ttl-seconds must be >= 1");
  if (ttlMinutes != null && ttlMinutes > 10) die("--ttl-minutes must be <= 10");
  if (ttlSeconds != null && ttlSeconds > MAX_CLI_TTL_SECONDS) die("--ttl-seconds must be <= 600");
  const ttlMs = ttlSeconds != null ? ttlSeconds * 1e3 : ttlMinutes != null ? ttlMinutes * 6e4 : null;
  const claimParams = {
    agentId: argAgentId || envAgentId || "agent",
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    runId: firstValue(args2, "run_id") ?? null,
    rationale: String(args2["rationale"] ?? "agent write operation"),
    testPlan: String(args2["test_plan"] ?? "post-edit verification"),
    contextRef: args2["context_ref"] ? String(args2["context_ref"]) : null,
    targetFiles,
    ttlMs
  };
  let result = preFlightIntent(db3, claimParams);
  const waitSeconds = parseBoundedSeconds(args2, "wait_seconds", 0, MAX_CLI_WAIT_SECONDS);
  const retrySeconds = parseBoundedSeconds(args2, "retry_interval", 1, MAX_CLI_RETRY_INTERVAL_SECONDS);
  if (!result.ok && waitSeconds != null && waitSeconds > 0) {
    const wait = waitForLock(db3, {
      agent_id: claimParams.agentId,
      target_files: targetFiles,
      workspace: claimParams.workspacePath ?? void 0,
      artifact: claimParams.artifact ?? void 0,
      lock_type: "EXCLUSIVE",
      wait_ms: waitSeconds * 1e3,
      retry_interval_ms: retrySeconds != null ? retrySeconds * 1e3 : void 0
    });
    if (wait.lock_free) result = preFlightIntent(db3, claimParams);
  }
  if (!result.ok) return emit({ db_path: dbPath2, ...result }, 2, opts2);
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdAuditUnverified(db3, args2, dbPath2, opts2) {
  const rawAuditWs = args2["workspace"] ? String(args2["workspace"]) : null;
  const result = auditUnverified(db3, {
    agentId: args2["agent_id"] ? String(args2["agent_id"]) : null,
    workspacePath: rawAuditWs ? normalizeWorkspacePath(rawAuditWs, rawAuditWs) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    abandon: Boolean(args2["abandon"])
  });
  if (opts2.compact) {
    const limit = 3;
    const compactRun = (run) => {
      const allFiles = Array.isArray(run["target_files"]) ? run["target_files"].map(String) : [];
      return {
        run_id: run["run_id"],
        agent_id: run["agent_id"],
        status: run["status"],
        test_plan: run["test_plan"],
        target_files: allFiles.slice(0, 1),
        file_count: allFiles.length,
        ...run["age_hours"] == null ? {} : { age_hours: run["age_hours"] }
      };
    };
    const unverified = result.unverified.slice(0, limit).map((run) => compactRun(run));
    const staleActive = result.stale_active.slice(0, limit).map((run) => compactRun(run));
    return emit({
      db_path: dbPath2,
      ok: true,
      count: result.count,
      unverified_count: result.unverified.length,
      stale_active_count: result.stale_active.length,
      unverified,
      stale_active: staleActive,
      omitted_count: result.count - unverified.length - staleActive.length
    }, result.count > 0 ? 1 : 0, opts2);
  }
  return emit({ db_path: dbPath2, ...result }, result.count > 0 ? 1 : 0, opts2);
}
function cmdVerify(db3, args2, dbPath2, opts2) {
  const allPending = Boolean(args2["all_pending"]);
  const runIds = valuesFor(args2, "run_id");
  if (!allPending && runIds.length === 0) {
    return emit({ error: "--run-id is required (or use --all-pending)" }, 1, opts2);
  }
  const statusArg = args2["status"] ? String(args2["status"]) : "SUCCESS";
  if (statusArg !== "SUCCESS" && statusArg !== "FAILED") {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts2);
  }
  if (!allPending && runIds.length > 1) {
    const results = runIds.map((runId) => markVerified(db3, {
      runId,
      agentId: String(args2["agent_id"] ?? "agent"),
      workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
      artifact: args2["artifact"] ? String(args2["artifact"]) : null,
      message: args2["message"] ? String(args2["message"]) : void 0,
      status: statusArg
    }));
    const failed = results.find((result2) => !result2.ok);
    if (failed && !failed.ok) {
      return emit({ db_path: dbPath2, ok: false, error: failed.error, run_id: null, run_ids: runIds, results }, 1, opts2);
    }
    return emit({
      db_path: dbPath2,
      run_id: null,
      run_ids: runIds,
      count: results.length,
      status: statusArg,
      results
    }, 0, opts2);
  }
  const result = markVerified(db3, {
    runId: runIds[0],
    agentId: String(args2["agent_id"] ?? "agent"),
    allPending,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    message: args2["message"] ? String(args2["message"]) : void 0,
    status: statusArg
  });
  return emit({ db_path: dbPath2, ...result }, result.ok ? 0 : 1, opts2);
}
function cmdReleaseFileLock(db3, args2, dbPath2, opts2) {
  const rawTarget = args2["target_file"] ?? args2["file"];
  const targetFiles = rawTarget ? Array.isArray(rawTarget) ? rawTarget : [String(rawTarget)] : [];
  const runId = firstValue(args2, "run_id");
  if (!runId && targetFiles.length === 0) {
    return emit({ error: "lock release requires --run-id or --target-file" }, 1, opts2);
  }
  const releaseStatus = String(args2["status"] ?? "SUCCESS").toUpperCase();
  if (!["PENDING", "SUCCESS", "FAILED"].includes(releaseStatus)) {
    die("--status must be PENDING, SUCCESS, or FAILED");
  }
  const result = releaseFileLock(db3, {
    agentId: String(args2["agent_id"] ?? "agent"),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    runId: runId ?? null,
    targetFiles,
    status: releaseStatus,
    verified: Boolean(args2["verified"]),
    verifiedNote: args2["verified_note"] ? String(args2["verified_note"]) : void 0
  });
  if (!result.released) {
    return emit({ db_path: dbPath2, ...result, ok: false }, result.ambiguousRelease ? 2 : 1, opts2);
  }
  if ("unverifiedConclusion" in result) {
    return emit({ db_path: dbPath2, ...result, ok: false }, 2, opts2);
  }
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function requiredArg(args2, key) {
  const value = args2[key];
  if (value == null || value === true || !String(value).trim()) {
    die(`--${key.replace(/_/g, "-")} is required`);
  }
  return String(value).trim();
}
function cmdPlan(db3, args2, dbPath2, opts2) {
  const action = requiredArg(args2, "action");
  if (action === "create") {
    const result = createPlan(db3, {
      name: requiredArg(args2, "name"),
      objective: requiredArg(args2, "objective"),
      leadAgentId: String(args2["lead_agent_id"] ?? args2["agent_id"] ?? process.env.OCTOCODE_AGENT_ID ?? "").trim(),
      workspacePath: String(args2["workspace"] ?? process.cwd()),
      artifact: args2["artifact"] ? String(args2["artifact"]) : null
    });
    return emit({ db_path: dbPath2, ...result }, 0, opts2);
  }
  if (action === "list") {
    const plans = listPlans(db3, {
      workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
      artifact: args2["artifact"] ? String(args2["artifact"]) : null,
      status: args2["status"] ? String(args2["status"]).toUpperCase() : null
    });
    return emit({ db_path: dbPath2, count: plans.length, plans }, 0, opts2);
  }
  const planId = requiredArg(args2, "plan_id");
  if (action === "show") {
    const plan = getPlan(db3, planId);
    return plan ? emit({ db_path: dbPath2, plan }, 0, opts2) : emit({ db_path: dbPath2, error: `plan not found: ${planId}` }, 1, opts2);
  }
  if (action === "join") {
    const member = joinPlan(db3, { planId, agentId: String(args2["agent_id"] ?? process.env.OCTOCODE_AGENT_ID ?? "").trim() });
    return emit({ db_path: dbPath2, plan_id: planId, member }, 0, opts2);
  }
  if (action === "doc") {
    const document = registerPlanDocument(db3, {
      planId,
      agentId: String(args2["agent_id"] ?? process.env.OCTOCODE_AGENT_ID ?? "").trim(),
      relativePath: valuesFor(args2, "path")[0] ?? "",
      title: requiredArg(args2, "title")
    });
    return emit({ db_path: dbPath2, plan_id: planId, document }, 0, opts2);
  }
  if (action === "status") {
    const status = requiredArg(args2, "status").toUpperCase();
    if (!["DRAFT", "ACTIVE", "PAUSED", "COMPLETED", "CANCELLED"].includes(status)) {
      die("--status must be DRAFT, ACTIVE, PAUSED, COMPLETED, or CANCELLED");
    }
    const plan = updatePlanStatus(db3, {
      planId,
      status,
      agentId: String(args2["agent_id"] ?? process.env.OCTOCODE_AGENT_ID ?? "").trim()
    });
    return emit({ db_path: dbPath2, plan }, 0, opts2);
  }
  return emit({ db_path: dbPath2, error: `unknown plan action: ${action}` }, 1, opts2);
}
function cmdTask(db3, args2, dbPath2, opts2) {
  const action = requiredArg(args2, "action");
  const agentId2 = String(args2["agent_id"] ?? args2["created_by"] ?? process.env.OCTOCODE_AGENT_ID ?? "").trim();
  if (action === "create") {
    const result = createTask(db3, {
      planId: requiredArg(args2, "plan_id"),
      title: requiredArg(args2, "title"),
      reasoning: requiredArg(args2, "reasoning"),
      acceptanceCriteria: args2["acceptance"] ? String(args2["acceptance"]) : void 0,
      paths: valuesFor(args2, "path"),
      createdBy: agentId2,
      priority: args2["priority"] == null ? void 0 : Number(args2["priority"]),
      dependsOn: valuesFor(args2, "depends_on")
    });
    return emit({ db_path: dbPath2, ...result }, 0, opts2);
  }
  if (action === "list" || action === "ready") {
    const tasks = action === "ready" ? listReadyTasks(db3, { planId: args2["plan_id"] ? String(args2["plan_id"]) : null }) : listTasks(db3, {
      planId: args2["plan_id"] ? String(args2["plan_id"]) : null,
      status: args2["status"] ? String(args2["status"]).toUpperCase() : null,
      agentId: args2["agent_id"] ? agentId2 : null
    });
    return emit({ db_path: dbPath2, count: tasks.length, tasks }, 0, opts2);
  }
  let taskId = args2["task_id"] ? String(args2["task_id"]) : "";
  if (action === "claim" && Boolean(args2["next"])) {
    const planId = requiredArg(args2, "plan_id");
    taskId = listReadyTasks(db3, { planId })[0]?.task_id ?? "";
    if (!taskId) return emit({ db_path: dbPath2, error: `no ready tasks in plan ${planId}` }, 1, opts2);
  }
  if (!taskId) die("--task-id is required");
  if (action === "show") {
    const task = getTask(db3, taskId);
    return task ? emit({ db_path: dbPath2, task }, 0, opts2) : emit({ db_path: dbPath2, error: `task not found: ${taskId}` }, 1, opts2);
  }
  if (action === "depend") {
    const dependencies = valuesFor(args2, "depends_on");
    if (dependencies.length === 0) die("task depend requires at least one --depends-on");
    for (const dependsOnTaskId of dependencies) {
      addTaskDependency(db3, { taskId, dependsOnTaskId, agentId: agentId2 });
    }
    return emit({ db_path: dbPath2, task: getTask(db3, taskId) }, 0, opts2);
  }
  const leaseMinutes = args2["lease_minutes"] == null ? void 0 : Number(args2["lease_minutes"]);
  if (leaseMinutes != null && (leaseMinutes < 1 || leaseMinutes > 60)) die("--lease-minutes must be between 1 and 60");
  if (action === "claim") {
    const result = claimTask(db3, {
      taskId,
      agentId: agentId2,
      leaseMs: leaseMinutes == null ? void 0 : leaseMinutes * 6e4,
      testPlan: args2["test_plan"] ? String(args2["test_plan"]) : void 0
    });
    return emit({ db_path: dbPath2, ...result }, result.ok ? 0 : 2, opts2);
  }
  const runId = firstValue(args2, "run_id") ?? "";
  if (!runId) die("--run-id is required");
  if (action === "heartbeat") {
    const claim = heartbeatTaskClaim(db3, {
      taskId,
      runId,
      agentId: agentId2,
      leaseMs: leaseMinutes == null ? void 0 : leaseMinutes * 6e4
    });
    return emit({ db_path: dbPath2, claim }, 0, opts2);
  }
  if (action === "submit") {
    const result = submitTask(db3, {
      taskId,
      runId,
      agentId: agentId2,
      message: args2["message"] ? String(args2["message"]) : void 0
    });
    return emit({ db_path: dbPath2, ...result }, 0, opts2);
  }
  if (action === "release") {
    const task = releaseTaskClaim(db3, {
      taskId,
      runId,
      agentId: agentId2,
      blockedReason: args2["blocked_reason"] ? String(args2["blocked_reason"]) : null
    });
    return emit({ db_path: dbPath2, task }, 0, opts2);
  }
  return emit({ db_path: dbPath2, error: `unknown task action: ${action}` }, 1, opts2);
}
function cmdWork(db3, args2, dbPath2, opts2) {
  const action = requiredArg(args2, "action");
  const agentId2 = String(args2["agent_id"] ?? process.env.OCTOCODE_AGENT_ID ?? "").trim();
  const runId = firstValue(args2, "run_id") ?? "";
  const rawFiles = args2["target_file"] ?? args2["file"];
  const targetFiles = Array.isArray(rawFiles) ? rawFiles.map(String) : rawFiles ? [String(rawFiles)] : [];
  const ttlMinutes = args2["ttl_minutes"] == null ? null : Number(args2["ttl_minutes"]);
  const ttlSeconds = args2["ttl_seconds"] == null ? null : Number(args2["ttl_seconds"]);
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1 || ttlMinutes > 60)) {
    die("--ttl-minutes must be between 1 and 60");
  }
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1 || ttlSeconds > 3600)) {
    die("--ttl-seconds must be between 1 and 3600");
  }
  const ttlMs = ttlSeconds != null ? ttlSeconds * 1e3 : ttlMinutes != null ? ttlMinutes * 6e4 : null;
  const compactMutation = (result) => {
    if (!opts2.compact) return result;
    const files = result.files.slice(0, 3).map((file) => file.file_path);
    const peers = result.peers.slice(0, 3).map((peer) => ({
      run_id: peer.run_id,
      agent_id: peer.agent_id,
      file_path: peer.file_path,
      origin: peer.origin,
      exclusive: peer.exclusive
    }));
    return {
      run: {
        run_id: result.run.run_id,
        task_id: result.run.task_id,
        origin: result.run.origin,
        status: result.run.status
      },
      files,
      file_count: result.files.length,
      omitted_file_count: Math.max(0, result.files.length - files.length),
      peer_count: result.peer_count,
      peers,
      omitted_peer_count: Math.max(0, result.peer_count - peers.length)
    };
  };
  const presentList = (result, limit) => {
    if (Boolean(args2["full"])) return result;
    const files = result.files.slice(0, limit).map((file) => ({
      file_path: file.file_path,
      run_id: file.run_id,
      task_id: file.task_id,
      origin: file.origin,
      agent_id: file.agent_id,
      exclusive: file.exclusive,
      reason: file.reason_override ?? file.rationale,
      heartbeat_at: file.heartbeat_at,
      expires_at: file.expires_at
    }));
    return { count: result.count, files, omitted_count: Math.max(0, result.count - files.length) };
  };
  if (action === "start") {
    if (!agentId2) die("work start requires --agent-id or OCTOCODE_AGENT_ID");
    if (targetFiles.length === 0) die("work start requires at least one --file");
    if (!runId && !args2["rationale"]) die("new WORK runs require --rationale");
    if (!runId && !args2["test_plan"]) die("new WORK runs require --test-plan");
    const result = startWork(db3, {
      agentId: agentId2,
      sessionId: args2["session_id"] ? String(args2["session_id"]) : null,
      workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
      artifact: args2["artifact"] ? String(args2["artifact"]) : null,
      runId: runId || null,
      targetFiles,
      rationale: args2["rationale"] ? String(args2["rationale"]) : void 0,
      testPlan: args2["test_plan"] ? String(args2["test_plan"]) : void 0,
      contextRef: args2["context_ref"] ? String(args2["context_ref"]) : null,
      origin: "WORK",
      source: "EXPLICIT",
      ttlMs,
      exclusive: Boolean(args2["exclusive"])
    });
    if (!result.ok) {
      const conflicts = opts2.compact ? result.conflicts.slice(0, 3) : result.conflicts;
      return emit({
        db_path: dbPath2,
        ok: false,
        conflict: true,
        conflicts,
        conflict_count: result.conflicts.length,
        omitted_conflict_count: result.conflicts.length - conflicts.length
      }, 2, opts2);
    }
    return emit({ db_path: dbPath2, ...compactMutation(result) }, 0, opts2);
  }
  if (action === "touch") {
    if (!agentId2) die("work touch requires --agent-id or OCTOCODE_AGENT_ID");
    if (!runId) die("work touch requires --run-id");
    const result = touchWork(db3, { agentId: agentId2, runId, targetFiles, ttlMs });
    return emit({ db_path: dbPath2, ...compactMutation(result) }, 0, opts2);
  }
  if (action === "end") {
    if (!agentId2) die("work end requires --agent-id or OCTOCODE_AGENT_ID");
    if (!runId) die("work end requires --run-id");
    const result = endWork(db3, { agentId: agentId2, runId, targetFiles });
    return emit({ db_path: dbPath2, ...compactMutation(result) }, 0, opts2);
  }
  if (action === "list") {
    const result = listWork(db3, {
      workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
      artifact: args2["artifact"] ? String(args2["artifact"]) : null,
      agentId: agentId2 || null,
      runId: runId || null,
      filePath: targetFiles[0] ?? null,
      activeOnly: !Boolean(args2["all"])
    });
    return emit({ db_path: dbPath2, ...presentList(result, opts2.compact ? 10 : 20) }, 0, opts2);
  }
  if (action === "show") {
    if (targetFiles.length !== 1) die("work show requires exactly one --file");
    const result = showWork(db3, {
      workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
      artifact: args2["artifact"] ? String(args2["artifact"]) : null,
      filePath: targetFiles[0],
      activeOnly: !Boolean(args2["all"])
    });
    return emit({ db_path: dbPath2, ...presentList(result, opts2.compact ? 5 : 10) }, 0, opts2);
  }
  return emit({ db_path: dbPath2, error: `unknown work action: ${action}` }, 1, opts2);
}
function cmdForget(db3, args2, dbPath2, opts2) {
  const rawIds = args2["memory_id"];
  const memoryIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const rawTags = [args2["tag"], args2["tags"]].flatMap((v) => Array.isArray(v) ? v : v && v !== true ? [String(v)] : []);
  const tags = rawTags;
  const result = forgetMemory(db3, {
    memoryIds,
    tags,
    before: args2["before"] ? String(args2["before"]) : void 0,
    maxImportance: args2["max_importance"] ? parseInt(String(args2["max_importance"]), 10) : void 0,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    dryRun: Boolean(args2["dry_run"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdRefineDelete(db3, args2, dbPath2, opts2) {
  const rawIds = args2["refinement_id"];
  const refinementIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  if (refinementIds.length === 0) return emit({ error: "--refinement-id is required" }, 1, opts2);
  const result = deleteRefinement(db3, {
    refinementIds,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : void 0,
    artifact: args2["artifact"] ? String(args2["artifact"]) : void 0,
    dryRun: Boolean(args2["dry_run"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdExportHarness(db3, args2, dbPath2, opts2) {
  const result = exportHarness(db3, {
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    min_importance: args2["min_importance"] ? parseInt(String(args2["min_importance"]), 10) : void 0,
    workspace_path: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdDeveloperReview(db3, args2, dbPath2, opts2) {
  const format = String(args2["format"] ?? "json").toLowerCase();
  const result = developerReviewDoc(db3, {
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : process.cwd(),
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    query: args2["query"] ? String(args2["query"]) : null,
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    state: Array.isArray(args2["state"]) ? args2["state"].map(String) : args2["state"] ? String(args2["state"]) : null
  });
  if (format === "markdown") {
    process.stdout.write(result.markdown);
    return 0;
  }
  return emit({
    db_path: dbPath2,
    view: "developer-review",
    open: result.open,
    resolved: result.resolved,
    count: result.rows.length,
    rows: result.rows
  }, 0, opts2);
}
function cmdQuery(db3, args2, dbPath2, opts2) {
  const view = String(args2["view"] ?? args2._[0] ?? "all");
  const format = String(args2["format"] ?? "json").toLowerCase();
  const workspacePath = args2["workspace"] ? String(args2["workspace"]) : process.cwd();
  const result = queryAwareness(db3, {
    view,
    workspacePath,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    query: args2["query"] ? String(args2["query"]) : null,
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    agentId: args2["agent_id"] ? String(args2["agent_id"]) : null,
    state: Array.isArray(args2["state"]) ? args2["state"].map(String) : args2["state"] ? String(args2["state"]) : null,
    label: Array.isArray(args2["label"]) ? args2["label"].map(String) : args2["label"] ? String(args2["label"]) : null,
    file: args2["file"] ? String(Array.isArray(args2["file"]) ? args2["file"][0] : args2["file"]) : null,
    since: args2["since"] ? String(args2["since"]) : null,
    includeBodies: flagBool(args2["include_bodies"])
  });
  const outPath = args2["out"] ? String(args2["out"]) : null;
  if (outPath) {
    const resolvedOutPath = isAbsolute8(outPath) ? resolve15(outPath) : resolve15(workspacePath, outPath);
    mkdirSync6(dirname6(resolvedOutPath), { recursive: true });
    writeFileSync5(resolvedOutPath, formatAwarenessQueryResult(result, format), "utf8");
    return emit({ db_path: dbPath2, path: resolvedOutPath, view: result.view, count: result.count }, 0, opts2);
  }
  if (format === "json") return emit({ db_path: dbPath2, ...result }, 0, opts2);
  process.stdout.write(formatAwarenessQueryResult(result, format));
  return 0;
}
function cmdAttend(db3, args2, dbPath2, opts2) {
  const rawFile = args2["file"];
  const files = Array.isArray(rawFile) ? rawFile.map(String) : rawFile ? [String(rawFile)] : [];
  const result = attendAwareness(db3, {
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : process.cwd(),
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    query: args2["query"] ? String(args2["query"]) : null,
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    file: files,
    includeBodies: flagBool(args2["include_bodies"]),
    explainOrgan: flagBool(args2["explain_organ"]),
    compact: opts2.compact
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdRepoInject(db3, args2, dbPath2, opts2) {
  const outDir = args2["out_dir"] ?? args2["out"];
  const result = injectRepoContext(db3, {
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : process.cwd(),
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    query: args2["query"] ? String(args2["query"]) : null,
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    outDir: outDir ? String(outDir) : void 0,
    mode: args2["mode"] ? String(args2["mode"]) : void 0,
    includeView: flagBool(args2["include_view"]),
    check: flagBool(args2["check"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdDocsCatalog(_db2, args2, _dbPath, opts2) {
  const action = String(args2["action"] ?? args2._[0] ?? "list").trim().toLowerCase();
  if (action === "list") {
    const full = Boolean(args2["full"]);
    const result = listSkillDocs({ lean: !full });
    return emit({
      ok: true,
      count: result.count,
      ...full ? { root: result.root } : {},
      docs: result.docs,
      next: "octocode-awareness docs show <name> --compact"
    }, 0, opts2);
  }
  if (action === "show") {
    const name = String(args2["name"] ?? args2._[0] ?? "").trim();
    if (!name) return emit({ ok: false, error: "docs show requires a name. Run docs list --compact." }, 1, opts2);
    const result = showSkillDoc(name);
    if (!result.ok) {
      return emit({ ok: false, error: result.error, suggestions: result.suggestions }, 1, opts2);
    }
    if (opts2.compact || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1") {
      return emit({
        ok: true,
        name: result.name,
        title: result.title,
        description: result.description,
        kind: result.kind,
        path: result.path,
        content: result.content
      }, 0, opts2);
    }
    process.stdout.write(`${result.content}${result.content.endsWith("\n") ? "" : "\n"}`);
    return 0;
  }
  return emit({ ok: false, error: `unknown docs action "${action}". Use docs list|show|staleness.` }, 1, opts2);
}
function cmdDocStaleness(db3, args2, dbPath2, opts2) {
  const rawTargets = args2["targets_json"];
  if (!rawTargets || typeof rawTargets !== "string") {
    return emit({ error: `--targets-json is required, e.g. '[{"docFile":"pkg/ARCHITECTURE.md","sourceDirs":["pkg/src"]}]'` }, 1, opts2);
  }
  let targets;
  try {
    const parsed = JSON.parse(rawTargets);
    if (!Array.isArray(parsed)) throw new Error("not an array");
    targets = parsed.map((t) => {
      const obj = t;
      const docFile = String(obj.docFile ?? obj.doc_file ?? "");
      const rawDirs = obj.sourceDirs ?? obj.source_dirs;
      const sourceDirs = Array.isArray(rawDirs) ? rawDirs.map(String) : [];
      if (!docFile || sourceDirs.length === 0) throw new Error("each target needs docFile and sourceDirs");
      return { docFile, sourceDirs };
    });
  } catch (err) {
    return emit({ error: `--targets-json is invalid: ${err.message}` }, 1, opts2);
  }
  const workspacePath = args2["workspace"] ? String(args2["workspace"]) : null;
  const artifact2 = args2["artifact"] ? String(args2["artifact"]) : null;
  const result = mineDocStaleness(db3, {
    targets,
    workspacePath,
    artifact: artifact2,
    minEditsSinceSync: args2["min_edits"] ? Number(args2["min_edits"]) : void 0,
    minLinesSinceSync: args2["min_lines"] ? Number(args2["min_lines"]) : void 0
  });
  const proposed = [];
  if (Boolean(args2["propose"])) {
    const agentId2 = String(args2["agent_id"] ?? "agent");
    const sessionId2 = args2["session_id"] ? String(args2["session_id"]) : null;
    for (const entry2 of result.entries) {
      if (!entry2.stale) continue;
      const harnessId = proposeDocRefresh(db3, entry2, { agentId: agentId2, sessionId: sessionId2, workspacePath, artifact: artifact2 });
      proposed.push({ target_file: entry2.doc_file, harness_id: harnessId });
    }
  }
  return emit({ db_path: dbPath2, ...result, proposed }, 0, opts2);
}
function cmdAgentSignal(db3, args2, dbPath2, opts2) {
  if (!args2["agent_id"]) return emit({ error: "--agent-id is required" }, 1, opts2);
  const action = String(args2["action"] ?? "");
  if (!["publish", "list", "reply", "resolve", "ack"].includes(action)) {
    return emit({ error: "--action must be publish, list, reply, resolve, or ack" }, 1, opts2);
  }
  const rawTo = args2["to_agent"] ?? args2["to"];
  const toAgents = Array.isArray(rawTo) ? rawTo : rawTo ? [String(rawTo)] : [];
  const rawFiles = args2["file"];
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [String(rawFiles)] : [];
  const rawRefs = args2["ref_id"];
  const refs = Array.isArray(rawRefs) ? rawRefs : rawRefs ? [String(rawRefs)] : [];
  const rawKinds = args2["kind"];
  const kinds = Array.isArray(rawKinds) ? rawKinds : rawKinds ? [String(rawKinds)] : [];
  const publishKind = kinds[0] ? normalizeNotificationKind(kinds[0]) : void 0;
  const rawSignalIds = args2["signal_id"];
  const signalIds = Array.isArray(rawSignalIds) ? rawSignalIds : rawSignalIds ? [String(rawSignalIds)] : [];
  const includeBodies = Boolean(args2["include_bodies"]);
  const result = agentSignal(db3, {
    action,
    agentId: String(args2["agent_id"]),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    kind: publishKind,
    subject: args2["subject"] ? String(args2["subject"]) : void 0,
    body: args2["body"] ? String(args2["body"]) : null,
    toAgents,
    files,
    refs,
    importance: args2["importance"] ? parseInt(String(args2["importance"]), 10) : void 0,
    inReplyTo: args2["in_reply_to"] ? String(args2["in_reply_to"]) : null,
    threadId: args2["thread_id"] ? String(args2["thread_id"]) : null,
    signalIds,
    unreadOnly: args2["all"] ? false : args2["unread_only"],
    markRead: Boolean(args2["mark_read"]),
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    kinds: kinds.length ? kinds.map((k) => normalizeNotificationKind(k)) : []
  });
  if (action === "list" && !includeBodies && "signals" in result && Array.isArray(result.signals)) {
    return emit({
      db_path: dbPath2,
      ...result,
      signals: result.signals.map((signal) => ({
        ...signal,
        body: signal.body == null ? null : summarizeText(String(signal.body), 160)
      })),
      bodies: "summarized"
    }, 0, opts2);
  }
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdNotifyPrune(db3, args2, dbPath2, opts2) {
  const rawIds = args2["signal_id"];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = pruneNotifications(db3, {
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    notificationIds,
    resolvedOnly: Boolean(args2["resolved"]),
    olderThanDays: args2["older_than_days"] ? parseInt(String(args2["older_than_days"]), 10) : void 0,
    dryRun: Boolean(args2["dry_run"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdAgentRegistry(db3, args2, dbPath2, opts2) {
  const action = String(args2["action"] ?? "list");
  if (!["list", "register"].includes(action)) {
    return emit({ error: "--action must be list or register" }, 1, opts2);
  }
  const workspacePath = args2["workspace"] ? String(args2["workspace"]) : null;
  const artifact2 = args2["artifact"] ? String(args2["artifact"]) : null;
  if (action === "register") {
    if (!args2["agent_id"]) return emit({ error: "--agent-id is required for register" }, 1, opts2);
    const agent = registerAgent(db3, {
      agentId: String(args2["agent_id"]),
      agentName: args2["agent_name"] ? String(args2["agent_name"]) : "",
      workspacePath,
      artifact: artifact2,
      context: args2["context"] ? String(args2["context"]) : null
    });
    return emit({ db_path: dbPath2, action: "register", agent }, 0, opts2);
  }
  const limit = Math.min(200, Math.max(1, parseInt(String(args2["limit"] ?? "50"), 10) || 50));
  const result = listAgents(db3, { workspacePath, artifact: artifact2 });
  const agents = result.agents.slice(0, limit);
  return emit({
    db_path: dbPath2,
    action: "list",
    count: agents.length,
    total_count: result.count,
    agents,
    workspace_path: workspacePath,
    artifact: artifact2
  }, 0, opts2);
}
function cmdStatus(db3, dbPath2, args2, opts2) {
  const rawWsPath = args2["workspace"] ? String(args2["workspace"]) : null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact2 = args2["artifact"] ? String(args2["artifact"]) : null;
  const memScope = [];
  const memScopeBinds = [];
  if (wsPath) {
    memScope.push("(workspace_path = ? OR workspace_path IS NULL)");
    memScopeBinds.push(wsPath);
  }
  if (artifact2) {
    memScope.push("(artifact = ? OR artifact IS NULL)");
    memScopeBinds.push(artifact2);
  }
  const memWhere = memScope.length > 0 ? `WHERE ${memScope.join(" AND ")}` : "";
  const memCount = db3.prepare(`SELECT COUNT(*) AS count FROM memories ${memWhere}`).get(...memScopeBinds).count;
  const memStates = Object.fromEntries(
    db3.prepare(`SELECT state, COUNT(*) AS count FROM memories ${memWhere} GROUP BY state`).all(...memScopeBinds).map((r) => [r.state, r.count])
  );
  const memLabels = Object.fromEntries(
    db3.prepare(`SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM memories ${memWhere} GROUP BY label`).all(...memScopeBinds).map((r) => [r.label, r.count])
  );
  const limit = Math.min(100, Math.max(1, parseInt(String(args2["limit"] ?? "20"), 10) || 20));
  const status = getWorkspaceStatus(db3, { workspace_path: wsPath, artifact: artifact2 });
  return emit({
    db_path: dbPath2,
    fts_enabled: hasFts(db3),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    ...status,
    locks: status.locks.slice(0, limit),
    workspace_path: wsPath,
    artifact: artifact2
  }, 0, opts2);
}
function cmdInit(db3, dbPath2, opts2) {
  const memCount = db3.prepare("SELECT COUNT(*) AS count FROM memories").get().count;
  return emit({ db_path: dbPath2, initialized: true, memory_count: memCount }, 0, opts2);
}
function cmdSelfTest(opts2) {
  const testDb = new DatabaseSync2(":memory:");
  testDb.exec("PRAGMA foreign_keys = ON");
  initDb(testDb);
  const testAgent = "self-test-agent";
  const { memoryId } = insertMemory(testDb, {
    agentId: testAgent,
    taskContext: "self-test task",
    observation: "This is a smoke-test memory.",
    importance: 7,
    label: "GOTCHA",
    tags: ["smoke-test"]
  });
  const { memories: results } = getMemory(testDb, { query: "smoke-test", limit: 5 });
  if (results.length === 0) {
    return emit({ ok: false, error: "FTS recall returned no results" }, 1, opts2);
  }
  const reflectResult = reflect(testDb, {
    agentId: testAgent,
    task: "self-test",
    outcome: "worked",
    fixRepo: "test fix"
  });
  return emit({
    ok: true,
    db: ":memory:",
    fts_enabled: hasFts(testDb),
    memory_written: memoryId,
    memory_recalled: results[0].memory_id,
    reflection_memory: reflectResult.learning_memory_id,
    refinement_id: reflectResult.repo_fix_refinement_id,
    checks: {
      write: Boolean(memoryId),
      fts_recall: results.length > 0,
      scoring: typeof results[0].score === "number",
      refinement: Boolean(reflectResult.repo_fix_refinement_id)
    }
  }, 0, opts2);
}
var HELP = `usage: octocode-awareness <command> [options]
common: --db <path> --compact
local-first: use octocode-awareness or a bundled local node path when present
fallback: npx @octocodeai/octocode-awareness <command>
agent map: octocode-awareness schema commands --compact
schema: octocode-awareness schema commands|list|json-schema <name>|example <name>|validate <name> <json-file|->

easy install:
  If the CLI is bundled locally, tell your agent to run that local CLI:
    octocode-awareness maintenance init --compact
  Package fallback only when no local CLI exists:
    npx @octocodeai/octocode-awareness maintenance init --compact
  The Agent Skill is bundled with this package under dist/skills/:
    npx octocode skill --add --path <awareness-package>/dist/skills/octocode-awareness --platform common
    npx octocode skill --add --path <awareness-package>/dist/skills/octocode-skills --platform common
  Use npx octocode for skill management and research/search operations; do not install awareness by registry name.

supported agents: Codex, Claude Code, Cursor, Pi, and custom library/CLI hosts
surfaces: CLI = control plane; Agent Skill = operating loop; hooks/Pi bridge = lifecycle automation

start: attend, workspace status, plan list, task ready, memory recall, signal list, query <view>
planning: plan create|list|show|join|doc|status; task create|list|ready|show|claim|heartbeat|submit|release|depend
file work: work start|touch|end|list|show (advisory; declare every edited path)
exclusive: lock acquire, lock wait, lock release, lock prune (sensitive work only)
verification: verify mark, verify audit
messages: signal publish, signal list, signal reply, signal ack, signal resolve, signal prune, agent register, agent list
bookkeeping/learning: memory record, memory forget, refinement set, refinement get, refinement delete, reflect record, reflect mine-weakness, reflect export-harness, reflect developer-review, docs list, docs show, docs staleness
housekeeping: maintenance digest, lock prune, signal prune, memory forget (dry-run first), session capture
repo context: query files|workboard|all|developer-review [--format json|table|csv|markdown|html], repo inject
hooks: hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end>, hooks install|check|remove --host claude|codex|cursor
utility: maintenance init, maintenance self-test

examples:
  octocode-awareness workspace status --workspace "$PWD" --compact
  octocode-awareness attend --workspace "$PWD" --query "current task" --compact
  octocode-awareness task ready --plan-id plan_123 --compact
  octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact
  octocode-awareness docs list --compact
  octocode-awareness docs show architecture
  octocode-awareness work start --agent-id agent --file src/file.ts --rationale "edit" --test-plan "yarn test" --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness schema commands --compact
  octocode-awareness query files --workspace "$PWD" --format table --limit 50
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
  octocode-awareness repo inject --workspace "$PWD" --mode local --compact

Run "octocode-awareness <command> --help" for command flags. Exit 2 = lock conflict or wait timeout.`;
var HELP_COMPACT = `octocode-awareness: canonical noun/verb CLI. Use --compact for JSON.
local-first: octocode-awareness <command>; fallback: npx @octocodeai/octocode-awareness <command>; bundled skill path: <awareness-package>/dist/skills/octocode-awareness; Octocode ops: npx octocode skill|search; agents: Codex, Claude, Cursor, Pi
start: attend; workspace status; plan create|list|show|join|doc|status; task create|list|ready|show|claim|heartbeat|submit|release|depend; memory recall; signal list
work: start|touch|end|list|show (advisory); exclusive lock acquire|wait|release|prune (sensitive only); verify audit|mark
msg: signal publish|list|reply|ack|resolve|prune; agent register|list
bookkeep/housekeep: memory record|forget; reflect record|mine-weakness|export-harness|developer-review; maintenance digest; lock|signal prune
repo: query files|workboard|all|developer-review --format json|table|csv|markdown|html; repo inject
inspect: schema commands --compact; docs list|show; schema json-schema <name>; <command> --help`;
var COMMAND_TO_SCHEMA = {
  "tell-memory": "tell_memory",
  "get-memory": "get_memory",
  "pre-flight-intent": "pre_flight_intent",
  "wait-for-lock": "wait_for_lock",
  "prune-stale-locks": "prune_stale_locks",
  "release-file-lock": "release_file_lock",
  "audit-unverified": "audit_unverified",
  "verify": "verify",
  "forget": "forget_memory",
  "refine-set": "refinement",
  "refine-get": "refine_query",
  "refine-delete": "refine_delete",
  "agent-registry": "agent_registry",
  "agent-signal": "agent_signal",
  "notify-prune": "signal_prune",
  "status": "workspace_status",
  "attend": "attend",
  "export-harness": "export_harness",
  "query": "query",
  "repo-inject": "repo_inject",
  "session-capture": "session_capture",
  "mine-weakness": "mine_weakness",
  "doc-staleness": "doc_staleness",
  "docs-catalog": "docs_catalog",
  "digest": "digest",
  "reflect": "reflect",
  "plan-command": "plan",
  "task-command": "task",
  "work-command": "work"
};
var COMMAND_DISPLAY = {
  "tell-memory": "memory record",
  "get-memory": "memory recall",
  "forget": "memory forget",
  "pre-flight-intent": "lock acquire",
  "wait-for-lock": "lock wait",
  "prune-stale-locks": "lock prune",
  "release-file-lock": "lock release",
  "audit-unverified": "verify audit",
  "verify": "verify mark",
  "refine-set": "refinement set",
  "refine-get": "refinement get",
  "refine-delete": "refinement delete",
  "agent-registry": "agent register|list",
  "agent-signal": "signal publish|list|reply|ack|resolve",
  "notify-prune": "signal prune",
  "status": "workspace status",
  "attend": "attend",
  "export-harness": "reflect export-harness",
  "developer-review": "reflect developer-review",
  "query": "query",
  "repo-inject": "repo inject",
  "session-capture": "session capture",
  "mine-weakness": "reflect mine-weakness",
  "doc-staleness": "docs staleness",
  "docs-catalog": "docs list|show",
  "digest": "maintenance digest",
  "init": "maintenance init",
  "self-test": "maintenance self-test",
  "reflect": "reflect record",
  "plan-command": "plan create|list|show|join|doc|status",
  "task-command": "task create|list|ready|show|claim|heartbeat|submit|release|depend",
  "work-command": "work start|touch|end|list|show",
  "hook-run": "hook run",
  "hooks-install": "hooks install|check|remove",
  "schema": "schema"
};
var COMMAND_EXAMPLE = {
  "tell-memory": 'octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact',
  "get-memory": 'octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact',
  "forget": "octocode-awareness memory forget --memory-id mem_123 --dry-run --compact",
  "pre-flight-intent": 'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact',
  "wait-for-lock": "octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact",
  "prune-stale-locks": 'octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact',
  "release-file-lock": "octocode-awareness lock release --agent-id agent --run-id run_123 --status SUCCESS --verified --compact",
  "audit-unverified": 'octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact',
  "verify": 'octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact',
  "refine-set": 'octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact',
  "refine-get": 'octocode-awareness refinement get --workspace "$PWD" --state open --compact',
  "refine-delete": "octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact",
  "agent-registry": 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact',
  "agent-signal": 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
  "notify-prune": 'octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact',
  "status": 'octocode-awareness workspace status --workspace "$PWD" --compact',
  "attend": 'octocode-awareness attend --query "current task" --workspace "$PWD" --compact',
  "export-harness": 'octocode-awareness reflect export-harness --workspace "$PWD" --compact',
  "developer-review": 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  "query": 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact',
  "repo-inject": 'octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact',
  "session-capture": 'octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact',
  "mine-weakness": 'octocode-awareness reflect mine-weakness --workspace "$PWD" --compact',
  "doc-staleness": `octocode-awareness docs staleness --targets-json '[{"docFile":"README.md","sourceDirs":["src"]}]' --compact`,
  "docs-catalog": "octocode-awareness docs list --compact",
  "digest": 'octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact',
  "init": "octocode-awareness maintenance init --compact",
  "self-test": "octocode-awareness maintenance self-test --compact",
  "reflect": 'octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep commands canonical" --compact',
  "plan-command": 'octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact',
  "task-command": "octocode-awareness task ready --plan-id plan_123 --compact",
  "work-command": 'octocode-awareness work start --agent-id agent --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact',
  "hook-run": "octocode-awareness hook run pre-edit < hook-payload.json",
  "hooks-install": "octocode-awareness hooks install --host codex --dry-run --compact",
  "schema": "octocode-awareness schema commands --compact"
};
var ROUTE_EXAMPLE = {
  "signal publish": 'octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact',
  "signal list": 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
  "signal reply": 'octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact',
  "signal ack": "octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact",
  "signal resolve": "octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact",
  "agent register": 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact',
  "agent list": 'octocode-awareness agent list --workspace "$PWD" --compact',
  "reflect developer-review": 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  "docs list": "octocode-awareness docs list --compact",
  "docs show": "octocode-awareness docs show architecture",
  "hooks install": "octocode-awareness hooks install --host codex --dry-run --compact",
  "hooks check": "octocode-awareness hooks check --host codex --strict --compact",
  "hooks remove": "octocode-awareness hooks remove --host codex --dry-run --compact",
  "schema commands": "octocode-awareness schema commands --compact",
  "schema list": "octocode-awareness schema list --compact",
  "schema json-schema": "octocode-awareness schema json-schema get_memory --compact",
  "schema example": "octocode-awareness schema example get_memory --compact",
  "schema validate": "octocode-awareness schema validate get_memory payload.json --compact"
};
var REMOVED_COMMAND_REPLACEMENTS = {
  "tell-memory": "memory record",
  "get-memory": "memory recall",
  "forget": "memory forget",
  "memory-index": "query memories --format markdown",
  "pre-flight-intent": "lock acquire",
  "wait-for-lock": "lock wait",
  "prune-stale-locks": "lock prune",
  "release-file-lock": "lock release",
  "audit-unverified": "verify audit",
  "verify": "verify mark",
  "refine-set": "refinement set",
  "refine-get": "refinement get",
  "refine-delete": "refinement delete",
  "agent-registry": "agent register|list",
  "agent-signal": "signal publish|list|reply|ack|resolve",
  "notify": "signal publish",
  "notify-get": "signal list",
  "notify-resolve": "signal resolve",
  "notify-prune": "signal prune",
  "workspace-status": "workspace status",
  "status": "workspace status",
  "export-harness": "reflect export-harness",
  "reflect": "reflect record",
  "mine-weakness": "reflect mine-weakness",
  "doc-staleness": "docs staleness",
  "docs-catalog": "docs list|show",
  "session-capture": "session capture",
  "digest": "maintenance digest",
  "view": "query all --format html --out .octocode/awareness/index.html",
  "inject": "repo inject",
  "init": "maintenance init",
  "self-test": "maintenance self-test"
};
var COMMAND_HELP = {
  "tell-memory": `usage: octocode-awareness memory record --agent-id <id> --task-context <text> --observation <text> --importance <1-10> [--label <l>] [--tag <t>]... [--reference <r>]... [--file <p>]...
example: octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact
note: unknown --label values hard-error
schema: octocode-awareness schema json-schema tell_memory --compact`,
  "get-memory": `usage: octocode-awareness memory recall [options]
filters: [--query <text>] [--limit <n>] [--min-importance <n>] [--label <l>]... [--tag <t>]... [--reference <r>]... [--file <p>]... [--regex <r>]... [--file-regex <r>]...
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--strict-scope] [--global-only]
rank: [--sort smart|score|importance|recent|accessed] [--state ACTIVE|SUPERSEDED]... [--as-of <iso>] [--semantic] [--explain] [--full]
example: octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
note: default recall projects lean fields; pass --full (or --explain) for complete MemoryRecord rows
schema: octocode-awareness schema json-schema get_memory --compact`,
  "pre-flight-intent": `usage: octocode-awareness lock acquire --agent-id <id> --target-file <p>... [--run-id <claimed-run>] [--workspace <p>] [--artifact <a>] [--rationale <t>] [--test-plan <t>] [--ttl-minutes <n>] [--wait-seconds <n>]
example: octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact
note: ordinary edits use work start/touch; lock acquire always adds exclusive protection (flag not configurable)
note: export OCTOCODE_AGENT_ID for CLI+hooks; --strict-agent-id / OCTOCODE_STRICT_AGENT_ID=1 hard-fails when missing
schema: octocode-awareness schema json-schema pre_flight_intent --compact`,
  "agent-signal": `usage: octocode-awareness signal publish|list|reply|ack|resolve --agent-id <id> [--to-agent <id>]... [--signal-id <id>]... [--thread-id <id>] [--kind <k>] [--subject <t>] [--body <t>] [--file <p>]...
examples:
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --include-bodies --compact
  octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --file src/file.ts --workspace "$PWD" --compact
  octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact
note: signal list summarizes body to 160 chars unless --include-bodies
schema: octocode-awareness schema json-schema agent_signal --compact`,
  "verify": `usage: octocode-awareness verify mark (--run-id <id>... | --all-pending) --agent-id <id> [--status SUCCESS|FAILED] [--message <t>] [--workspace <p>] [--artifact <a>]
example: octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema verify --compact`,
  "reflect": `usage: octocode-awareness reflect record --agent-id <id> --task <text> --outcome worked|partial|failed [--lesson <t>] [--fix-repo <t>] [--fix-instructions <t>] [--fix-file <p>]... [--failure-signature <s>]
example: octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep CLI nouns canonical" --compact
note: --outcome must be worked|partial|failed; unknown values hard-error
note: --fix-repo \u2192 coding refinement; --fix-harness \u2192 skill/tooling; --fix-instructions \u2192 feedback to the human instruction author (see reflect developer-review)
schema: octocode-awareness schema json-schema reflect --compact`,
  "developer-review": `usage: octocode-awareness reflect developer-review [--workspace <repo>] [--state open|ongoing|done]... [--format json|markdown] [--limit <n>]
example: octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact
note: reads agent feedback on the instructions themselves (from reflect record --fix-instructions); same rows feed .octocode/DEVELOPER_REVIEW.md`,
  "query": `usage: octocode-awareness query <all|repo-profile|memories|gotchas|lessons|plans|tasks|runs|locks|agents|signals|refinements|files|activity|workboard|developer-review> [--workspace <repo>] [--format json|table|csv|markdown|html] [--out <path>]
examples:
  octocode-awareness query files --workspace "$PWD" --format table --limit 50
  octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
note: files/memories expose missing file references as file_exists, missing_file, missing_references, and stale_file_refs workboard reasons
schema: octocode-awareness schema json-schema query --compact`,
  "attend": `usage: octocode-awareness attend [--workspace <repo>] [--query <text>] [--file <p>]... [--limit <n>] [--include-bodies] [--explain-organ]
example: octocode-awareness attend --query "current task" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema attend --compact`,
  "repo-inject": `usage: octocode-awareness repo inject [--workspace <repo>] [--out .octocode] [--mode local|share] [--no-check] [--no-include-view]
example: octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
schema: octocode-awareness schema json-schema repo_inject --compact`,
  "docs-catalog": `usage: octocode-awareness docs list|show [name] [--full]
examples:
  octocode-awareness docs list --compact
  octocode-awareness docs list --full --compact
  octocode-awareness docs show architecture
  octocode-awareness docs show architecture --compact
note: docs list is lean by default (no abs path/root); --full restores path+root
schema: octocode-awareness schema json-schema docs_catalog --compact`,
  "plan-command": `usage: octocode-awareness plan create|list|show|join|doc|status [options]
create: --name <text> --objective <text> --lead-agent-id <id> --workspace <repo> [--artifact <name>]
show/join/doc/status: --plan-id <id>; join also --agent-id <id>; doc uses --agent-id <member> --path docs/NOTE.md --title <text>; status uses --agent-id <lead> --status DRAFT|ACTIVE|PAUSED|COMPLETED|CANCELLED
example: octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema plan --compact`,
  "task-command": `usage: octocode-awareness task create|list|ready|show|claim|heartbeat|submit|release|depend [options]
create: --plan-id <id> --title <text> --reasoning <text> --path <workspace-relative>... --agent-id <id> [--acceptance <text>] [--depends-on <task-id>]...
claim: --task-id <id> --agent-id <id>; or --next --plan-id <id> --agent-id <id>. Returns run_id for lock/submit/verify.
heartbeat/submit/release: --task-id <id> --run-id <id> --agent-id <id>; release optionally --blocked-reason <text>
example: octocode-awareness task ready --plan-id plan_123 --compact
schema: octocode-awareness schema json-schema task --compact`,
  "work-command": `usage: octocode-awareness work start|touch|end|list|show [options]
start: --agent-id <id> --file <path>... --rationale <why> --test-plan <check> [--run-id <claimed-task-run>] [--exclusive] [--ttl-minutes <1-60>]
touch/end: --agent-id <id> --run-id <id> [--file <path>]...; omit files to touch/end all active presence
list/show: [--workspace <repo>] [--agent-id <id>] [--run-id <id>] [--file <path>] [--all] [--full]
example: octocode-awareness work start --agent-id agent --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact
schema: octocode-awareness schema json-schema work --compact`,
  "hook-run": `usage: octocode-awareness hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json`,
  "hooks-install": hooksInstallUsage(),
  "schema": `usage: octocode-awareness schema commands|list|json-schema <name>|example <name>|validate <name> <json-file|->
examples:
  octocode-awareness schema commands --compact
  octocode-awareness schema commands --examples --compact
  octocode-awareness schema json-schema query --compact
note: schema commands omits examples by default; pass --examples for full recipes`,
  "init": `usage: octocode-awareness maintenance init [--db <path>]
example: octocode-awareness maintenance init --db .octocode/awareness.sqlite3 --compact`,
  "self-test": `usage: octocode-awareness maintenance self-test
example: octocode-awareness maintenance self-test --compact`
};
function hyphenFlag(flag2) {
  return `--${flag2.replace(/_/g, "-")}`;
}
function helpFor(command2, options = {}) {
  if (!command2) return options.compact ? HELP_COMPACT : HELP;
  const normalized = command2.replace(/_/g, "-");
  const flags = KNOWN_FLAGS[normalized];
  if (!flags) return HELP;
  const schema = COMMAND_TO_SCHEMA[normalized] ?? null;
  const display = options.routeKey ?? COMMAND_DISPLAY[normalized] ?? normalized;
  const example = (options.routeKey ? ROUTE_EXAMPLE[options.routeKey] : void 0) ?? COMMAND_EXAMPLE[normalized];
  if (options.compact) {
    return [
      `usage: octocode-awareness ${display} [options]`,
      schema ? `schema: ${schema}` : "schema: none",
      `example: ${example ?? `octocode-awareness ${display}`}`
    ].join("\n").trimEnd();
  }
  if (COMMAND_HELP[normalized]) return COMMAND_HELP[normalized];
  return [
    `usage: octocode-awareness ${display} [options]`,
    `flags: ${flags.map(hyphenFlag).join(" ")}`,
    schema ? `schema: octocode-awareness schema json-schema ${schema} --compact` : "schema: none",
    example ? `example: ${example}` : ""
  ].join("\n").trimEnd();
}
function commandFromHelpArgv(argv) {
  const withoutHelp = argv.filter((arg) => arg !== "--help" && arg !== "-h" && arg !== "--compact");
  const filtered = extractGlobalDb(withoutHelp).filtered;
  const [firstRaw, secondRaw] = filtered;
  const first = normalizeToken(firstRaw);
  const second = normalizeToken(secondRaw);
  let routeKey;
  if (first === "hook" && second === "run") routeKey = "hook run";
  else if (first === "hooks" && second && ["install", "check", "remove"].includes(second)) routeKey = `hooks ${second}`;
  else if (first === "schema" && second && ["commands", "list", "json-schema", "example", "validate"].includes(second)) routeKey = `schema ${second}`;
  else if (first && second && COMMAND_ROUTES[`${first} ${second}`]) routeKey = `${first} ${second}`;
  else if (first && SINGLE_COMMANDS.has(first)) routeKey = first;
  return { command: selectCommand(filtered).command ?? null, routeKey };
}
var rawArgv = process.argv.slice(2);
if (rawArgv.length === 0 || rawArgv.includes("--help") || rawArgv.includes("-h")) {
  const compactHelp = rawArgv.includes("--compact") || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
  const helpTarget = commandFromHelpArgv(rawArgv);
  process.stdout.write(helpFor(helpTarget.command, { compact: compactHelp, routeKey: helpTarget.routeKey }) + "\n");
  process.exit(0);
}
var { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
var { command, rest } = selectCommand(filteredArgv);
var args = parseArgs(rest ?? []);
if (globalDb) args["db"] = globalDb;
if (command && KNOWN_FLAGS[command]) {
  const unknown = validateFlags(command, args);
  if (unknown.length > 0) {
    const compactError = args["compact"] === true || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
    const payload = {
      ok: false,
      command: COMMAND_DISPLAY[command] ?? command,
      schema: COMMAND_TO_SCHEMA[command] ?? null,
      error: `unknown flag(s): ${unknown.map((f) => `--${f.replace(/_/g, "-")}`).join(", ")}`,
      known_flags: KNOWN_FLAGS[command].map((f) => `--${f.replace(/_/g, "-")}`),
      hint: `Run "octocode-awareness ${COMMAND_DISPLAY[command] ?? command} --help" for this command.`,
      example: COMMAND_EXAMPLE[command]
    };
    process.stdout.write(JSON.stringify(payload, null, compactError ? 0 : 2) + "\n");
    process.exit(1);
  }
}
if (command && command !== UNKNOWN_COMMAND) validateFlagValues(args);
var dbPath = resolveDbPath(globalDb ?? null);
var compact = args["compact"] === true || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
var opts = { compact };
if (!command) {
  process.stdout.write((compact ? HELP_COMPACT : HELP) + "\n");
  process.exit(0);
}
if (command === UNKNOWN_COMMAND) {
  const requested = filteredArgv.slice(0, 2).join(" ") || filteredArgv[0] || "";
  const first = filteredArgv[0]?.replace(/_/g, "-");
  const replacement = first ? REMOVED_COMMAND_REPLACEMENTS[first] : void 0;
  const payload = {
    ok: false,
    error: `unknown command: ${requested}`,
    hint: replacement ? `Use canonical command: octocode-awareness ${replacement}` : 'Use canonical noun/verb commands only; run "octocode-awareness --help" for the command map.',
    replacement,
    examples: [
      'octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact',
      'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact',
      'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
      'octocode-awareness query gotchas --workspace "$PWD" --format json --limit 20 --compact'
    ]
  };
  process.stdout.write(JSON.stringify(payload, null, compact ? 0 : 2) + "\n");
  process.exit(1);
}
if (command === "self-test") {
  process.exit(cmdSelfTest(opts));
}
if (command === "schema") {
  const script = packageSkillScriptPath("schema.mjs");
  const result = spawnSync6(process.execPath, [script, ...rest], { stdio: "inherit" });
  process.exit(result.status ?? 1);
}
if (command === "hook-run") {
  if (globalDb) die("hook run ignores --db: hooks always use the canonical store. Remove --db, or set OCTOCODE_MEMORY_HOME to relocate the store.");
  process.exit(await runHookCommand(String(args._[0] ?? "help")));
}
if (command === "hooks-install") {
  const result = runHooksInstall(rest, { hookDir: packageSkillScriptPath("hooks") });
  if (result.text !== void 0) process.stdout.write(result.text);
  else if (result.payload) emit(result.payload, result.exitCode, opts);
  process.exit(result.exitCode);
}
var db2;
try {
  db2 = connectDb(dbPath);
} catch (err) {
  process.stderr.write(`octocode-awareness: failed to connect DB at ${dbPath}: ${String(err)}
`);
  process.exit(1);
}
var exitCode = 0;
try {
  switch (command) {
    case "tell-memory":
      exitCode = cmdTellMemory(db2, args, dbPath, opts);
      break;
    case "get-memory":
      exitCode = cmdGetMemory(db2, args, dbPath, opts);
      break;
    case "reflect":
      exitCode = cmdReflect(db2, args, dbPath, opts);
      break;
    case "refine-set":
      exitCode = cmdRefineSet(db2, args, dbPath, opts);
      break;
    case "refine-get":
      exitCode = cmdRefineGet(db2, args, dbPath, opts);
      break;
    case "pre-flight-intent":
      exitCode = cmdPreFlightIntent(db2, args, dbPath, opts);
      break;
    case "release-file-lock":
      exitCode = cmdReleaseFileLock(db2, args, dbPath, opts);
      break;
    case "plan-command":
      exitCode = cmdPlan(db2, args, dbPath, opts);
      break;
    case "task-command":
      exitCode = cmdTask(db2, args, dbPath, opts);
      break;
    case "work-command":
      exitCode = cmdWork(db2, args, dbPath, opts);
      break;
    case "status":
      exitCode = cmdStatus(db2, dbPath, args, opts);
      break;
    case "init":
      exitCode = cmdInit(db2, dbPath, opts);
      break;
    case "prune-stale-locks":
      exitCode = emit({ db_path: dbPath, ...pruneStale(db2, args) }, 0, opts);
      break;
    case "audit-unverified":
      exitCode = cmdAuditUnverified(db2, args, dbPath, opts);
      break;
    case "verify":
      exitCode = cmdVerify(db2, args, dbPath, opts);
      break;
    case "session-capture":
      exitCode = emit({
        db_path: dbPath,
        ...sessionCapture(db2, {
          agent_id: args["agent_id"],
          workspace: args["workspace"],
          artifact: args["artifact"],
          repo: args["repo"],
          ref: args["ref"],
          reason: args["reason"],
          cwd: args["cwd"]
        })
      }, 0, opts);
      break;
    case "mine-weakness": {
      const mwParams = {
        agentId: args["agent_id"],
        workspacePath: args["workspace"],
        artifact: args["artifact"],
        minCount: args["min_count"] ? Number(args["min_count"]) : void 0,
        limit: args["limit"] ? Number(args["limit"]) : void 0,
        cwd: args["cwd"]
      };
      exitCode = emit({ db_path: dbPath, ...mineWeakness(db2, mwParams) }, 0, opts);
      break;
    }
    case "doc-staleness":
      exitCode = cmdDocStaleness(db2, args, dbPath, opts);
      break;
    case "docs-catalog":
      exitCode = cmdDocsCatalog(db2, args, dbPath, opts);
      break;
    case "digest": {
      const retDays = args["retention_days"] ? Number(args["retention_days"]) : void 0;
      const handoffDays = args["refinement_handoff_retention_days"] ? Number(args["refinement_handoff_retention_days"]) : void 0;
      const doneDays = args["refinement_done_retention_days"] ? Number(args["refinement_done_retention_days"]) : void 0;
      const isDryRun = Boolean(args["dry_run"] ?? args["dry-run"]);
      const digestResult = digest(db2, {
        ...retDays !== void 0 ? { retention_days: retDays } : {},
        ...handoffDays !== void 0 ? { refinement_handoff_retention_days: handoffDays } : {},
        ...doneDays !== void 0 ? { refinement_done_retention_days: doneDays } : {},
        ...args["workspace"] ? { workspace: String(args["workspace"]) } : {},
        ...args["artifact"] ? { artifact: String(args["artifact"]) } : {},
        ...isDryRun ? { dry_run: true } : {}
      });
      const payload = { db_path: dbPath, ...digestResult };
      if (!isDryRun && (args["export_doc"] ?? args["export-doc"])) {
        try {
          const wsPath = args["workspace"] ?? process.cwd();
          const artifact2 = args["artifact"];
          const { mkdirSync: mkdirSync7, writeFileSync: writeFileSync6 } = await import("node:fs");
          const { join: join10 } = await import("node:path");
          const docDir = join10(wsPath, ".octocode", "memory-reports");
          mkdirSync7(docDir, { recursive: true });
          const dateStr = (/* @__PURE__ */ new Date()).toISOString().slice(0, 16).replace("T", "-").replace(":", "");
          const docPath = typeof (args["export_doc"] ?? args["export-doc"]) === "string" ? args["export_doc"] ?? args["export-doc"] : join10(docDir, `memory-report-${dateStr}.md`);
          writeFileSync6(docPath, exportMemoryDoc(db2, { workspace_path: wsPath, artifact: artifact2 }), "utf8");
          payload["doc_path"] = docPath;
        } catch (err) {
          payload["doc_warning"] = `Could not write doc: ${err.message}`;
        }
      }
      exitCode = emit(payload, 0, opts);
      break;
    }
    case "wait-for-lock": {
      const rawWaitTarget = args["target_file"] ?? args["file"];
      const waitTargets = Array.isArray(rawWaitTarget) ? rawWaitTarget : rawWaitTarget ? [String(rawWaitTarget)] : [];
      const waitSecs = parseBoundedSeconds(args, "wait_seconds", 0, MAX_CLI_WAIT_SECONDS);
      const retrySecs = parseBoundedSeconds(args, "retry_interval", 1, MAX_CLI_RETRY_INTERVAL_SECONDS);
      const waitResult = waitForLock(db2, {
        agent_id: args["agent_id"],
        target_files: waitTargets,
        workspace: args["workspace"],
        artifact: args["artifact"],
        lock_type: "EXCLUSIVE",
        wait_ms: waitSecs != null ? waitSecs * 1e3 : void 0,
        retry_interval_ms: retrySecs != null ? retrySecs * 1e3 : void 0
      });
      exitCode = emit({ db_path: dbPath, ...waitResult }, waitResult.lock_free ? 0 : 2, opts);
      break;
    }
    case "forget":
      exitCode = cmdForget(db2, args, dbPath, opts);
      break;
    case "refine-delete":
      exitCode = cmdRefineDelete(db2, args, dbPath, opts);
      break;
    case "export-harness":
      exitCode = cmdExportHarness(db2, args, dbPath, opts);
      break;
    case "developer-review":
      exitCode = cmdDeveloperReview(db2, args, dbPath, opts);
      break;
    case "query":
      exitCode = cmdQuery(db2, args, dbPath, opts);
      break;
    case "attend":
      exitCode = cmdAttend(db2, args, dbPath, opts);
      break;
    case "repo-inject":
      exitCode = cmdRepoInject(db2, args, dbPath, opts);
      break;
    case "agent-registry":
      exitCode = cmdAgentRegistry(db2, args, dbPath, opts);
      break;
    case "agent-signal": {
      const signalFormat = String(args["format"] ?? "json");
      if (args["action"] === "list" && signalFormat === "hook") {
        const signalBriefing = notifyGet(db2, {
          workspace: args["workspace"],
          artifact: args["artifact"],
          format: signalFormat,
          agent_id: args["agent_id"]
        });
        exitCode = signalBriefing["additionalContext"] ? emit({ additionalContext: signalBriefing["additionalContext"] }, 0, opts) : emit({ db_path: dbPath, ...signalBriefing }, 0, opts);
      } else {
        exitCode = cmdAgentSignal(db2, args, dbPath, opts);
      }
      break;
    }
    case "notify-prune":
      exitCode = cmdNotifyPrune(db2, args, dbPath, opts);
      break;
    default:
      exitCode = emit({ error: `unknown command: ${command}. Run --help for usage.` }, 1, opts);
  }
} catch (err) {
  exitCode = emit({
    error: err instanceof Error ? err.message : String(err)
  }, 1, opts);
}
process.exit(exitCode);
