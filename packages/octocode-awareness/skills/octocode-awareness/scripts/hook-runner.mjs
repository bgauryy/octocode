#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (w) => {
  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;
  console.error(w?.stack ?? String(w));
});

// bin/hook-runner.ts
import { createHash as createHash3 } from "node:crypto";
import {
  closeSync,
  mkdirSync as mkdirSync2,
  openSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync
} from "node:fs";
import { basename as basename2, dirname as dirname3, join as join3, relative as relative2, resolve as resolve7 } from "node:path";
import { fileURLToPath } from "node:url";

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
function normalizeArtifact(value) {
  if (value == null) return null;
  const cleaned = String(value).trim().slice(0, 256);
  return cleaned.length > 0 ? cleaned : null;
}

// src/git.ts
import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { basename, dirname, join, resolve as resolve2 } from "node:path";
function runCmd(cmd, args, cwd) {
  try {
    const r = spawnSync(cmd, args, { cwd: cwd ?? process.cwd(), encoding: "utf8", timeout: 5e3 });
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
  let dir = resolve2(input);
  const tail = [];
  for (let guard = 0; guard < 4096; guard += 1) {
    try {
      return tail.length ? join(realpathSync(dir), ...tail) : realpathSync(dir);
    } catch {
      const parent = dirname(dir);
      if (parent === dir) return resolve2(input);
      tail.unshift(basename(dir));
      dir = parent;
    }
  }
  return resolve2(input);
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
  const candidate = workspacePath ? resolve2(workspacePath) : cwd ? resolve2(cwd) : null;
  const scope = fillScope({ workspace_path: candidate }, candidate ?? process.cwd());
  if (scope.workspace_path) return scope.workspace_path;
  return candidate;
}

// src/sql/agents.ts
var AGENTS_UPSERT = `INSERT INTO agents (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)
   ON CONFLICT(agent_id) DO UPDATE SET
     agent_name     = CASE WHEN excluded.agent_name <> '' THEN excluded.agent_name ELSE agent_name END,
     workspace_path = COALESCE(excluded.workspace_path, workspace_path),
     artifact       = COALESCE(excluded.artifact, artifact),
     context        = COALESCE(excluded.context, context),
     last_seen_at   = excluded.last_seen_at`;

// src/agents.ts
function registerAgent(db2, params) {
  const agentId2 = params.agentId;
  const agentName2 = params.agentName ?? "";
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const context = params.context ?? null;
  const now = utcNow();
  db2.prepare(AGENTS_UPSERT).run(agentId2, agentName2, workspacePath, artifact2, context, now, now);
  return { agent_id: agentId2, agent_name: agentName2, workspace_path: workspacePath, artifact: artifact2, context, registered_at: now, last_seen_at: now };
}

// src/audit.ts
import { randomUUID } from "node:crypto";
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

// src/audit.ts
function insertEditLog(db2, params) {
  const editId = "edit_" + randomUUID();
  const now = utcNow();
  db2.prepare(EDIT_LOG_INSERT).run(
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

// src/db.ts
import { DatabaseSync } from "node:sqlite";
import { mkdirSync } from "node:fs";
import { join as join2, resolve as resolve3, dirname as dirname2 } from "node:path";
import { homedir, platform } from "node:os";

// src/v4/runtime.ts
var FIXED_BRANCHES = /* @__PURE__ */ new Map([
  [44, 6],
  [50, 7],
  [51, 3]
]);
function parseSqliteVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:\D.*)?$/.exec(version.trim());
  if (!match) return null;
  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);
  if (![major, minor, patch].every(Number.isSafeInteger)) return null;
  return [major, minor, patch];
}
function assessConcurrentWalSafety(sqliteVersion) {
  const parsed = parseSqliteVersion(sqliteVersion);
  if (!parsed) {
    return {
      sqliteVersion,
      safe: false,
      reason: "the embedded SQLite version could not be parsed"
    };
  }
  const [major, minor, patch] = parsed;
  const futureFixedLine = major > 3 || major === 3 && minor > 51;
  const fixedPatch = major === 3 ? FIXED_BRANCHES.get(minor) : void 0;
  const safe = futureFixedLine || fixedPatch !== void 0 && patch >= fixedPatch;
  return {
    sqliteVersion,
    safe,
    reason: safe ? "the embedded SQLite includes the concurrent WAL reset fix" : "concurrent WAL requires SQLite 3.44.6, 3.50.7, or 3.51.3 (or a newer fixed release)"
  };
}
function journalModeForSqliteVersion(sqliteVersion) {
  return assessConcurrentWalSafety(sqliteVersion).safe ? "WAL" : "DELETE";
}

// src/db.ts
var DEFAULT_DB_NAME = "awareness.sqlite3";
var MEMORY_HOME_ENV = "OCTOCODE_MEMORY_HOME";
var V3_SCHEMA_VERSION = 3;
var SQLITE_BUSY_RETRY_MS = 25;
var SQLITE_BUSY_DEADLINE_MS = 1e4;
var SQLITE_WAIT = new Int32Array(new SharedArrayBuffer(4));
var LEGACY_V0_RELATION_NAMES = /* @__PURE__ */ new Set([
  "agent_intents",
  "agent_memories",
  "file_locks",
  "intent_events",
  "memory_fts",
  "notifications",
  "notification_reads",
  "task_log"
]);
var _db;
function memoryHome() {
  const configured = process.env[MEMORY_HOME_ENV];
  if (configured?.trim()) return resolve3(configured.trim());
  const h = homedir();
  const p = platform();
  if (p === "win32") {
    const appData = process.env["APPDATA"] ?? join2(h, "AppData", "Roaming");
    return join2(appData, ".octocode", "memory");
  }
  if (p === "darwin") return join2(h, ".octocode", "memory");
  const xdg = process.env["XDG_CONFIG_HOME"] ?? join2(h, ".config");
  return join2(xdg, ".octocode", "memory");
}
function resolveDbPath(dbArg) {
  if (dbArg) return resolve3(dbArg);
  return join2(memoryHome(), DEFAULT_DB_NAME);
}
function connectDb(dbPath) {
  mkdirSync(dirname2(dbPath), { recursive: true });
  const db2 = new DatabaseSync(dbPath);
  try {
    db2.exec(`PRAGMA busy_timeout = ${SQLITE_BUSY_DEADLINE_MS}`);
    assertV3SchemaIdentity(db2);
    const versionRow = db2.prepare("SELECT sqlite_version() AS version").get();
    const journalMode = journalModeForSqliteVersion(versionRow.version);
    withSqliteBusyRetry(() => db2.exec(`PRAGMA journal_mode = ${journalMode}`));
    db2.exec("PRAGMA foreign_keys = ON");
    initDb(db2);
    _db = db2;
    return db2;
  } catch (error) {
    db2.close();
    throw error;
  }
}
function readV3SchemaIdentity(db2) {
  const application = db2.prepare("PRAGMA application_id").get();
  const version = db2.prepare("PRAGMA user_version").get();
  return {
    applicationId: application.application_id ?? 0,
    userVersion: version.user_version ?? 0
  };
}
function assertV3SchemaIdentity(db2) {
  const identity = readV3SchemaIdentity(db2);
  if (identity.applicationId !== 0) {
    throw new Error(
      `refusing foreign Awareness application_id ${identity.applicationId}; v3 expects 0`
    );
  }
  if (identity.userVersion > V3_SCHEMA_VERSION) {
    throw new Error(
      `refusing newer Awareness schema version ${identity.userVersion}; v3 supports versions 0-${V3_SCHEMA_VERSION}`
    );
  }
  if (identity.userVersion === 0) {
    const relations = db2.prepare(`
      SELECT name, type
      FROM sqlite_schema
      WHERE type IN ('table', 'view')
        AND name NOT LIKE 'sqlite_%'
      ORDER BY name
    `).all();
    if (relations.length === 0) return;
    const known = /* @__PURE__ */ new Set([
      ...canonicalColumns().keys(),
      ...LEGACY_V0_RELATION_NAMES,
      "memories_fts"
    ]);
    const unexpected = relations.filter(({ name, type }) => {
      if (type !== "table") return true;
      return !known.has(name) && !name.startsWith("memories_fts_") && !name.startsWith("memory_fts_");
    });
    if (unexpected.length > 0) {
      throw new Error(
        `refusing unrecognized non-empty unversioned SQLite store; unexpected relations: ${unexpected.map(({ name }) => name).join(", ")}`
      );
    }
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
function checkpointWal(db2) {
  try {
    db2.exec("PRAGMA wal_checkpoint(TRUNCATE)");
  } catch {
  }
}
function getDeliveryFingerprint(db2, key) {
  const row = db2.prepare(`SELECT fingerprint FROM delivery_state
    WHERE consumer_id = ? AND channel = ? AND scope_key = ?`).get(key.consumerId, key.channel, key.scopeKey);
  return row?.fingerprint ?? null;
}
function setDeliveryFingerprint(db2, params) {
  db2.prepare(`INSERT INTO delivery_state
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
function tableExists(db2, table) {
  return Boolean(db2.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?"
  ).get(table));
}
function renameColumnIfPresent(db2, table, from, to) {
  if (!tableExists(db2, table)) return;
  const columns = tableColumns(db2, table);
  if (columns.has(from) && !columns.has(to)) {
    db2.exec(`ALTER TABLE ${table} RENAME COLUMN ${from} TO ${to}`);
  }
}
function migrateLegacyTaskRuns(db2) {
  if (!tableExists(db2, "tasks")) return;
  const columns = tableColumns(db2, "tasks");
  const isLegacyExecutionTable = columns.has("agent_id") && columns.has("test_plan") && !columns.has("plan_id");
  if (!isLegacyExecutionTable) return;
  if (tableExists(db2, "task_runs")) {
    throw new Error("schema migration cannot move legacy tasks: task_runs already exists");
  }
  for (const index of ["idx_tasks_status", "idx_tasks_agent_status", "idx_tasks_workspace", "idx_tasks_scope"]) {
    db2.exec(`DROP INDEX IF EXISTS ${index}`);
  }
  db2.exec("ALTER TABLE tasks RENAME TO task_runs");
  renameColumnIfPresent(db2, "task_runs", "task_id", "run_id");
  renameColumnIfPresent(db2, "task_runs", "plan_doc_ref", "context_ref");
  renameColumnIfPresent(db2, "locks", "task_id", "run_id");
  if (tableExists(db2, "task_log") && !tableExists(db2, "run_log")) {
    db2.exec("ALTER TABLE task_log RENAME TO run_log");
  }
  renameColumnIfPresent(db2, "run_log", "task_id", "run_id");
  renameColumnIfPresent(db2, "edit_log", "task_id", "run_id");
  renameColumnIfPresent(db2, "harness_log", "task_id", "run_id");
}
function initDb(db2) {
  assertV3SchemaIdentity(db2);
  if (db2.isTransaction) {
    initDbSchema(db2);
    return;
  }
  db2.exec("PRAGMA foreign_keys = OFF");
  let began = false;
  try {
    withSqliteBusyRetry(() => db2.exec("BEGIN IMMEDIATE"));
    began = true;
    initDbSchema(db2);
    db2.exec("COMMIT");
    began = false;
  } catch (error) {
    if (began) {
      try {
        db2.exec("ROLLBACK");
      } catch {
      }
    }
    throw error;
  } finally {
    db2.exec("PRAGMA foreign_keys = ON");
  }
}
function initDbSchema(db2) {
  migrateLegacyTaskRuns(db2);
  db2.exec(SCHEMA_DDL);
  migrateExistingTables(db2);
  migrateExecutionSchemaV3(db2);
  migrateRefinementQualityConstraint(db2);
  migrateCheckConstraints(db2);
  db2.exec(`
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
    db2.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
      USING fts5(memory_id UNINDEXED, task_context, observation, tags)
    `);
  } catch {
  }
  if (hasFts(db2)) {
    const row = db2.prepare("SELECT COUNT(*) AS cnt FROM memories_fts").get();
    if (row.cnt === 0) rebuildFts(db2);
  }
  db2.exec("PRAGMA user_version = 3");
}
function tableColumns(db2, tableName) {
  const rows = db2.prepare(`PRAGMA table_info(${tableName})`).all();
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
function migrateExistingTables(db2) {
  for (const [table, columns] of canonicalColumns()) {
    const existing = tableColumns(db2, table);
    for (const col of columns) {
      if (existing.has(col.name)) continue;
      let clause = `${col.name} ${col.type}`;
      if (isConstantDefault(col.dflt_value)) {
        if (col.notnull) clause += " NOT NULL";
        clause += ` DEFAULT ${col.dflt_value}`;
      }
      db2.exec(`ALTER TABLE ${table} ADD COLUMN ${clause}`);
    }
  }
}
function migrateExecutionSchemaV3(db2) {
  if (!tableExists(db2, "task_runs")) return;
  const runColumns = tableColumns(db2, "task_runs");
  const lockColumns = tableExists(db2, "locks") ? tableColumns(db2, "locks") : /* @__PURE__ */ new Set();
  const needsRunRebuild = runColumns.has("files_json");
  const needsLockRebuild = ["agent_id", "session_id", "lock_type"].some((name) => lockColumns.has(name));
  if (!needsRunRebuild && !needsLockRebuild) return;
  if (needsRunRebuild) {
    const rows = db2.prepare(`SELECT run_id, task_id, status, files_json, created_at, updated_at
      FROM task_runs`).all();
    const insert = db2.prepare(`INSERT OR IGNORE INTO run_files
      (run_id, file_path, reason_override, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES (?, ?, NULL, ?, ?, ?, ?, ?)`);
    const now = utcNow();
    for (const row of rows) {
      const source = row.task_id == null ? "HOOK" : "EXPLICIT";
      const startedAt = row.created_at ?? now;
      const heartbeatAt = row.updated_at ?? startedAt;
      for (const filePath of parseJsonList(row.files_json)) {
        const lease = db2.prepare(`SELECT MAX(expires_at) AS expires_at FROM (
          SELECT expires_at FROM locks WHERE run_id = ? AND file_path = ?
          UNION ALL
          SELECT expires_at FROM task_claims WHERE run_id = ?
        )`).get(row.run_id, filePath, row.run_id);
        const expiresAt = lease.expires_at ?? heartbeatAt;
        const active = row.status === "ACTIVE" && expiresAt > now;
        insert.run(row.run_id, filePath, source, startedAt, heartbeatAt, expiresAt, active ? null : heartbeatAt);
      }
    }
    db2.prepare(`UPDATE task_runs SET origin = CASE
      WHEN task_id IS NOT NULL THEN 'TASK' ELSE 'HOOK' END`).run();
  }
  if (needsRunRebuild) {
    const sql = canonicalTableSql().get("task_runs");
    if (!sql) throw new Error("schema migration cannot find canonical task_runs DDL");
    rebuildTableFromCanonical(db2, "task_runs", sql);
  }
  if (needsLockRebuild) {
    const sql = canonicalTableSql().get("locks");
    if (!sql) throw new Error("schema migration cannot find canonical locks DDL");
    rebuildTableFromCanonical(db2, "locks", sql);
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
function rebuildTableFromCanonical(db2, table, canonSql) {
  const liveCols = tableColumns(db2, table);
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
  db2.exec(`SAVEPOINT ${savepoint}`);
  try {
    db2.exec(`DROP TABLE IF EXISTS ${tmpName};`);
    db2.exec(createTmp);
    db2.exec(`INSERT INTO ${tmpName} (${colList}) SELECT ${colList} FROM ${table};`);
    db2.exec(`DROP TABLE ${table};`);
    db2.exec(`ALTER TABLE ${tmpName} RENAME TO ${table};`);
    db2.exec(`RELEASE SAVEPOINT ${savepoint}`);
  } catch (err) {
    try {
      db2.exec(`ROLLBACK TO SAVEPOINT ${savepoint}`);
    } catch {
    }
    try {
      db2.exec(`RELEASE SAVEPOINT ${savepoint}`);
    } catch {
    }
    throw err;
  }
}
function migrateCheckConstraints(db2) {
  const drifted = [];
  for (const [table, canonSql] of canonicalTableSql()) {
    const live = db2.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name=?"
    ).get(table);
    if (!live?.sql) continue;
    if (checkClauses(live.sql) !== checkClauses(canonSql)) drifted.push([table, canonSql]);
  }
  if (drifted.length === 0) return;
  for (const [table, canonSql] of drifted) rebuildTableFromCanonical(db2, table, canonSql);
}
function migrateRefinementQualityConstraint(db2) {
  const row = db2.prepare(
    "SELECT sql FROM sqlite_master WHERE type='table' AND name='refinements'"
  ).get();
  if (!row?.sql || row.sql.includes("'instructions'")) return;
  db2.exec("SAVEPOINT migrate_refinement_quality_constraint");
  try {
    db2.exec(`
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
    db2.exec("RELEASE SAVEPOINT migrate_refinement_quality_constraint");
  } catch (err) {
    try {
      db2.exec("ROLLBACK TO SAVEPOINT migrate_refinement_quality_constraint");
    } catch {
    }
    try {
      db2.exec("RELEASE SAVEPOINT migrate_refinement_quality_constraint");
    } catch {
    }
    throw err;
  }
}
function hasFts(db2) {
  const row = db2.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories_fts'"
  ).get();
  return Boolean(row);
}
function ftsTermsForRow(row) {
  const tags = parseJsonList(row.tags_json);
  const label = (row.label ?? "OTHER").toLowerCase();
  return [...tags, label, ...row.references ?? []].filter(Boolean).join(" ");
}
function rebuildFts(db2) {
  db2.exec("SAVEPOINT rebuild_fts");
  try {
    db2.exec("DELETE FROM memories_fts");
    const rows = db2.prepare(
      "SELECT memory_id, task_context, observation, tags_json, label FROM memories"
    ).all();
    if (rows.length > 0) {
      const refs = db2.prepare(
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
    const insert = db2.prepare(
      "INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
    );
    for (const row of rows) {
      insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
    }
    db2.exec("RELEASE SAVEPOINT rebuild_fts");
  } catch (e) {
    try {
      db2.exec("ROLLBACK TO SAVEPOINT rebuild_fts");
    } catch {
    }
    try {
      db2.exec("RELEASE SAVEPOINT rebuild_fts");
    } catch {
    }
    throw e;
  }
}

// src/tasks.ts
import { randomUUID as randomUUID2 } from "node:crypto";
import { isAbsolute, relative, resolve as resolve4, sep } from "node:path";
var DEFAULT_CLAIM_LEASE_MS = 30 * 6e4;
var MAX_CLAIM_LEASE_MS = 60 * 6e4;
function event(db2, taskId, runId, agentId2, eventType, message, now = utcNow()) {
  db2.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`).run(`tevt_${randomUUID2().replace(/-/g, "")}`, taskId, runId, agentId2, eventType, message, now);
}
function evictExpiredTaskClaims(db2, now = utcNow()) {
  const expired = db2.prepare(
    "SELECT task_id, run_id, agent_id FROM task_claims WHERE expires_at <= ?"
  ).all(now);
  if (expired.length === 0) return;
  db2.exec("SAVEPOINT evict_expired_task_claims");
  try {
    for (const claim of expired) {
      db2.prepare("DELETE FROM locks WHERE run_id = ?").run(claim.run_id);
      db2.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, claim.run_id);
      db2.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'").run(now, claim.run_id);
      db2.prepare("UPDATE tasks SET status = 'OPEN', updated_at = ? WHERE task_id = ? AND status = 'IN_PROGRESS'").run(now, claim.task_id);
      db2.prepare("DELETE FROM task_claims WHERE task_id = ?").run(claim.task_id);
      event(db2, claim.task_id, claim.run_id, claim.agent_id, "CLAIM_EXPIRED", "claim lease expired", now);
    }
    db2.exec("RELEASE SAVEPOINT evict_expired_task_claims");
  } catch (e) {
    try {
      db2.exec("ROLLBACK TO SAVEPOINT evict_expired_task_claims");
    } catch {
    }
    try {
      db2.exec("RELEASE SAVEPOINT evict_expired_task_claims");
    } catch {
    }
    throw e;
  }
}
function activeTaskClaimForAgent(db2, params) {
  evictExpiredTaskClaims(db2);
  const workspacePath = normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve4(params.workspacePath);
  const where = ["c.agent_id = ?", "p.workspace_path = ?", "c.expires_at > ?"];
  const binds = [params.agentId, workspacePath, utcNow()];
  if (params.artifact) {
    where.push("(p.artifact = ? OR p.artifact IS NULL)");
    binds.push(params.artifact);
  }
  const claims = db2.prepare(`SELECT c.* FROM task_claims c
    JOIN tasks t ON t.task_id = c.task_id
    JOIN plans p ON p.plan_id = t.plan_id
    WHERE ${where.join(" AND ")} ORDER BY c.claimed_at DESC LIMIT 2`).all(...binds);
  return claims.length === 1 ? claims[0] : null;
}

// src/work.ts
import { randomUUID as randomUUID3 } from "node:crypto";
import { isAbsolute as isAbsolute2, resolve as resolve5 } from "node:path";
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
    return canonicalizePath(isAbsolute2(value) ? resolve5(value) : resolve5(base, value));
  }))];
}
function expiry(ttlMs) {
  const effective = Math.min(Math.max(1, ttlMs ?? DEFAULT_PRESENCE_TTL_MS), MAX_PRESENCE_TTL_MS);
  return new Date(Date.now() + effective).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function getRun(db2, runId) {
  const row = db2.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(runId);
  if (!row) throw new Error(`run not found: ${runId}`);
  return row;
}
function fileRows(db2, runId) {
  return db2.prepare("SELECT * FROM run_files WHERE run_id = ? ORDER BY file_path").all(runId);
}
function activePeerRows(db2, runId, files) {
  if (files.length === 0) return [];
  const now = utcNow();
  const rows = db2.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
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
function mutationResult(db2, runId, affectedFiles) {
  const allFiles = fileRows(db2, runId);
  const affected = affectedFiles ? new Set(affectedFiles) : null;
  const files = affected ? allFiles.filter((file) => affected.has(file.file_path)) : allFiles;
  const allPeers = activePeerRows(db2, runId, files.filter((file) => file.ended_at == null).map((file) => file.file_path));
  return {
    run: getRun(db2, runId),
    files,
    peers: allPeers.slice(0, PEER_DETAIL_LIMIT),
    peer_count: allPeers.length
  };
}
function conflictRows(db2, runId, files, exclusive) {
  if (files.length === 0) return [];
  const now = utcNow();
  const placeholders = files.map(() => "?").join(",");
  if (exclusive) {
    return db2.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
        tr.rationale, rf.heartbeat_at, rf.expires_at,
        EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
          AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
        'ACTIVE_WORK' AS conflict_type
      FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
      WHERE rf.file_path IN (${placeholders}) AND rf.run_id <> ?
        AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
      ORDER BY rf.file_path, rf.heartbeat_at DESC`).all(now, ...files, runId, now);
  }
  return db2.prepare(`SELECT l.run_id, tr.task_id, tr.origin, tr.agent_id, l.file_path,
      tr.rationale, rf.heartbeat_at, COALESCE(l.expires_at, rf.expires_at) AS expires_at,
      1 AS exclusive, 'EXCLUSIVE_LOCK' AS conflict_type
    FROM locks l
    JOIN task_runs tr ON tr.run_id = l.run_id
    LEFT JOIN run_files rf ON rf.run_id = l.run_id AND rf.file_path = l.file_path
    WHERE l.file_path IN (${placeholders}) AND l.run_id <> ? AND tr.status = 'ACTIVE'
      AND (l.expires_at IS NULL OR l.expires_at > ?)
    ORDER BY l.file_path, l.acquired_at DESC`).all(...files, runId, now);
}
function startWork(db2, params) {
  const agentId2 = required(params.agentId, "agent id");
  const now = utcNow();
  const expiresAt = expiry(params.ttlMs);
  const requestedOrigin = params.origin ?? "WORK";
  const source = params.source ?? (requestedOrigin === "HOOK" ? "HOOK" : "EXPLICIT");
  const fileBasePath = params.workspacePath ?? process.cwd();
  let wsPath = workspaceRoot(params.workspacePath);
  let artifact2 = normalizeArtifact(params.artifact);
  let runId = params.runId ?? null;
  if (!runId) {
    required(params.rationale, "rationale");
    required(params.testPlan, "test plan");
  }
  db2.exec("BEGIN IMMEDIATE");
  try {
    runId ??= `run_${randomUUID3().replace(/-/g, "")}`;
    let run = db2.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(runId);
    if (run) {
      if (run.agent_id !== agentId2) throw new Error(`run ${runId} belongs to ${run.agent_id}`);
      if (run.status !== "ACTIVE") throw new Error(`run ${runId} is not ACTIVE`);
      const runWorkspace = workspaceRoot(run.workspace_path);
      if (params.workspacePath != null && wsPath !== runWorkspace) {
        throw new Error(`workspace ${wsPath} does not match run workspace ${runWorkspace}`);
      }
      const runArtifact = normalizeArtifact(run.artifact);
      if (params.artifact != null && artifact2 !== runArtifact) {
        throw new Error(`artifact ${artifact2 ?? "(none)"} does not match run artifact ${runArtifact ?? "(none)"}`);
      }
      wsPath = runWorkspace;
      artifact2 = runArtifact;
    } else {
      if (params.runId) throw new Error(`run not found: ${params.runId}`);
      if (params.sessionId) {
        db2.prepare(`INSERT OR IGNORE INTO sessions
          (session_id, agent_id, workspace_path, artifact, started_at) VALUES (?, ?, ?, ?, ?)`).run(params.sessionId, agentId2, wsPath, artifact2, now);
      }
      db2.prepare(`INSERT INTO task_runs
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
      run = getRun(db2, runId);
    }
    const files = normalizeFiles(params.targetFiles, fileBasePath);
    const conflicts = conflictRows(db2, runId, files, params.exclusive === true);
    if (conflicts.length > 0) {
      db2.exec("ROLLBACK");
      return { ok: false, conflict: true, conflicts };
    }
    const upsert = db2.prepare(`INSERT INTO run_files
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
        db2.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`).run(`lock_${randomUUID3().replace(/-/g, "")}`, file, runId, now, expiresAt);
      }
    }
    db2.prepare("UPDATE task_runs SET updated_at = ? WHERE run_id = ?").run(now, runId);
    db2.exec("COMMIT");
    return { ok: true, ...mutationResult(db2, runId, files) };
  } catch (error) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
}
function touchWork(db2, params) {
  const run = getRun(db2, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.status !== "ACTIVE") throw new Error(`run ${params.runId} is not ACTIVE`);
  const targets = params.targetFiles?.length ? normalizeFiles(params.targetFiles, run.workspace_path) : fileRows(db2, params.runId).filter((file) => file.ended_at == null).map((file) => file.file_path);
  if (targets.length === 0) throw new Error("run has no active file presence");
  const now = utcNow();
  const expiresAt = expiry(params.ttlMs);
  db2.exec("BEGIN IMMEDIATE");
  try {
    const update = db2.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?
      WHERE run_id = ? AND file_path = ? AND ended_at IS NULL`);
    for (const file of targets) {
      const result = update.run(now, expiresAt, params.runId, file);
      if (result.changes === 0) throw new Error(`active file presence not found: ${file}`);
    }
    db2.prepare(`UPDATE locks SET expires_at = ? WHERE run_id = ?
      AND file_path IN (${targets.map(() => "?").join(",")})`).run(expiresAt, params.runId, ...targets);
    db2.prepare("UPDATE task_runs SET updated_at = ? WHERE run_id = ?").run(now, params.runId);
    db2.exec("COMMIT");
  } catch (error) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return mutationResult(db2, params.runId, targets);
}
function endWork(db2, params) {
  const run = getRun(db2, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.origin === "TASK") throw new Error("TASK work must end through task submit or task release");
  const targets = params.targetFiles?.length ? normalizeFiles(params.targetFiles, run.workspace_path) : fileRows(db2, params.runId).filter((file) => file.ended_at == null).map((file) => file.file_path);
  const now = utcNow();
  db2.exec("BEGIN IMMEDIATE");
  try {
    if (targets.length > 0) {
      const ended = db2.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND file_path IN (${targets.map(() => "?").join(",")}) AND ended_at IS NULL`).run(now, now, now, params.runId, ...targets);
      if (ended.changes !== targets.length) {
        throw new Error("one or more active file presences were not found for this run");
      }
      db2.prepare(`DELETE FROM locks WHERE run_id = ?
        AND file_path IN (${targets.map(() => "?").join(",")})`).run(params.runId, ...targets);
    }
    const active = db2.prepare(`SELECT 1 FROM run_files
      WHERE run_id = ? AND ended_at IS NULL AND expires_at > ? LIMIT 1`).get(params.runId, now);
    if (!active) {
      db2.prepare(`UPDATE task_runs SET status = 'PENDING', updated_at = ?
        WHERE run_id = ? AND status = 'ACTIVE' AND origin IN ('WORK','HOOK')`).run(now, params.runId);
    }
    db2.exec("COMMIT");
  } catch (error) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw error;
  }
  return mutationResult(db2, params.runId, targets);
}
function listWork(db2, params = {}) {
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
  const limit = params.limit == null ? null : Math.max(1, Math.floor(params.limit));
  const limitSql = limit == null ? "" : "LIMIT ?";
  if (limit != null) binds.push(limit);
  const rows = db2.prepare(`SELECT rf.*, tr.task_id, tr.origin, tr.agent_id, tr.session_id,
      tr.rationale, tr.test_plan, tr.status, tr.workspace_path, tr.artifact,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
      COUNT(*) OVER() AS result_total
    FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE ${where.join(" AND ")}
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id ${limitSql}`).all(...binds);
  const totalCount = rows[0]?.result_total ?? 0;
  const files = rows.map(({ result_total: _total, ...row }) => ({ ...row, exclusive: Boolean(row.exclusive) }));
  return {
    count: files.length,
    total_count: totalCount,
    omitted_count: Math.max(0, totalCount - files.length),
    files
  };
}

// src/verify.ts
import { randomUUID as randomUUID4 } from "node:crypto";

// src/sql/runs.ts
var RUNS_UPDATE_PENDING_TO_FAILED = `UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'PENDING'`;
var RUNS_UPDATE_ACTIVE_TO_FAILED = `UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'`;
var RUN_LOG_INSERT_ABANDONED = `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`;
var RUN_LOG_INSERT_STALE_ABANDONED = `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'stale active (no live file presence) abandoned by audit-unverified --abandon', ?)`;

// src/verify.ts
function targetFilesForRun(db2, runId) {
  return db2.prepare("SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path").all(runId).map((row) => String(row.file_path));
}
function closeRunFiles(db2, runId, now) {
  db2.prepare("DELETE FROM locks WHERE run_id = ?").run(runId);
  db2.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
    WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, runId);
}
function abandonLinkedTask(db2, runId, agentId2, now, message) {
  const linked = db2.prepare("SELECT task_id FROM task_runs WHERE run_id = ?").get(runId);
  if (!linked?.task_id) return;
  const updated = db2.prepare(`UPDATE tasks SET status = 'FAILED', updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status IN ('IN_PROGRESS', 'VERIFY')`).run(now, now, linked.task_id);
  if (updated.changes === 0) return;
  db2.prepare("DELETE FROM task_claims WHERE task_id = ?").run(linked.task_id);
  db2.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, 'ABANDONED', ?, ?)`).run(`tevt_${randomUUID4().replace(/-/g, "")}`, linked.task_id, runId, agentId2, message, now);
}
function auditUnverified(db2, params = {}) {
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
  const rows = db2.prepare(
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
    target_files: targetFilesForRun(db2, r.run_id),
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at
  }));
  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db2.prepare(RUNS_UPDATE_PENDING_TO_FAILED).run(now, intent.run_id);
      closeRunFiles(db2, intent.run_id, now);
      abandonLinkedTask(db2, intent.run_id, intent.agent_id, now, "pending run abandoned by verification audit");
      try {
        db2.prepare(RUN_LOG_INSERT_ABANDONED).run(
          "evt_" + randomUUID4().replace(/-/g, ""),
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
    const staleRows = db2.prepare(
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
        target_files: targetFilesForRun(db2, r.run_id),
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
      db2.prepare(RUNS_UPDATE_ACTIVE_TO_FAILED).run(now, intent.run_id);
      closeRunFiles(db2, intent.run_id, now);
      abandonLinkedTask(db2, intent.run_id, intent.agent_id, now, "stale task run abandoned by verification audit");
      try {
        db2.prepare(RUN_LOG_INSERT_STALE_ABANDONED).run(
          "evt_" + randomUUID4().replace(/-/g, ""),
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

// src/maintenance.ts
import { spawnSync as spawnSync2 } from "node:child_process";
import { createHash as createHash2, randomUUID as randomUUID6 } from "node:crypto";
import { isAbsolute as isAbsolute3, resolve as resolve6 } from "node:path";

// src/notifications.ts
import { randomUUID as randomUUID5 } from "node:crypto";

// src/sql/sessions.ts
var SESSIONS_UPDATE_END = `UPDATE sessions SET ended_at = ?, summary = ? WHERE session_id = ? RETURNING *`;

// src/sql/signals.ts
var SIGNALS_SELECT_BASE = "SELECT n.* FROM signals n";
var SIGNALS_SELECT_LEFT_JOIN_READS = "LEFT JOIN signal_reads nr ON nr.signal_id = n.signal_id AND nr.agent_id = ?";
var SIGNALS_SELECT_ORDER_LIMIT = "ORDER BY n.created_at DESC LIMIT ?";
var SIGNAL_READS_INSERT_IGNORE = "INSERT OR IGNORE INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)";

// src/sql/refinements.ts
var COLS = "refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json, reasoning, remember, quality, state, created_at, updated_at";
var REFINEMENTS_SELECT_OPEN = `SELECT ${COLS} FROM refinements
   WHERE state IN ('open','ongoing') AND quality NOT IN ('handoff','instructions')
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;
var REFINEMENTS_SELECT_BY_WORKSPACE = `SELECT ${COLS} FROM refinements
   WHERE (workspace_path = ? OR workspace_path IS NULL)
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;

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
function getNotifications(db2, params) {
  const {
    agentId: agentId2,
    kinds = [],
    signalIds = [],
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
  if (signalIds.length > 0) {
    where.push(`n.signal_id IN (${signalIds.map(() => "?").join(",")})`);
    binds.push(...signalIds);
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
  const boundedLimit = Math.min(200, Math.max(1, Math.floor(Number.isFinite(limit) ? limit : 20)));
  const rows = db2.prepare(sql).all(...allBinds, boundedLimit);
  const signals = rows.map(rowToNotification);
  if (markRead && signals.length > 0) {
    const now = utcNow();
    const insertRead = db2.prepare(SIGNAL_READS_INSERT_IGNORE);
    for (const n of signals) {
      insertRead.run(n.signal_id, agentId2, now);
    }
  }
  return { count: signals.length, signals, unread_only: unreadOnly };
}

// src/maintenance.ts
var SESSION_CAPTURE_FILE_LIMIT = 20;
var SESSION_CAPTURE_VISIBLE_FILE_LIMIT = 10;
var SESSION_CAPTURE_RUN_DETAIL_LIMIT = 3;
var SESSION_CAPTURE_RUN_FILE_LIMIT = 3;
var SESSION_CAPTURE_TEXT_LIMIT = 120;
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
function pruneStale(db2, params = {}) {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) : params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId2 = typeof params.agent_id === "string" ? params.agent_id : typeof params.agentId === "string" ? params.agentId : null;
  const rawWorkspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const rawTarget = params.target_file ?? params.targetFile;
  const targetFiles = (Array.isArray(rawTarget) ? rawTarget : rawTarget != null ? [rawTarget] : []).map(String).filter(Boolean).map((file) => {
    const base = rawWorkspacePath ? resolve6(rawWorkspacePath) : process.cwd();
    return canonicalizePath(isAbsolute3(file) ? resolve6(file) : resolve6(base, file));
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
    staleLocks = db2.prepare(
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
  const ownsTransaction = !db2.isTransaction;
  if (ownsTransaction) db2.exec("BEGIN IMMEDIATE");
  try {
    staleLocks = db2.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds);
    if (staleLocks.length === 0) {
      if (ownsTransaction) db2.exec("COMMIT");
      return { pruned_locks: 0 };
    }
    const ph = staleLocks.map(() => "?").join(",");
    db2.prepare(`DELETE FROM locks WHERE lock_id IN (${ph})`).run(...staleLocks.map((l) => l.lock_id));
    if (ownsTransaction) db2.exec("COMMIT");
  } catch (e) {
    if (ownsTransaction) {
      try {
        db2.exec("ROLLBACK");
      } catch {
      }
    }
    throw e;
  }
  return { pruned_locks: staleLocks.length };
}
function openRefinementCount(db2, params = {}) {
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
  return db2.prepare(sql).get(...queryParams).c;
}
var BRIEFING_LABELS = ["GOTCHA", "BUG", "DECISION", "IMPROVEMENT", "ARCHITECTURE", "SECURITY"];
function notifyGet(db2, params = {}) {
  const wsPath = params.workspace ?? null;
  const artifact2 = normalizeArtifact(params.artifact);
  const format = params.format ?? "json";
  const agentId2 = String(params.agent_id ?? params.agentId ?? "agent");
  const notifyCwd = wsPath ?? params.cwd ?? process.cwd();
  const items = [];
  try {
    const inbox = getNotifications(db2, {
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
    const overrideRows = db2.prepare(
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
    const memRows = db2.prepare(
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
    const topWk = db2.prepare(
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
    const refCount = openRefinementCount(db2, { workspacePath: wsPath, artifact: artifact2, cwd: notifyCwd });
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
    if (getDeliveryFingerprint(db2, delivery) === fingerprint) {
      return { ok: true, count: 0, notifications: [] };
    }
    setDeliveryFingerprint(db2, { ...delivery, fingerprint });
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
    const result = spawnSync2("git", ["-C", workspacePath, "status", "--porcelain=v1"], {
      encoding: "utf8",
      timeout: 5e3
    });
    if (result.status !== 0) return [];
    return parseGitStatusShortLines(String(result.stdout));
  } catch {
    return [];
  }
}
function sessionCapture(db2, params = {}) {
  const agentId2 = String(params.agent_id ?? params.agentId ?? "agent");
  const reason = params.reason ? String(params.reason) : null;
  const workspaceInput = params.workspace ?? params.workspace_path ?? params.workspacePath;
  const rawWorkspacePath = typeof workspaceInput === "string" && workspaceInput.trim() ? resolve6(workspaceInput.trim()) : null;
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
  const runRows = db2.prepare(
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
  const files = [...new Set(runRows.flatMap((row) => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeRuns = runRows.filter((row) => row.status === "ACTIVE").length;
  const pendingRuns = runRows.filter((row) => row.status === "PENDING").length;
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
    consolidationOpportunities = db2.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE ${cConds.join(" AND ")}`
    ).get(...cBinds).c;
  } catch {
  }
  if (runRows.length === 0 && dirtyFiles.length === 0) {
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
  const refinementId = "ref_" + randomUUID6().replace(/-/g, "");
  const allCapturedFiles = [.../* @__PURE__ */ new Set([...files, ...dirtyFiles])];
  const capturedFiles = allCapturedFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const capturedDirtyFiles = dirtyFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const statusSummary = runRows.slice(0, SESSION_CAPTURE_RUN_DETAIL_LIMIT).map((row) => {
    const rowFiles = parseJsonList(row.files_json);
    const shownFiles = rowFiles.slice(0, SESSION_CAPTURE_RUN_FILE_LIMIT);
    const omittedFiles = rowFiles.length - shownFiles.length;
    const fileSuffix = rowFiles.length > 0 ? ` files=${shownFiles.join(", ")}${omittedFiles > 0 ? ` (+${omittedFiles} more)` : ""}` : "";
    const planSuffix = row.context_ref ? ` plan=${row.context_ref}` : "";
    return `${row.status} ${row.run_id}: ${compactText(row.rationale)}; verify=${compactText(row.test_plan)}${planSuffix}${fileSuffix}`;
  });
  const omittedRunDetails = runRows.length - statusSummary.length;
  const reasoning = [
    `Session capture for ${agentId2}${reason ? ` (${reason})` : ""}.`,
    `Unresolved runs: ${runRows.length} (${activeRuns} active, ${pendingRuns} pending).`,
    listSummary("Dirty files", dirtyFiles),
    statusSummary.length > 0 ? `Run details: ${statusSummary.join(" | ")}${omittedRunDetails > 0 ? ` | ${omittedRunDetails} more runs omitted` : ""}` : null
  ].filter(Boolean).join(" ");
  const remember = [
    `Review session handoff for ${agentId2}: ${activeRuns} active and ${pendingRuns} pending runs remain.`,
    listSummary("Touched files", allCapturedFiles),
    dirtyFiles.length > 0 ? "Check dirty git state before continuing." : null,
    pendingRuns > 0 ? "Run the recorded verification before claiming completion." : null
  ].filter(Boolean).join(" ");
  const existing = db2.prepare(
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
  db2.prepare(
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
function digest(db2, params = {}) {
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
    const wouldArchive = db2.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).get(now, ...memoryScopeBinds).c;
    const wouldPruneOld = db2.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).get(cutoff, ...memoryScopeBinds).c;
    const lockDryRun = pruneStale(db2, {
      ...workspacePath ? { workspace: workspacePath } : {},
      ...artifact2 ? { artifact: artifact2 } : {},
      expired_only: true,
      dry_run: true
    });
    const wouldPruneLocks = lockDryRun.would_prune ?? 0;
    const wouldPruneRefinements = db2.prepare(`SELECT COUNT(*) AS c FROM refinements
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
  let archiveRes = { changes: 0 };
  let deleteRes = { changes: 0 };
  let prunedLocks = 0;
  let pruneRefinementsRes = { changes: 0 };
  let ftsRebuilt = false;
  const ownsDigestTransaction = !db2.isTransaction;
  if (ownsDigestTransaction) db2.exec("BEGIN IMMEDIATE");
  try {
    archiveRes = db2.prepare(
      `UPDATE memories
       SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).run(now, now, now, ...memoryScopeBinds);
    deleteRes = db2.prepare(
      `DELETE FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).run(cutoff, ...memoryScopeBinds);
    prunedLocks = pruneStale(db2, {
      ...workspacePath ? { workspace: workspacePath } : {},
      ...artifact2 ? { artifact: artifact2 } : {},
      expired_only: true
    }).pruned_locks;
    pruneRefinementsRes = db2.prepare(
      `DELETE FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`
    ).run(handoffCutoff, doneCutoff, ...refinementScopeBinds);
    if (hasFts(db2)) {
      rebuildFts(db2);
      ftsRebuilt = true;
    }
    if (ownsDigestTransaction) db2.exec("COMMIT");
  } catch (error) {
    if (ownsDigestTransaction) {
      try {
        db2.exec("ROLLBACK");
      } catch {
      }
    }
    throw error;
  }
  if (ownsDigestTransaction) checkpointWal(db2);
  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks: prunedLocks,
    pruned_refinements: pruneRefinementsRes.changes,
    fts_rebuilt: ftsRebuilt
  };
}

// src/sessions.ts
import { randomUUID as randomUUID7 } from "node:crypto";
function endSession(db2, params) {
  const now = utcNow();
  const result = db2.prepare(SESSIONS_UPDATE_END).get(
    now,
    params.summary ?? null,
    params.sessionId
  );
  return result ?? null;
}

// src/pi-hooks.ts
import path from "node:path";
import { spawnSync as spawnSync3 } from "node:child_process";
import { randomUUID as randomUUID8 } from "node:crypto";
import { realpathSync as realpathSync2 } from "node:fs";
var _sessionStartupToken = randomUUID8().slice(0, 8);
function addPathValue(paths, value) {
  if (typeof value === "string" && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
  }
}
function addApplyPatchPaths(paths, command) {
  if (typeof command !== "string") return;
  for (const line of command.split("\n")) {
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
  const command = typeof input === "string" ? input : firstString(payload.command, payload.patch);
  if (!isWriteTool) {
    const patchPaths = [];
    addApplyPatchPaths(patchPaths, command);
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
  addApplyPatchPaths(paths, command);
  return [...new Set(paths)];
}
function canonicalPath(input) {
  const resolved = path.resolve(input);
  try {
    return realpathSync2(resolved);
  } catch {
    const missingParts = [];
    let cursor = resolved;
    while (true) {
      const parent = path.dirname(cursor);
      if (parent === cursor) return resolved;
      missingParts.unshift(path.basename(cursor));
      cursor = parent;
      try {
        return path.join(realpathSync2(cursor), ...missingParts);
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
    const result = spawnSync3("git", ["-C", dir, "rev-parse", "--abbrev-ref", "HEAD"], {
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
  return new Promise((resolve8) => {
    let raw = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      raw += chunk;
    });
    process.stdin.on("end", () => resolve8(raw));
    process.stdin.on("error", () => resolve8(raw));
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
  return canonicalizePath(resolve7(cwd, file));
}
function db() {
  return connectDb(resolveDbPath(null));
}
var HOOK_RUN_STATE_TTL_MS = 10 * 6e4;
var HOOK_RUN_STATE_LOCK_WAIT = new Int32Array(new SharedArrayBuffer(4));
var HOOK_RUN_STATE_LOCK_RETRY_MS = 10;
var HOOK_RUN_STATE_LOCK_TIMEOUT_MS = 2e3;
var HOOK_RUN_STATE_LOCK_STALE_MS = 3e4;
var HOOK_DB_RETRY_TIMEOUT_MS = 5e3;
function isHookDbBusy(error) {
  const sqlite = error;
  const message = sqlite && typeof sqlite === "object" ? `${sqlite.errstr ?? ""} ${sqlite.message ?? ""}` : String(error);
  return sqlite?.errcode === 5 || /database is (?:locked|busy)/i.test(message);
}
function withHookDbRetry(operation) {
  const deadline = Date.now() + HOOK_DB_RETRY_TIMEOUT_MS;
  for (; ; ) {
    try {
      return operation();
    } catch (error) {
      if (!isHookDbBusy(error) || Date.now() >= deadline) throw error;
      Atomics.wait(HOOK_RUN_STATE_LOCK_WAIT, 0, 0, HOOK_RUN_STATE_LOCK_RETRY_MS);
    }
  }
}
function hookRunStateDir() {
  const stateDir = join3(dirname3(resolveDbPath(null)), "hook-state", "runs");
  mkdirSync2(stateDir, { recursive: true });
  return stateDir;
}
function hookRunStateFile(key) {
  return join3(hookRunStateDir(), `${key}.json`);
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
function removeStaleHookRunStateLock(lockFile) {
  try {
    const owner = Number.parseInt(readFileSync(lockFile, "utf8"), 10);
    const staleByAge = Date.now() - statSync(lockFile).mtimeMs > HOOK_RUN_STATE_LOCK_STALE_MS;
    if (processIsAlive(owner) && !staleByAge) return false;
    unlinkSync(lockFile);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return true;
    return false;
  }
}
function withHookRunStateLock(key, operation) {
  const lockFile = `${hookRunStateFile(key)}.lock`;
  const deadline = Date.now() + HOOK_RUN_STATE_LOCK_TIMEOUT_MS;
  for (; ; ) {
    try {
      const fd = openSync(lockFile, "wx", 384);
      try {
        writeFileSync(fd, `${process.pid}
`, "utf8");
      } finally {
        closeSync(fd);
      }
      try {
        return operation();
      } finally {
        try {
          unlinkSync(lockFile);
        } catch (error) {
          if (error.code !== "ENOENT") throw error;
        }
      }
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
      if (removeStaleHookRunStateLock(lockFile)) continue;
      if (Date.now() >= deadline) {
        throw new Error(`timed out waiting for hook correlation state: ${lockFile}`);
      }
      Atomics.wait(HOOK_RUN_STATE_LOCK_WAIT, 0, 0, HOOK_RUN_STATE_LOCK_RETRY_MS);
    }
  }
}
function readHookRunEntries(key) {
  try {
    const parsed = JSON.parse(readFileSync(hookRunStateFile(key), "utf8"));
    if (!Array.isArray(parsed)) return [];
    const cutoff = Date.now() - HOOK_RUN_STATE_TTL_MS;
    return parsed.filter((entry) => {
      if (!entry || typeof entry !== "object") return false;
      const candidate = entry;
      const createdAt = typeof candidate.createdAt === "string" ? Date.parse(candidate.createdAt) : NaN;
      return typeof candidate.runId === "string" && candidate.runId.length > 0 && Array.isArray(candidate.files) && candidate.files.every((file) => typeof file === "string" && file.length > 0) && Number.isFinite(createdAt) && createdAt >= cutoff;
    });
  } catch {
    return [];
  }
}
function writeHookRunEntries(key, entries) {
  const file = hookRunStateFile(key);
  if (entries.length === 0) {
    try {
      unlinkSync(file);
    } catch {
    }
    return;
  }
  const tempFile = `${file}.${process.pid}.${Date.now()}.tmp`;
  writeFileSync(tempFile, JSON.stringify(entries, null, 2) + "\n", "utf8");
  renameSync(tempFile, file);
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
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve7(cwd),
    artifact: artifact(payload),
    event: explicitId,
    files: explicitId ? [] : files.map((file) => resolveHookPath(file, cwd)).sort()
  };
  return createHash3("sha1").update(JSON.stringify(identity)).digest("hex");
}
function recordHookRun(payload, files, cwd, runId) {
  const key = hookRunKey(payload, files, cwd);
  withHookRunStateLock(key, () => {
    const entries = readHookRunEntries(key);
    entries.push({
      runId,
      files: files.map((file) => resolveHookPath(file, cwd)),
      createdAt: (/* @__PURE__ */ new Date()).toISOString()
    });
    writeHookRunEntries(key, entries.slice(-20));
  });
}
function consumeHookRun(database, payload, files, cwd) {
  const key = hookRunKey(payload, files, cwd);
  return withHookRunStateLock(key, () => {
    const entries = readHookRunEntries(key);
    const activeEntries = entries.filter((entry2) => {
      const activeFiles = new Set(listWork(database, {
        agentId: agentId(payload),
        workspacePath: cwd,
        artifact: artifact(payload),
        runId: entry2.runId,
        activeOnly: true
      }).files.map((file) => file.file_path));
      return entry2.files.every((file) => activeFiles.has(file));
    });
    const entry = activeEntries.pop() ?? null;
    writeHookRunEntries(key, activeEntries);
    return entry?.runId ?? null;
  });
}
function activeRunForFiles(database, params) {
  const absFiles = params.files.map((file) => resolveHookPath(file, params.workspacePath));
  if (absFiles.length === 0) return null;
  const rows = listWork(database, {
    agentId: params.agentId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    activeOnly: true
  }).files.filter((entry) => params.origins.includes(entry.origin));
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
  const stateDir = join3(dirname3(resolveDbPath(null)), "hook-state", "peers");
  mkdirSync2(stateDir, { recursive: true });
  return stateDir;
}
function peerStateKey(payload, files, cwd) {
  return createHash3("sha1").update(JSON.stringify({
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve7(cwd),
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
  const stateFile = join3(peerStateDir(), `${key}.txt`);
  const fingerprint = peerFingerprint(peers);
  let previous = null;
  try {
    previous = readFileSync(stateFile, "utf8").trim();
  } catch {
  }
  if (previous === fingerprint) return;
  writeFileSync(stateFile, fingerprint, "utf8");
  if (peers.length === 0) return;
  const shown = peers.slice(0, 3).map(peerLabel).join("; ");
  const omitted = peers.length > 3 ? ` +${peers.length - 3}` : "";
  const canonicalWorkspace = canonicalizePath(cwd);
  const targets = files.slice(0, 2).map((file) => relative2(canonicalWorkspace, resolveHookPath(file, cwd)) || basename2(file)).join(",");
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
      const detail = result.conflicts.slice(0, 3).map((conflict) => `${relative2(hookWorkspace, conflict.file_path)} (${conflict.agent_id})`).join(", ");
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
  const hookWorkspace = workspace(payload) ?? process.cwd();
  let consumedRunId = null;
  let stage = "open database";
  try {
    const database = db();
    stage = "register hook agent";
    withHookDbRetry(() => registerHookAgent(database, payload, "hook:post-edit"));
    const hookAgentId = agentId(payload);
    const hookArtifact = artifact(payload);
    stage = "consume correlation";
    consumedRunId = withHookDbRetry(() => consumeHookRun(database, payload, files, hookWorkspace));
    stage = "resolve fallback run";
    const correlatedRunId = consumedRunId ?? withHookDbRetry(() => activeTaskClaimForAgent(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact
    }))?.run_id ?? withHookDbRetry(() => activeRunForFiles(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      files,
      origins: ["WORK", "HOOK"]
    }));
    if (!correlatedRunId) {
      console.error("octocode-awareness post-edit warning (continuing): could not identify a unique work run; leaving presence for expiry.");
      return 0;
    }
    stage = "read run origin";
    const origin = withHookDbRetry(() => runOrigin(database, correlatedRunId));
    stage = "finish work lifecycle";
    if (origin === "HOOK") {
      withHookDbRetry(() => endWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files
      }));
    } else {
      withHookDbRetry(() => touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
        ttlMs: 10 * 6e4
      }));
    }
    consumedRunId = null;
    stage = "write edit log";
    for (const file of files) {
      withHookDbRetry(() => insertEditLog(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        filePath: resolveHookPath(file, hookWorkspace),
        operation: "update",
        workspacePath: hookWorkspace,
        artifact: hookArtifact
      }));
    }
  } catch (error) {
    if (consumedRunId) {
      try {
        recordHookRun(payload, files, hookWorkspace, consumedRunId);
      } catch {
      }
    }
    console.error(`octocode-awareness post-edit warning during ${stage} (continuing): ${error instanceof Error ? error.message : String(error)}`);
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
  const memoryHome2 = dirname3(resolveDbPath(null));
  const markerPath = join3(memoryHome2, ".last-digest-epoch-ms");
  try {
    const database = db();
    let last = 0;
    try {
      last = Number(readFileSync(markerPath, "utf8").trim() || 0);
    } catch {
      last = 0;
    }
    const now = Date.now();
    if (!last || now - last >= intervalMs) {
      digest(database, { workspace: workspace(payload), memoryHome: memoryHome2 });
      mkdirSync2(memoryHome2, { recursive: true });
      writeFileSync(markerPath, String(now), "utf8");
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
async function runHookCommand(command, rawPayload) {
  if (command === "help" || command === "--help" || command === "-h") {
    process.stdout.write("usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n");
    return 0;
  }
  const payload = parsePayload(rawPayload ?? await readStdin());
  switch (command) {
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
      console.error(`unknown hook command: ${command}`);
      return 1;
  }
}
async function main() {
  return runHookCommand(process.argv[2] ?? "help");
}
var isMain = process.argv[1] ? fileURLToPath(import.meta.url) === resolve7(process.argv[1]) : false;
var invokedAsHookRunner = process.argv[1] ? /^hook-runner\.(js|mjs|ts)$/.test(basename2(process.argv[1])) : false;
if (isMain && invokedAsHookRunner) {
  process.exitCode = await main();
}
export {
  runHookCommand
};
