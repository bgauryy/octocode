#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (w) => {
  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;
  console.error(w?.stack ?? String(w));
});

// bin/hook-runner.ts
import { createHash as createHash5 } from "node:crypto";
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
import { createHash as createHash2 } from "node:crypto";
import { mkdirSync } from "node:fs";
import { join as join2, resolve as resolve3, dirname as dirname2 } from "node:path";
import { homedir, platform } from "node:os";

// src/sqlite-runtime.ts
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
var AWARENESS_APPLICATION_ID = 1329812529;
var AWARENESS_SCHEMA_VERSION = 1;
var LEGACY_MAX_USER_VERSION = 3;
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
    const schemaState = inspectSchemaState(db2);
    if (schemaState === "legacy") createPreV1Backup(db2, dbPath);
    const versionRow = db2.prepare("SELECT sqlite_version() AS version").get();
    const journalMode = journalModeForSqliteVersion(versionRow.version);
    withSqliteBusyRetry(() => db2.exec(`PRAGMA journal_mode = ${journalMode}`));
    db2.exec("PRAGMA foreign_keys = ON");
    initializeDb(db2, schemaState === "legacy");
    _db = db2;
    return db2;
  } catch (error) {
    db2.close();
    throw error;
  }
}
function readSchemaIdentity(db2) {
  const application = db2.prepare("PRAGMA application_id").get();
  const version = db2.prepare("PRAGMA user_version").get();
  const relations = db2.prepare(`
    SELECT name, type
    FROM sqlite_schema
    WHERE type IN ('table', 'view')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
      AND name NOT GLOB 'memory_fts_*'
    ORDER BY name
  `).all();
  return {
    applicationId: application.application_id ?? 0,
    userVersion: version.user_version ?? 0,
    relations
  };
}
function hasColumns(db2, table, columns) {
  if (!tableExists(db2, table)) return false;
  const actual = tableColumns(db2, table);
  return columns.every((column) => actual.has(column));
}
function recognizedLegacySignature(db2) {
  const matchedTables = /* @__PURE__ */ new Set();
  for (const [table, columns] of [
    ["sessions", ["session_id", "agent_id", "started_at"]],
    ["memories", ["memory_id", "agent_id", "task_context", "observation", "importance"]],
    ["tasks", ["task_id", "agent_id", "rationale", "test_plan", "status"]],
    ["task_runs", ["run_id", "agent_id", "rationale", "test_plan", "status"]],
    ["locks", ["lock_id", "file_path", "acquired_at"]],
    ["refinements", ["refinement_id", "agent_id", "reasoning", "remember", "state"]],
    ["agent_intents", ["intent_id", "agent_id", "rationale", "test_plan", "status"]],
    ["intent_events", ["event_id", "intent_id", "agent_id", "event_type"]],
    ["agent_memories", ["memory_id", "agent_id", "task_context", "observation", "importance_score"]]
  ]) {
    if (hasColumns(db2, table, columns)) matchedTables.add(table);
  }
  for (const [table, columns] of canonicalColumns()) {
    if (columns.length >= 2 && hasColumns(db2, table, columns.slice(0, 2).map(({ name }) => name))) {
      matchedTables.add(table);
    }
  }
  return matchedTables.size >= 2;
}
function inspectSchemaState(db2) {
  const identity = readSchemaIdentity(db2);
  if (identity.applicationId === AWARENESS_APPLICATION_ID) {
    if (identity.userVersion !== AWARENESS_SCHEMA_VERSION) {
      throw new Error(
        `unsupported canonical Awareness schema version ${identity.userVersion}; expected ${AWARENESS_SCHEMA_VERSION}`
      );
    }
    assertCanonicalRelationContract(db2);
    assertCanonicalSchemaFingerprint(db2);
    return "canonical";
  }
  if (identity.applicationId !== 0) {
    throw new Error(
      `refusing foreign Awareness application_id ${identity.applicationId}; expected ${AWARENESS_APPLICATION_ID}`
    );
  }
  if (identity.userVersion > LEGACY_MAX_USER_VERSION) {
    throw new Error(
      `refusing unsupported unbranded Awareness schema version ${identity.userVersion}; legacy versions are 0-${LEGACY_MAX_USER_VERSION}`
    );
  }
  if (identity.relations.length === 0) {
    if (identity.userVersion === 0) return "fresh";
    throw new Error(`refusing unrelated empty versioned SQLite store at user_version ${identity.userVersion}`);
  }
  const known = /* @__PURE__ */ new Set([
    ...canonicalColumns().keys(),
    ...LEGACY_V0_RELATION_NAMES,
    "memories_fts"
  ]);
  const unexpected = identity.relations.filter(({ name, type }) => type !== "table" || !known.has(name));
  if (unexpected.length > 0 || !recognizedLegacySignature(db2)) {
    const suffix = unexpected.length > 0 ? `; unexpected relations: ${unexpected.map(({ name }) => name).join(", ")}` : "";
    throw new Error(`refusing unrecognized or unrelated SQLite store${suffix}`);
  }
  return "legacy";
}
function createPreV1Backup(db2, dbPath) {
  if (dbPath === ":memory:") return null;
  const stamp = (/* @__PURE__ */ new Date()).toISOString().replace(/[-:.TZ]/g, "");
  const backupPath = `${resolve3(dbPath)}.pre-v1-${stamp}-${process.pid}.sqlite3`;
  db2.exec(`VACUUM INTO '${backupPath.replace(/'/g, "''")}'`);
  return backupPath;
}
function assertDatabaseIntegrity(db2) {
  const integrity = db2.prepare("PRAGMA integrity_check").all();
  const failures = integrity.filter(({ integrity_check }) => integrity_check !== "ok");
  if (failures.length > 0) {
    throw new Error(`canonical v1 integrity_check failed: ${failures.map((row) => row.integrity_check).join("; ")}`);
  }
  const foreignKeys = db2.prepare("PRAGMA foreign_key_check").all();
  if (foreignKeys.length > 0) {
    throw new Error(`canonical v1 foreign_key_check failed with ${foreignKeys.length} row(s)`);
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
var SCHEMA_INDEX_DDL = `
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
var FTS_SCHEMA_DDL = `
  CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
  USING fts5(memory_id UNINDEXED, task_context, observation, tags)
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
function expectCopied(changes, expected, relation) {
  if (Number(changes) !== expected) {
    throw new Error(`schema migration copied ${String(changes)}/${expected} rows from ${relation}`);
  }
}
function legacySqlValue(value, field) {
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "bigint") {
    return value;
  }
  if (value instanceof Uint8Array) return value;
  throw new Error(`schema migration cannot bind legacy field ${field}`);
}
function legacySqlValues(values) {
  return values.map(([value, field]) => legacySqlValue(value, field));
}
function migrateLegacyV0Relations(db2) {
  if (tableExists(db2, "agent_memories")) {
    const rows = db2.prepare("SELECT * FROM agent_memories").all();
    const insert = db2.prepare(`INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance, state, label,
      superseded_by, tags_json, workspace_path, artifact, repo, ref,
      file_tree_fingerprint, novelty_score, last_accessed_at, access_count,
      decay_half_life_days, failure_signature, valid_from, valid_to, expired_at,
      embedding, embedding_model, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`);
    const insertRef = db2.prepare(`INSERT INTO memory_refs(memory_id, reference, kind, ordinal)
      VALUES (?, ?, 'file', 0)`);
    for (const row of rows) {
      insert.run(...legacySqlValues([
        [row["memory_id"], "agent_memories.memory_id"],
        [row["agent_id"], "agent_memories.agent_id"],
        [row["task_context"], "agent_memories.task_context"],
        [row["observation"], "agent_memories.observation"],
        [row["importance_score"], "agent_memories.importance_score"],
        [row["state"] ?? "ACTIVE", "agent_memories.state"],
        [row["label"] ?? "OTHER", "agent_memories.label"],
        [row["superseded_by"] ?? null, "agent_memories.superseded_by"],
        [row["tags_json"] ?? "[]", "agent_memories.tags_json"],
        [row["file_tree_fingerprint"] ?? null, "agent_memories.file_tree_fingerprint"],
        [row["last_accessed_at"] ?? null, "agent_memories.last_accessed_at"],
        [row["access_count"] ?? 0, "agent_memories.access_count"],
        [row["decay_half_life_days"] ?? null, "agent_memories.decay_half_life_days"],
        [row["failure_signature"] ?? null, "agent_memories.failure_signature"],
        [row["valid_from"] ?? row["created_at"], "agent_memories.valid_from"],
        [row["valid_to"] ?? null, "agent_memories.valid_to"],
        [row["expired_at"] ?? null, "agent_memories.expired_at"],
        [row["embedding"] ?? null, "agent_memories.embedding"],
        [row["embedding_model"] ?? null, "agent_memories.embedding_model"],
        [row["created_at"], "agent_memories.created_at"],
        [row["updated_at"] ?? null, "agent_memories.updated_at"]
      ]));
      if (typeof row["file"] === "string" && row["file"].trim()) {
        insertRef.run(
          legacySqlValue(row["memory_id"], "agent_memories.memory_id"),
          `file:${row["file"]}`
        );
      }
    }
  }
  if (tableExists(db2, "agent_intents")) {
    const rows = db2.prepare("SELECT * FROM agent_intents").all();
    const insertRun = db2.prepare(`INSERT INTO task_runs (
      run_id, task_id, origin, agent_id, session_id, rationale, test_plan,
      context_ref, status, workspace_path, artifact, created_at, updated_at
    ) VALUES (?, NULL, 'WORK', ?, NULL, ?, ?, ?, ?, ?, NULL, ?, ?)`);
    const insertFile = db2.prepare(`INSERT INTO run_files (
      run_id, file_path, reason_override, source, started_at, heartbeat_at,
      expires_at, ended_at
    ) VALUES (?, ?, NULL, 'EXPLICIT', ?, ?, ?, ?)`);
    for (const row of rows) {
      insertRun.run(...legacySqlValues([
        [row["intent_id"], "agent_intents.intent_id"],
        [row["agent_id"], "agent_intents.agent_id"],
        [row["rationale"], "agent_intents.rationale"],
        [row["test_plan"], "agent_intents.test_plan"],
        [row["plan_doc_ref"] ?? null, "agent_intents.plan_doc_ref"],
        [row["status"], "agent_intents.status"],
        [row["workspace_path"] ?? null, "agent_intents.workspace_path"],
        [row["created_at"], "agent_intents.created_at"],
        [row["updated_at"], "agent_intents.updated_at"]
      ]));
      for (const filePath of parseJsonList(String(row["files_json"] ?? "[]"))) {
        const updatedAt = String(row["updated_at"]);
        insertFile.run(...legacySqlValues([
          [row["intent_id"], "agent_intents.intent_id"],
          [filePath, "agent_intents.files_json[]"],
          [row["created_at"], "agent_intents.created_at"],
          [updatedAt, "agent_intents.updated_at"],
          [updatedAt, "agent_intents.updated_at"],
          [row["status"] === "ACTIVE" ? null : updatedAt, "agent_intents.ended_at"]
        ]));
      }
    }
  }
  if (tableExists(db2, "file_locks")) {
    const expected = db2.prepare("SELECT COUNT(*) AS count FROM file_locks").get().count;
    const result = db2.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
      SELECT f.lock_id, f.file_path, f.intent_id, f.acquired_at, f.expires_at
      FROM file_locks f JOIN task_runs r ON r.run_id = f.intent_id`).run();
    expectCopied(result.changes, expected, "file_locks");
  }
  if (tableExists(db2, "intent_events")) {
    const expected = db2.prepare("SELECT COUNT(*) AS count FROM intent_events").get().count;
    const result = db2.prepare(`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
      SELECT e.event_id, e.intent_id, e.agent_id, e.event_type, e.message, e.created_at
      FROM intent_events e LEFT JOIN task_runs r ON r.run_id = e.intent_id`).run();
    expectCopied(result.changes, expected, "intent_events");
  }
  if (tableExists(db2, "task_log")) {
    const columns = tableColumns(db2, "task_log");
    const runColumn = columns.has("run_id") ? "run_id" : "task_id";
    const expected = db2.prepare("SELECT COUNT(*) AS count FROM task_log").get().count;
    const result = db2.prepare(`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
      SELECT event_id, ${runColumn}, agent_id, event_type, message, created_at FROM task_log`).run();
    expectCopied(result.changes, expected, "task_log");
  }
  if (tableExists(db2, "notifications")) {
    const expected = db2.prepare("SELECT COUNT(*) AS count FROM notifications").get().count;
    const result = db2.prepare(`INSERT INTO signals (
      signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind,
      subject, body, files_json, refs_json, thread_id, reply_to, importance,
      status, resolved_at, created_at
    ) SELECT notification_id, workspace_path, NULL, repo, ref, from_agent, to_agent,
      kind, subject, body, files_json, refs_json, thread_id, in_reply_to,
      importance, status, CASE WHEN status = 'resolved' THEN created_at ELSE NULL END,
      created_at FROM notifications`).run();
    expectCopied(result.changes, expected, "notifications");
  }
  if (tableExists(db2, "notification_reads")) {
    const expected = db2.prepare("SELECT COUNT(*) AS count FROM notification_reads").get().count;
    const result = db2.prepare(`INSERT INTO signal_reads(signal_id, agent_id, read_at)
      SELECT r.notification_id, r.agent_id, r.read_at
      FROM notification_reads r JOIN signals s ON s.signal_id = r.notification_id`).run();
    expectCopied(result.changes, expected, "notification_reads");
  }
  for (const relation of [
    "notification_reads",
    "notifications",
    "file_locks",
    "intent_events",
    "agent_intents",
    "agent_memories",
    "task_log",
    "memory_fts"
  ]) {
    db2.exec(`DROP TABLE IF EXISTS ${relation}`);
  }
}
function repairLegacyForeignKeyReferences(db2) {
  db2.exec(`INSERT OR IGNORE INTO sessions (
    session_id, agent_id, workspace_path, artifact, repo, ref,
    started_at, ended_at, summary
  ) SELECT session_id, agent_id, workspace_path, artifact, NULL, NULL,
      created_at, CASE WHEN status = 'ACTIVE' THEN NULL ELSE updated_at END,
      'migrated legacy session reference'
    FROM task_runs
    WHERE session_id IS NOT NULL
    ORDER BY created_at, run_id`);
  db2.exec(`UPDATE task_runs
    SET context_ref = COALESCE(context_ref, 'legacy-task:' || task_id), task_id = NULL
    WHERE task_id IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM tasks WHERE tasks.task_id = task_runs.task_id
    )`);
}
function mainDatabasePath(db2) {
  const row = db2.prepare("PRAGMA database_list").all().find(({ name }) => name === "main");
  return row?.file?.trim() || null;
}
function initializeDb(db2, fileBackupCreated) {
  const state = inspectSchemaState(db2);
  if (state === "canonical") {
    if (!db2.isTransaction) db2.exec("PRAGMA foreign_keys = ON");
    return;
  }
  if (db2.isTransaction) {
    throw new Error("cannot initialize or migrate canonical v1 inside a caller-owned transaction");
  }
  if (state === "legacy" && mainDatabasePath(db2) && !fileBackupCreated) {
    throw new Error("file-backed legacy migration requires connectDb(path) so a pre-v1 backup is created");
  }
  db2.exec("PRAGMA foreign_keys = OFF");
  let began = false;
  try {
    withSqliteBusyRetry(() => db2.exec("BEGIN IMMEDIATE"));
    began = true;
    const lockedState = inspectSchemaState(db2);
    if (lockedState !== "canonical") initDbSchema(db2, lockedState);
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
function initDbSchema(db2, state) {
  migrateLegacyTaskRuns(db2);
  db2.exec(SCHEMA_DDL);
  migrateExistingTables(db2);
  migrateLegacyExecutionSchema(db2);
  migrateRefinementQualityConstraint(db2);
  migrateCheckConstraints(db2);
  migrateLegacyV0Relations(db2);
  repairLegacyForeignKeyReferences(db2);
  if (state === "legacy") {
    db2.exec("DROP TABLE IF EXISTS memories_fts");
    rebuildAllCanonicalTables(db2);
  }
  db2.exec(SCHEMA_INDEX_DDL);
  try {
    db2.exec(FTS_SCHEMA_DDL);
  } catch {
  }
  if (hasFts(db2)) {
    const row = db2.prepare("SELECT COUNT(*) AS cnt FROM memories_fts").get();
    if (row.cnt === 0) rebuildFts(db2);
  }
  assertCanonicalRelationContract(db2);
  assertCanonicalSchemaFingerprint(db2);
  assertDatabaseIntegrity(db2);
  db2.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
  db2.exec(`PRAGMA user_version = ${AWARENESS_SCHEMA_VERSION}`);
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
function migrateLegacyExecutionSchema(db2) {
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
function normalizeSchemaSql(sql) {
  return sql.replace(/--[^\n]*/g, " ").replace(/["`\[\]]/g, "").replace(/\bIF\s+NOT\s+EXISTS\b/gi, "").replace(/\s+/g, " ").replace(/\s*([(),])\s*/g, "$1").trim().toLowerCase();
}
function readSchemaObjects(db2) {
  const rows = db2.prepare(`
    SELECT type, name, tbl_name, sql
    FROM sqlite_schema
    WHERE type IN ('table', 'view', 'index', 'trigger')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
      AND name NOT GLOB 'memory_fts_*'
    ORDER BY type, name
  `).all();
  return rows.map((row) => ({
    type: row.type,
    name: row.name,
    tableName: row.tbl_name,
    sql: normalizeSchemaSql(row.sql ?? "")
  }));
}
function schemaObjectsFingerprint(objects) {
  return createHash2("sha256").update(JSON.stringify(objects)).digest("hex");
}
var _canonicalSchemaFingerprints = /* @__PURE__ */ new Map();
function canonicalSchemaFingerprint(includeFts) {
  const cached = _canonicalSchemaFingerprints.get(includeFts);
  if (cached) return cached;
  const canonical = new DatabaseSync(":memory:");
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
function assertCanonicalRelationContract(db2) {
  const actualRows = readSchemaIdentity(db2).relations;
  const expected = new Set(canonicalColumns().keys());
  const actual = new Set(actualRows.map(({ name }) => name));
  const missing = [...expected].filter((name) => !actual.has(name));
  const unexpected = actualRows.filter(({ name, type }) => type !== "table" || !expected.has(name) && name !== "memories_fts");
  if (missing.length === 0 && unexpected.length === 0) return;
  const details = [
    missing.length > 0 ? `missing: ${missing.join(", ")}` : null,
    unexpected.length > 0 ? `unexpected: ${unexpected.map(({ name }) => name).join(", ")}` : null
  ].filter((value) => value !== null).join("; ");
  throw new Error(`canonical v1 relation contract mismatch (${details})`);
}
function assertCanonicalSchemaFingerprint(db2) {
  const objects = readSchemaObjects(db2);
  const includeFts = objects.some(({ type, name }) => type === "table" && name === "memories_fts");
  const expectedFingerprint = canonicalSchemaFingerprint(includeFts);
  const actualFingerprint = schemaObjectsFingerprint(objects);
  if (actualFingerprint !== expectedFingerprint) {
    throw new Error(
      `canonical v1 schema fingerprint mismatch (expected ${expectedFingerprint}, got ${actualFingerprint})`
    );
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
function rebuildAllCanonicalTables(db2) {
  for (const [table, sql] of canonicalTableSql()) {
    if (tableExists(db2, table)) rebuildTableFromCanonical(db2, table, sql);
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
import { randomUUID as randomUUID3 } from "node:crypto";
import { isAbsolute, relative, resolve as resolve4, sep } from "node:path";

// src/sessions.ts
import { randomUUID as randomUUID2 } from "node:crypto";

// src/sql/sessions.ts
var SESSIONS_INSERT = `INSERT INTO sessions (session_id, agent_id, workspace_path, artifact, repo, ref, started_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)`;
var SESSIONS_SELECT_BY_ID = `SELECT session_id, agent_id, workspace_path, artifact, repo, ref, started_at, ended_at, summary
   FROM sessions WHERE session_id = ?`;

// src/sessions.ts
function scopedWorkspacePath(workspacePath) {
  return workspacePath ? normalizeWorkspacePath(workspacePath, workspacePath) : null;
}
function ensureRunSession(db2, params) {
  const sessionId2 = params.sessionId.trim();
  if (!sessionId2) throw new Error("session id is required");
  const workspacePath = scopedWorkspacePath(params.workspacePath);
  const artifact2 = normalizeArtifact(params.artifact);
  const existing = getSession(db2, sessionId2);
  if (existing) {
    if (existing.agent_id !== params.agentId) {
      throw new Error(`session ${sessionId2} belongs to agent ${existing.agent_id}`);
    }
    if (existing.workspace_path !== workspacePath) {
      throw new Error(`session ${sessionId2} belongs to workspace ${existing.workspace_path ?? "(none)"}`);
    }
    if (existing.artifact !== artifact2) {
      throw new Error(`session ${sessionId2} belongs to artifact ${existing.artifact ?? "(none)"}`);
    }
    if (existing.ended_at != null) {
      throw new Error(`session ${sessionId2} has already ended`);
    }
    return existing;
  }
  const now = utcNow();
  db2.prepare(SESSIONS_INSERT).run(
    sessionId2,
    params.agentId,
    workspacePath,
    artifact2,
    null,
    null,
    now
  );
  return getSession(db2, sessionId2);
}
function endSession(db2, params) {
  const now = utcNow();
  const where = ["session_id = ?", "agent_id = ?", "ended_at IS NULL"];
  const binds = [params.sessionId, params.agentId];
  if (params.workspacePath !== void 0) {
    where.push("workspace_path IS ?");
    binds.push(scopedWorkspacePath(params.workspacePath));
  }
  if (params.artifact !== void 0) {
    where.push("artifact IS ?");
    binds.push(normalizeArtifact(params.artifact));
  }
  const result = db2.prepare(
    `UPDATE sessions SET ended_at = ?, summary = ? WHERE ${where.join(" AND ")} RETURNING *`
  ).get(now, params.summary ?? null, ...binds);
  return result ?? null;
}
function getSession(db2, sessionId2) {
  const row = db2.prepare(SESSIONS_SELECT_BY_ID).get(sessionId2);
  return row ?? null;
}

// src/tasks.ts
var DEFAULT_CLAIM_LEASE_MS = 30 * 6e4;
var MAX_CLAIM_LEASE_MS = 60 * 6e4;
function event(db2, taskId, runId, agentId2, eventType, message, now = utcNow()) {
  db2.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`).run(`tevt_${randomUUID3().replace(/-/g, "")}`, taskId, runId, agentId2, eventType, message, now);
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
import { randomUUID as randomUUID4 } from "node:crypto";
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
  let fileBasePath = params.workspacePath ?? process.cwd();
  let wsPath = workspaceRoot(params.workspacePath);
  let artifact2 = normalizeArtifact(params.artifact);
  let runId = params.runId ?? null;
  if (!runId) {
    required(params.rationale, "rationale");
    required(params.testPlan, "test plan");
  }
  db2.exec("BEGIN IMMEDIATE");
  try {
    runId ??= `run_${randomUUID4().replace(/-/g, "")}`;
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
      fileBasePath = runWorkspace;
      if (params.sessionId != null) {
        if (params.sessionId !== run.session_id) {
          throw new Error(`run ${runId} belongs to session ${run.session_id ?? "(none)"}`);
        }
        ensureRunSession(db2, {
          sessionId: params.sessionId,
          agentId: agentId2,
          workspacePath: runWorkspace,
          artifact: runArtifact
        });
      }
    } else {
      if (params.runId) throw new Error(`run not found: ${params.runId}`);
      if (params.sessionId) {
        ensureRunSession(db2, {
          sessionId: params.sessionId,
          agentId: agentId2,
          workspacePath: wsPath,
          artifact: artifact2
        });
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
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`).run(`lock_${randomUUID4().replace(/-/g, "")}`, file, runId, now, expiresAt);
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
import { randomUUID as randomUUID5 } from "node:crypto";

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
    VALUES (?, ?, ?, ?, 'ABANDONED', ?, ?)`).run(`tevt_${randomUUID5().replace(/-/g, "")}`, linked.task_id, runId, agentId2, message, now);
}
function auditUnverified(db2, params = {}) {
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const where = ["status = 'PENDING'"];
  const binds = [];
  let ageCutoff = null;
  if (params.olderThanDays != null) {
    if (!Number.isFinite(params.olderThanDays) || params.olderThanDays < 1) {
      throw new Error("olderThanDays must be a finite number >= 1");
    }
    ageCutoff = new Date(Date.now() - Math.floor(params.olderThanDays) * 864e5).toISOString();
    where.push("updated_at < ?");
    binds.push(ageCutoff);
  }
  if (params.origins?.length) {
    const origins = [...new Set(params.origins)];
    if (origins.some((origin) => !["TASK", "WORK", "HOOK"].includes(origin))) {
      throw new Error("origins must contain only TASK, WORK, or HOOK");
    }
    where.push(`origin IN (${origins.map(() => "?").join(",")})`);
    binds.push(...origins);
  }
  let before = null;
  if (params.before) {
    const parsed = new Date(params.before);
    if (Number.isNaN(parsed.getTime())) throw new Error("before must be a valid ISO timestamp");
    before = parsed.toISOString();
    where.push("created_at < ?");
    binds.push(before);
  }
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
          "evt_" + randomUUID5().replace(/-/g, ""),
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
    if (ageCutoff) {
      staleWhere.push("ai.updated_at < ?");
      staleBinds.push(ageCutoff);
    }
    if (params.origins?.length) {
      const origins = [...new Set(params.origins)];
      staleWhere.push(`ai.origin IN (${origins.map(() => "?").join(",")})`);
      staleBinds.push(...origins);
    }
    if (before) {
      staleWhere.push("ai.created_at < ?");
      staleBinds.push(before);
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
          "evt_" + randomUUID5().replace(/-/g, ""),
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
import { createHash as createHash3, randomUUID as randomUUID8 } from "node:crypto";
import { existsSync } from "node:fs";
import { isAbsolute as isAbsolute3, resolve as resolve6 } from "node:path";

// src/memory.ts
import { randomUUID as randomUUID6 } from "node:crypto";
var DECAY_WEIGHTS = { importance: 0.25, recency: 0.3, access: 0.15, lexical: 0.3 };
var DEFAULT_HALF_LIFE_DAYS = 30;
var ACCESS_SATURATION = 50;
var BM25_SQUASH_K = 1;
var BM25_DEGENERATE_MAX = 0.01;
var JUDGMENT_RELEVANCE_FLOOR = 0.35;
var SCORING_PREFETCH_FACTOR = 3;
function canonicalMemoryInstant(value, field) {
  if (value == null) return null;
  const text = String(value).trim();
  const isoInstant = /^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2}))?$/;
  if (!isoInstant.test(text)) {
    throw new Error(`${field} must be a valid ISO 8601 timestamp`);
  }
  const parsed = new Date(text);
  if (!text || Number.isNaN(parsed.getTime())) {
    throw new Error(`${field} must be a valid ISO 8601 timestamp`);
  }
  return parsed.toISOString().replace(/\.\d{3}Z$/, "Z");
}
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
function fallbackSearch(db2, query, conditions, params, limit) {
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
  return db2.prepare(sql).all(...fallbackParams, limit);
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
function lexicalSearch(db2, query, limit, minImportance, tags, labels, states, scopeOptions = {}) {
  const ftsQuery = query ? buildFtsQuery(query) : null;
  if (query.trim() && !ftsQuery) return [];
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
  } else {
    const now = utcNow();
    conditions.push(`(m.state <> 'ACTIVE' OR (
      (m.valid_from IS NULL OR m.valid_from <= ?)
      AND (m.valid_to IS NULL OR m.valid_to > ?)
    ))`);
    params.push(now, now);
  }
  const candidateIds = scopeOptions.candidateMemoryIds ? [...new Set(scopeOptions.candidateMemoryIds)].filter(Boolean) : null;
  if (candidateIds && candidateIds.length === 0) return [];
  let usingCandidateTable = false;
  if (candidateIds) {
    if (candidateIds.length <= 400) {
      conditions.push(`m.memory_id IN (${candidateIds.map(() => "?").join(",")})`);
      params.push(...candidateIds);
    } else {
      db2.exec("CREATE TEMP TABLE IF NOT EXISTS temp_memory_candidate_ids(memory_id TEXT PRIMARY KEY)");
      db2.exec("DELETE FROM temp_memory_candidate_ids");
      const insertCandidate = db2.prepare("INSERT OR IGNORE INTO temp_memory_candidate_ids(memory_id) VALUES (?)");
      for (const id of candidateIds) insertCandidate.run(id);
      conditions.push("EXISTS (SELECT 1 FROM temp_memory_candidate_ids c WHERE c.memory_id = m.memory_id)");
      usingCandidateTable = true;
    }
  }
  let rows;
  try {
    if (ftsQuery && hasFts(db2)) {
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
        rows = db2.prepare(sql).all(ftsQuery, ...params, limit);
      } catch {
        rows = fallbackSearch(db2, query, conditions, params, limit);
      }
    } else {
      rows = fallbackSearch(db2, query, conditions, params, limit);
    }
  } finally {
    if (usingCandidateTable) {
      try {
        db2.exec("DELETE FROM temp_memory_candidate_ids");
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
function attachMemoryReferences(db2, memories) {
  if (memories.length === 0) return;
  try {
    const ids = [...new Set(memories.map((m) => m.memory_id))];
    const ph = ids.map(() => "?").join(",");
    const rows = db2.prepare(
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
function exactReferenceCandidateIds(db2, references) {
  const refs = normalizeReferences(references);
  if (refs.length === 0) return /* @__PURE__ */ new Set();
  const rows = db2.prepare(
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
function anyReferenceCandidateIds(db2, references) {
  const refs = [...new Set(references.map((ref) => String(ref ?? "").trim().slice(0, 512)).filter(Boolean))];
  if (refs.length === 0) return /* @__PURE__ */ new Set();
  const rows = db2.prepare(
    `SELECT DISTINCT memory_id
     FROM memory_refs
     WHERE reference IN (${refs.map(() => "?").join(",")})`
  ).all(...refs);
  return new Set(rows.map((row) => row.memory_id));
}
function fileRegexCandidateIds(db2, regexes) {
  if (regexes.length === 0) return /* @__PURE__ */ new Set();
  const rows = db2.prepare(
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
function regexCandidateIds(db2, regexes) {
  if (regexes.length === 0) return /* @__PURE__ */ new Set();
  const rows = db2.prepare(
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
function bumpAccess(db2, memoryIds) {
  if (memoryIds.length === 0) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => "?").join(",");
  db2.prepare(`
    UPDATE memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${placeholders})
  `).run(now, ...memoryIds);
}
function getMemory(db2, params = {}) {
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
    candidateMemoryIds = [],
    recordAccess = true,
    cwd: cwdParam
  } = params;
  const limitCap = candidateMemoryIds.length > 0 ? 2e3 : 50;
  const limit = Math.min(limitCap, Math.max(1, Number(limitRaw) || 3));
  const smartEnabled = smart === true || smart === "true";
  let minImportance = Math.max(1, Number(minImpRaw) || 1);
  let smartExpanded = false;
  const droppedSmartFilters = [];
  if (smartEnabled && minImportance > 1) {
    minImportance = Math.max(1, minImportance - 1);
    smartExpanded = true;
    droppedSmartFilters.push("min_importance");
  }
  const states = statesRaw ?? (asOf ? ["ACTIVE", "SUPERSEDED"] : ["ACTIVE"]);
  const labels = label ? Array.isArray(label) ? label.map((value) => normalizeLabel(value)) : [normalizeLabel(label)] : [];
  const effectiveCwd = cwdParam ?? workspacePath ?? void 0;
  const normalizedAsOf = canonicalMemoryInstant(asOf, "as_of");
  const asOfDate = normalizedAsOf ? new Date(normalizedAsOf) : null;
  if (asOfDate && isNaN(asOfDate.getTime())) {
    throw new Error(`invalid --as-of value "${asOf}" \u2014 expected ISO 8601 date string (e.g. 2024-06-01T00:00:00Z)`);
  }
  let candidateIds = candidateMemoryIds.length > 0 ? new Set(candidateMemoryIds.filter(Boolean)) : null;
  const refFilters = normalizeReferences(references);
  const fileRefFilters = fileReferenceCandidates(files, effectiveCwd);
  const compiledRegex = regex.map(compileRecallRegex);
  const compiledFileRegex = fileRegex.map(compileRecallRegex);
  if (refFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, exactReferenceCandidateIds(db2, refFilters));
  }
  if (fileRefFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, anyReferenceCandidateIds(db2, fileRefFilters));
  }
  if (compiledFileRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, fileRegexCandidateIds(db2, compiledFileRegex));
  }
  if (compiledRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, regexCandidateIds(db2, compiledRegex));
  }
  const scopeOptions = {
    workspacePath: workspacePath ?? cwdParam ?? null,
    artifact: artifact2,
    repo: repoArg,
    ref: refArg,
    strictScope,
    globalOnly,
    cwd: cwdParam,
    asOf: normalizedAsOf,
    candidateMemoryIds: candidateIds ? [...candidateIds] : void 0
  };
  let memories = lexicalSearch(
    db2,
    query,
    limit * SCORING_PREFETCH_FACTOR,
    minImportance,
    tags,
    labels,
    states,
    {
      ...scopeOptions
    }
  );
  if (smartEnabled && memories.length < limit && (labels.length > 0 || tags.length > 0 || minImportance > 1)) {
    if (labels.length > 0 && !droppedSmartFilters.includes("label")) droppedSmartFilters.push("label");
    if (tags.length > 0 && !droppedSmartFilters.includes("tag")) droppedSmartFilters.push("tag");
    if (minImportance > 1 && !droppedSmartFilters.includes("min_importance")) droppedSmartFilters.push("min_importance");
    const expanded = lexicalSearch(
      db2,
      query,
      limit * SCORING_PREFETCH_FACTOR,
      1,
      [],
      [],
      states,
      scopeOptions
    );
    const byId = new Map(memories.map((memory) => [memory.memory_id, memory]));
    for (const memory of expanded) byId.set(memory.memory_id, memory);
    memories = [...byId.values()];
    smartExpanded = true;
  }
  attachMemoryReferences(db2, memories);
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
  if (recordAccess) bumpAccess(db2, memories.map((m) => m.memory_id));
  const mode = hasFts(db2) ? "lexical" : "fallback";
  const result = {
    count: memories.length,
    memories,
    mode,
    sort,
    as_of: normalizedAsOf,
    global_only: Boolean(globalOnly),
    states,
    ...smartExpanded ? {
      smart_expanded: true,
      smart_dropped_filters: droppedSmartFilters
    } : {}
  };
  if (query.trim()) {
    const topRelevance = memories[0]?.lexical ?? 0;
    if (memories.length === 0) {
      result.judgment_required = true;
      result.judgment_reason = smartEnabled ? "no results after smart widening \u2014 absence of recall is not proof of absence; broaden the query terms or scope" : "no results \u2014 absence of recall is not proof of absence; retry with --smart or broader terms";
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

// src/notifications.ts
import { randomUUID as randomUUID7 } from "node:crypto";

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
function isBroadcastThread(db2, threadId) {
  return db2.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND reply_to IS NULL AND to_agent IS NULL
    LIMIT 1`).get(threadId) != null;
}
function isThreadParticipant(db2, threadId, agentId2) {
  const addressed = db2.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND (from_agent = ? OR to_agent = ?)
    LIMIT 1`).get(threadId, agentId2, agentId2) != null;
  if (addressed) return true;
  if (!isBroadcastThread(db2, threadId)) return false;
  return db2.prepare(`SELECT 1 FROM signal_reads read
    JOIN signals signal ON signal.signal_id = read.signal_id
    WHERE signal.thread_id = ? AND read.agent_id = ?
    LIMIT 1`).get(threadId, agentId2) != null;
}
function canReadOrJoinThread(db2, threadId, agentId2) {
  return isBroadcastThread(db2, threadId) || isThreadParticipant(db2, threadId, agentId2);
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
  if (threadId && !canReadOrJoinThread(db2, threadId, agentId2)) {
    return { count: 0, signals: [], unread_only: unreadOnly };
  }
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
    return {
      pruned_locks: 0,
      dry_run: true,
      would_prune: staleLocks.length,
      lock_ids: staleLocks.map((lock) => lock.lock_id).slice(0, 20)
    };
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
var INTERVENTION_CANDIDATE_LIMIT = 50;
var HOOK_BRIEF_ITEM_MAX_BYTES = 180;
var INTERVENTION_STOP_WORDS = /* @__PURE__ */ new Set([
  "the",
  "and",
  "for",
  "with",
  "from",
  "into",
  "this",
  "that",
  "about",
  "before",
  "after",
  "fix",
  "update",
  "change",
  "make",
  "during"
]);
function interventionTokens(text) {
  return new Set(
    (text.toLowerCase().match(/[a-z0-9]{3,}/g) ?? []).filter((token) => !INTERVENTION_STOP_WORDS.has(token))
  );
}
function summarizeUtf8(value, maxBytes) {
  const flat = value.replace(/\s+/g, " ").trim();
  if (Buffer.byteLength(flat, "utf8") <= maxBytes) return flat;
  const suffix = "...";
  const suffixBytes = Buffer.byteLength(suffix, "utf8");
  let bytes = 0;
  let output = "";
  for (const character of flat) {
    const characterBytes = Buffer.byteLength(character, "utf8");
    if (bytes + characterBytes + suffixBytes > maxBytes) break;
    output += character;
    bytes += characterBytes;
  }
  return output.trimEnd() + suffix;
}
function isPromptGroundedMemory(query, memory) {
  const queryTokens = interventionTokens(query);
  if (queryTokens.size < 2) return false;
  const memoryTokens = interventionTokens([
    memory.task_context,
    memory.observation,
    memory.label,
    memory.failure_signature ?? ""
  ].join(" "));
  let overlap = 0;
  for (const token of queryTokens) {
    if (memoryTokens.has(token) && ++overlap >= 2) return true;
  }
  return false;
}
function notifyGet(db2, params = {}) {
  const wsPath = params.workspace ?? null;
  const artifact2 = normalizeArtifact(params.artifact);
  const format = params.format ?? "json";
  const interventionQuery = String(params.query ?? "").trim().slice(0, 4e3);
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
      const fileSuffix = n.files.length > 0 ? ` files=${n.files.length}[${summarizeText(n.files[0], 48)}]` : "";
      const bodySuffix = n.body ? ` \u2014 ${summarizeText(n.body, 60)}` : "";
      items.push({
        kind: "notification",
        text: `\u{1F4E8} ${n.kind} from ${n.from_agent} (${target})${fileSuffix}: ${summarizeText(n.subject, 72)}${bodySuffix}`,
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
    let memRows = [];
    if (format === "hook") {
      if (interventionQuery) {
        const recall = getMemory(db2, {
          query: interventionQuery,
          // Grounding is stricter than retrieval. Inspect the full normal recall
          // budget so high-importance one-token hits cannot starve a lower-ranked
          // memory that satisfies the two-token intervention gate.
          limit: INTERVENTION_CANDIDATE_LIMIT,
          minImportance: 6,
          label: [...BRIEFING_LABELS],
          workspacePath: wsPath,
          artifact: artifact2,
          repo: params.repo ?? null,
          ref: params.ref ?? null,
          recordAccess: false,
          cwd: notifyCwd
        });
        const selected = recall.memories.find((memory) => isPromptGroundedMemory(interventionQuery, memory));
        if (selected) memRows = [selected];
      }
    } else {
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
      memRows = db2.prepare(
        `SELECT memory_id, task_context, observation, label, importance, failure_signature
         FROM memories
         WHERE ${conditions.join(" AND ")}
         ORDER BY importance DESC, last_accessed_at DESC
         LIMIT 3`
      ).all(...bindParams);
    }
    for (const m of memRows) {
      items.push({
        kind: "memory",
        text: `Memory lead \u2014 verify: ${m.label}(${m.importance}): ${m.observation.slice(0, 120)}`,
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
    const hookItems = items.slice(0, 5).map((item) => ({
      ...item,
      text: summarizeUtf8(item.text, HOOK_BRIEF_ITEM_MAX_BYTES)
    }));
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
    const fingerprint = createHash3("sha256").update(additionalContext).digest("hex");
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
  const refinementId = "ref_" + randomUUID8().replace(/-/g, "");
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
var MIN_RETENTION_DAYS = 1;
var MAX_RETENTION_DAYS = 3650;
function retentionWindow(params, snakeName, camelName, fallback) {
  const raw = params[snakeName] ?? params[camelName] ?? fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < MIN_RETENTION_DAYS || value > MAX_RETENTION_DAYS) {
    throw new Error(`${snakeName} must be an integer in ${MIN_RETENTION_DAYS}..${MAX_RETENTION_DAYS}`);
  }
  return value;
}
function inspectMaintenancePressure(db2, params = {}) {
  const requestedDays = Number(params.pressure_age_days ?? params.pressureAgeDays ?? 1);
  const pressureAgeDays = Number.isFinite(requestedDays) ? Math.min(3650, Math.max(1, Math.floor(requestedDays))) : 1;
  const cutoff = new Date(Date.now() - pressureAgeDays * 864e5).toISOString();
  const rawWorkspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? params.workspace_normalized === true ? resolve6(rawWorkspacePath) : normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const scope = [];
  const scopeBinds = [];
  if (workspacePath) {
    scope.push("workspace_path = ?");
    scopeBinds.push(workspacePath);
  }
  if (artifact2) {
    scope.push("artifact = ?");
    scopeBinds.push(artifact2);
  }
  const scopeSql = scope.length > 0 ? ` AND ${scope.join(" AND ")}` : "";
  const pendingCount = db2.prepare(
    `SELECT COUNT(*) AS count FROM task_runs
      WHERE status = 'PENDING' AND updated_at < ?${scopeSql}`
  ).get(cutoff, ...scopeBinds).count;
  const pendingRows = db2.prepare(
    `SELECT run_id FROM task_runs
      WHERE status = 'PENDING' AND updated_at < ?${scopeSql}
      ORDER BY datetime(updated_at), run_id LIMIT 3`
  ).all(cutoff, ...scopeBinds);
  const signalCount = db2.prepare(
    `SELECT COUNT(*) AS count FROM signals
      WHERE status = 'open' AND created_at < ?${scopeSql}`
  ).get(cutoff, ...scopeBinds).count;
  const signalRows = db2.prepare(
    `SELECT signal_id FROM signals
      WHERE status = 'open' AND created_at < ?${scopeSql}
      ORDER BY datetime(created_at), signal_id LIMIT 3`
  ).all(cutoff, ...scopeBinds);
  const referenceRows = db2.prepare(
    `SELECT m.memory_id, r.reference
       FROM memories m
       JOIN memory_refs r ON r.memory_id = m.memory_id
      WHERE m.state = 'ACTIVE'
        AND r.reference LIKE 'file:%'
        AND COALESCE(m.updated_at, m.created_at) < ?
        ${scopeSql.replaceAll("workspace_path", "m.workspace_path").replaceAll("artifact", "m.artifact")}
      ORDER BY datetime(COALESCE(m.updated_at, m.created_at)), m.memory_id
      LIMIT 1000`
  ).all(cutoff, ...scopeBinds);
  const staleMemoryIds = /* @__PURE__ */ new Set();
  for (const row of referenceRows) {
    const raw = row.reference.slice("file:".length).replace(/(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)$/, "");
    const path2 = isAbsolute3(raw) ? raw : resolve6(workspacePath ?? process.cwd(), raw);
    if (!existsSync(path2)) staleMemoryIds.add(row.memory_id);
  }
  return {
    pressure_age_days: pressureAgeDays,
    cutoff,
    stale_pending_runs: pendingCount,
    stale_open_signals: signalCount,
    stale_missing_refs: staleMemoryIds.size,
    samples: {
      run_ids: pendingRows.map((row) => row.run_id),
      signal_ids: signalRows.map((row) => row.signal_id),
      memory_ids: [...staleMemoryIds].slice(0, 3)
    }
  };
}
function digest(db2, params = {}) {
  const retentionDays = retentionWindow(params, "retention_days", "retentionDays", 90);
  const handoffRetentionDays = retentionWindow(params, "refinement_handoff_retention_days", "refinementHandoffRetentionDays", 7);
  const doneRetentionDays = retentionWindow(params, "refinement_done_retention_days", "refinementDoneRetentionDays", 30);
  const operationalRetentionDays = retentionWindow(params, "operational_retention_days", "operationalRetentionDays", 90);
  retentionWindow(params, "pressure_age_days", "pressureAgeDays", 1);
  const rawWorkspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const now = (/* @__PURE__ */ new Date()).toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 864e5).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 864e5).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 864e5).toISOString();
  const operationalCutoff = new Date(Date.now() - operationalRetentionDays * 864e5).toISOString();
  const pressure = inspectMaintenancePressure(db2, params);
  const pressureFields = {
    pressure_age_days: pressure.pressure_age_days,
    stale_pending_runs: pressure.stale_pending_runs,
    stale_open_signals: pressure.stale_open_signals,
    stale_missing_refs: pressure.stale_missing_refs,
    pressure_samples: pressure.samples
  };
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
    const candidateLimit = 20;
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
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`).get(handoffCutoff, doneCutoff, ...refinementScopeBinds).c;
    const wouldPruneRuns = db2.prepare(`SELECT COUNT(*) AS c FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}`).get(operationalCutoff, ...memoryScopeBinds).c;
    const expireMemoryIds = db2.prepare(
      `SELECT memory_id FROM memories
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}
       ORDER BY datetime(valid_to), memory_id LIMIT ?`
    ).all(now, ...memoryScopeBinds, candidateLimit).map((row) => row.memory_id);
    const purgeMemoryIds = db2.prepare(
      `SELECT memory_id FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}
       ORDER BY datetime(updated_at), memory_id LIMIT ?`
    ).all(cutoff, ...memoryScopeBinds, candidateLimit).map((row) => row.memory_id);
    const refinementIds = db2.prepare(
      `SELECT refinement_id FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}
       ORDER BY datetime(updated_at), refinement_id LIMIT ?`
    ).all(handoffCutoff, doneCutoff, ...refinementScopeBinds, candidateLimit).map((row) => row.refinement_id);
    const runIds = db2.prepare(
      `SELECT run_id FROM task_runs
       WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
         AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}
       ORDER BY datetime(updated_at), run_id LIMIT ?`
    ).all(operationalCutoff, ...memoryScopeBinds, candidateLimit).map((row) => row.run_id);
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      pruned_refinements: 0,
      pruned_runs: 0,
      fts_rebuilt: false,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements,
      would_prune_runs: wouldPruneRuns,
      candidate_limit: candidateLimit,
      candidate_ids: {
        expire_memory_ids: expireMemoryIds,
        purge_memory_ids: purgeMemoryIds,
        lock_ids: lockDryRun.lock_ids ?? [],
        refinement_ids: refinementIds,
        run_ids: runIds
      },
      ...pressureFields
    };
  }
  let archiveRes = { changes: 0 };
  let deleteRes = { changes: 0 };
  let prunedLocks = 0;
  let pruneRefinementsRes = { changes: 0 };
  let pruneRunsRes = { changes: 0 };
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
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`
    ).run(handoffCutoff, doneCutoff, ...refinementScopeBinds);
    pruneRunsRes = db2.prepare(`DELETE FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}`).run(operationalCutoff, ...memoryScopeBinds);
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
    pruned_runs: pruneRunsRes.changes,
    fts_rebuilt: ftsRebuilt,
    ...pressureFields
  };
}

// src/pi-hooks.ts
import path from "node:path";
import { spawnSync as spawnSync3 } from "node:child_process";
import { createHash as createHash4, randomUUID as randomUUID9 } from "node:crypto";
import { realpathSync as realpathSync2 } from "node:fs";
var _sessionStartupToken = randomUUID9().slice(0, 8);
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
var INTERNAL_HOOK_HOST = "__octocode_hook_host";
var INTERNAL_SKILL_ROOT = "__octocode_skill_root";
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
function normalizeShellHookHost(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase();
  return normalized === "claude" || normalized === "codex" || normalized === "cursor" ? normalized : null;
}
function shellHookHost(payload) {
  const explicit = normalizeShellHookHost(
    payload[INTERNAL_HOOK_HOST] ?? process.env.OCTOCODE_AGENT_HOST ?? payload.host ?? payload.client
  );
  if (explicit) return explicit;
  const eventName = firstString2(payload.hook_event_name, payload.eventName) ?? "";
  if (eventName && eventName[0] === eventName[0]?.toLowerCase()) return "cursor";
  return "claude";
}
function hookSkillRoot(payload) {
  return firstString2(payload[INTERNAL_SKILL_ROOT], process.env.OCTOCODE_SKILL_ROOT);
}
function hookContextEnvelope(host, eventName, message) {
  if (host === "cursor") {
    if (eventName === "sessionStart") return { additional_context: message };
    return { permission: "allow", agent_message: message };
  }
  return {
    hookSpecificOutput: {
      hookEventName: eventName,
      additionalContext: message
    }
  };
}
function hookBlockOutcome(host, phase, message) {
  if (host !== "cursor") return { exitCode: 2, stderr: message };
  if (phase === "stop") {
    return { exitCode: 0, payload: { followup_message: message } };
  }
  return {
    exitCode: 0,
    payload: {
      permission: "deny",
      user_message: message,
      agent_message: message
    }
  };
}
function writeHookPayload(payload) {
  process.stdout.write(`${JSON.stringify(payload)}
`);
}
function emitHookContext(payload, eventName, message) {
  writeHookPayload(hookContextEnvelope(shellHookHost(payload), eventName, message));
}
function completeHookControl(outcome) {
  if (outcome.payload) writeHookPayload(outcome.payload);
  if (outcome.stderr) console.error(outcome.stderr);
  return outcome.exitCode;
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
    payload[INTERNAL_HOOK_HOST],
    process.env.OCTOCODE_AGENT_HOST,
    payload.host,
    payload.client,
    payload.source,
    payload.context
  ) ?? "shell";
  const scope = `${host}\0${workspace(payload) ?? process.cwd()}`;
  const suffix = createHash5("sha1").update(scope).digest("hex").slice(0, 12);
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
function promptQuery(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  const prompt = firstString2(
    payload.prompt,
    payload.user_prompt,
    payload.userPrompt,
    payload.text,
    payload.message,
    typeof payload.input === "string" ? payload.input : null,
    input.prompt,
    input.user_prompt,
    input.userPrompt,
    input.text,
    input.message
  );
  return prompt ? prompt.slice(0, 4e3) : null;
}
function hookSessionCorrelation(payload) {
  const input = objectOrEmpty2(payloadInput(payload));
  return firstString2(
    sessionId(payload),
    payload.transcript_path,
    payload.transcriptPath,
    payload.conversation_id,
    payload.conversationId,
    payload.thread_id,
    payload.threadId,
    input.transcript_path,
    input.transcriptPath,
    input.conversation_id,
    input.conversationId,
    input.thread_id,
    input.threadId
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
function fallbackVerificationPlan(files, cwd) {
  const canonicalWorkspace = canonicalizePath(cwd);
  const normalized = [...new Set(files.map((file) => resolveHookPath(file, cwd)))];
  const shown = normalized.slice(0, 3).map((file) => relative2(canonicalWorkspace, file) || basename2(file)).join(", ");
  const omitted = normalized.length > 3 ? ` (+${normalized.length - 3} more)` : "";
  return `Verify ${shown || "the edited files"}${omitted}: run the smallest relevant test/typecheck and inspect the diff; record the check and result.`;
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
  return createHash5("sha1").update(JSON.stringify(identity)).digest("hex");
}
var HOOK_AGGREGATE_CONTEXT_PREFIX = "hook-scope:";
function hookAggregateContextRef(payload, cwd) {
  const sessionCorrelation = hookSessionCorrelation(payload);
  if (!sessionCorrelation) return null;
  const identity = {
    agent: agentId(payload),
    session: sessionCorrelation,
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve7(cwd),
    artifact: normalizeArtifact(artifact(payload))
  };
  return `${HOOK_AGGREGATE_CONTEXT_PREFIX}${createHash5("sha1").update(JSON.stringify(identity)).digest("hex")}`;
}
function activeFallbackHookRun(database, payload, cwd) {
  const contextRef = hookAggregateContextRef(payload, cwd);
  if (!contextRef) return null;
  const row = database.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY updated_at DESC, created_at DESC LIMIT 1`).get(
    agentId(payload),
    normalizeWorkspacePath(cwd, cwd) ?? resolve7(cwd),
    normalizeArtifact(artifact(payload)),
    contextRef
  );
  return row?.run_id ?? null;
}
function hookAggregateLockKey(payload, cwd) {
  const contextRef = hookAggregateContextRef(payload, cwd);
  return contextRef ? `aggregate-${createHash5("sha1").update(contextRef).digest("hex")}` : null;
}
function startOrAttachFallbackHookRun(database, payload, cwd, files) {
  const contextRef = hookAggregateContextRef(payload, cwd);
  const startOrAttach = () => {
    const existingRunId = activeFallbackHookRun(database, payload, cwd);
    const result = startWork(database, {
      agentId: agentId(payload),
      sessionId: sessionId(payload),
      workspacePath: cwd,
      artifact: artifact(payload),
      runId: existingRunId ?? void 0,
      rationale: autoClaimRationale(payload, files),
      testPlan: fallbackVerificationPlan(files, cwd),
      contextRef: contextRef ?? void 0,
      targetFiles: files,
      origin: "HOOK",
      source: "HOOK",
      ttlMs: 10 * 6e4
    });
    if (result.ok && existingRunId) {
      touchWork(database, {
        agentId: agentId(payload),
        runId: existingRunId,
        ttlMs: 10 * 6e4
      });
    }
    return result;
  };
  const lockKey = hookAggregateLockKey(payload, cwd);
  return lockKey ? withHookRunStateLock(lockKey, startOrAttach) : startOrAttach();
}
function refreshFallbackVerificationPlan(database, runId, cwd) {
  if (!isAggregatedFallbackHookRun(database, runId)) return;
  const files = database.prepare("SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path").all(runId);
  database.prepare("UPDATE task_runs SET test_plan = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE run_id = ? AND origin = 'HOOK'").run(fallbackVerificationPlan(files.map((file) => file.file_path), cwd), runId);
}
function isAggregatedFallbackHookRun(database, runId) {
  const row = database.prepare(`SELECT origin, context_ref FROM task_runs WHERE run_id = ?`).get(runId);
  return row?.origin === "HOOK" && row.context_ref?.startsWith(HOOK_AGGREGATE_CONTEXT_PREFIX) === true;
}
function finalizeActiveFallbackHookRuns(database, payload, cwd) {
  const contextRef = hookAggregateContextRef(payload, cwd);
  if (!contextRef) return [];
  const rows = database.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY created_at`).all(
    agentId(payload),
    normalizeWorkspacePath(cwd, cwd) ?? resolve7(cwd),
    normalizeArtifact(artifact(payload)),
    contextRef
  );
  const finalized = [];
  for (const row of rows) {
    endWork(database, { agentId: agentId(payload), runId: row.run_id });
    finalized.push(row.run_id);
  }
  return finalized;
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
  return createHash5("sha1").update(JSON.stringify({
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve7(cwd),
    artifact: artifact(payload),
    files: files.map((file) => resolveHookPath(file, cwd)).sort()
  })).digest("hex");
}
function peerFingerprint(peers) {
  return createHash5("sha1").update(JSON.stringify(peers.map((peer) => ({
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
  if (previous === fingerprint) return null;
  writeFileSync(stateFile, fingerprint, "utf8");
  if (peers.length === 0) return null;
  const shown = peers.slice(0, 3).map(peerLabel).join("; ");
  const omitted = peers.length > 3 ? ` +${peers.length - 3}` : "";
  const canonicalWorkspace = canonicalizePath(cwd);
  const targets = files.slice(0, 2).map((file) => relative2(canonicalWorkspace, resolveHookPath(file, cwd)) || basename2(file)).join(",");
  return `AWARE ${targets} | peers ${shown}${omitted}`;
}
function hookAgentContext(payload, hookName) {
  const value = process.env.OCTOCODE_AGENT_CONTEXT ?? payload[INTERNAL_HOOK_HOST] ?? process.env.OCTOCODE_AGENT_HOST ?? payload.context ?? payload.host ?? payload.client ?? payload.source;
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
    skillRoot: hookSkillRoot(payload),
    cwd: hookWorkspace
  });
  if (guardReason) {
    return completeHookControl(hookBlockOutcome(
      shellHookHost(payload),
      "pre-edit",
      `${guardReason} Edit blocked.`
    ));
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
    }) } : activeClaim ? startWork(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      runId: activeClaim.run_id,
      targetFiles: files,
      origin: "HOOK",
      source: "HOOK",
      ttlMs: 10 * 6e4
    }) : startOrAttachFallbackHookRun(database, payload, hookWorkspace, files);
    if (!result.ok) {
      const detail = result.conflicts.slice(0, 3).map((conflict) => `${relative2(hookWorkspace, conflict.file_path)} (${conflict.agent_id})`).join(", ");
      return completeHookControl(hookBlockOutcome(
        shellHookHost(payload),
        "pre-edit",
        `octocode-awareness: exclusive file work blocks this edit${detail ? `: ${detail}` : ""}.`
      ));
    }
    withHookDbRetry(() => refreshFallbackVerificationPlan(database, result.run.run_id, hookWorkspace));
    recordHookRun(payload, files, hookWorkspace, result.run.run_id);
    const peerContext = emitPeerDelta(payload, files, hookWorkspace, result.peers);
    if (peerContext) {
      emitHookContext(
        payload,
        shellHookHost(payload) === "cursor" ? "preToolUse" : "PreToolUse",
        peerContext
      );
    }
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
    if (origin === "HOOK" && isAggregatedFallbackHookRun(database, correlatedRunId)) {
      withHookDbRetry(() => touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        ttlMs: 10 * 6e4
      }));
    } else if (origin === "HOOK") {
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
    skillRoot: hookSkillRoot(payload),
    cwd: process.cwd()
  });
  if (reason) {
    return completeHookControl(hookBlockOutcome(
      shellHookHost(payload),
      "pre-edit",
      `${reason} Edit blocked.`
    ));
  }
  return 0;
}
async function runStopVerify(payload) {
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:stop-verify");
    const finalizedRunIds = withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd()
    ));
    if (process.env.OCTOCODE_NO_VERIFY_GATE === "1") return 0;
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      if (isStopHookActive(payload) && finalizedRunIds.length === 0) return 0;
      const details = [
        ...report.unverified.map((run) => `${run.status}:${run.run_id}: ${run.test_plan}`),
        ...report.stale_active.map((run) => `STALE:${run.run_id}: ${run.rationale}`)
      ];
      const shown = details.slice(0, 3);
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : "";
      return completeHookControl(hookBlockOutcome(
        shellHookHost(payload),
        "stop",
        `octocode-awareness: concluding with unverified work. ${shown.join("; ")}${omitted}`
      ));
    }
  } catch (error) {
    console.error(`octocode-awareness verify warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}
function maybePreviewDigest(payload) {
  if (process.env.OCTOCODE_NO_DIGEST === "1") return null;
  if (process.env.OCTOCODE_NOTIFY_RUN_DIGEST !== "1") return null;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 36e5 : 4 * 36e5;
  const memoryHome2 = dirname3(resolveDbPath(null));
  const digestScope = workspace(payload) ?? "global";
  const scopeHash = createHash5("sha256").update(digestScope).digest("hex").slice(0, 12);
  const markerPath = join3(memoryHome2, `.last-digest-preview-${scopeHash}-epoch-ms`);
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
      const preview = digest(database, {
        workspace: workspace(payload),
        memoryHome: memoryHome2,
        dry_run: true
      });
      mkdirSync2(memoryHome2, { recursive: true });
      writeFileSync(markerPath, String(now), "utf8");
      const pressure = {
        archive: preview.would_archive ?? 0,
        memories: preview.would_prune_old ?? 0,
        locks: preview.would_prune_locks ?? 0,
        refinements: preview.would_prune_refinements ?? 0
      };
      if (Object.values(pressure).some((count) => count > 0)) {
        return `Maintenance pressure: archive ${pressure.archive}, prune memories ${pressure.memories}, locks ${pressure.locks}, refinements ${pressure.refinements}. Review with octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact; apply only after review.`;
      }
    }
  } catch (error) {
    console.error(`octocode-awareness digest warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return null;
}
async function runNotifyDeliver(payload) {
  if (process.env.OCTOCODE_NO_NOTIFY === "1") return 0;
  const maintenanceContext = maybePreviewDigest(payload);
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:notify-deliver");
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd()
    ));
    const result = notifyGet(database, {
      agent_id: agentId(payload),
      session_id: hookSessionCorrelation(payload) ?? void 0,
      workspace: workspace(payload) ?? void 0,
      artifact: artifact(payload) ?? void 0,
      query: promptQuery(payload) ?? void 0,
      format: "hook"
    });
    const additionalContext = [result.additionalContext, maintenanceContext].filter(Boolean).join("\n");
    if (additionalContext) {
      emitHookContext(
        payload,
        shellHookHost(payload) === "cursor" ? "sessionStart" : "UserPromptSubmit",
        additionalContext
      );
    }
  } catch (error) {
    console.error(`octocode-awareness session-capture warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}
async function runSessionEnd(payload) {
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:session-end");
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd()
    ));
    if (process.env.OCTOCODE_NO_SESSION_CAPTURE !== "1" && hookReason(payload) !== "clear") {
      sessionCapture(database, {
        agent_id: agentId(payload),
        workspace: workspace(payload) ?? void 0,
        artifact: artifact(payload) ?? void 0,
        reason: hookReason(payload) || void 0
      });
    }
    const sid = sessionId(payload);
    if (sid) endSession(database, {
      sessionId: sid,
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? process.cwd(),
      artifact: artifact(payload)
    });
  } catch {
  }
  return 0;
}
async function runHookCommand(command, rawPayload, options = {}) {
  if (command === "help" || command === "--help" || command === "-h") {
    process.stdout.write("usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n");
    return 0;
  }
  const payload = {
    ...parsePayload(rawPayload ?? await readStdin()),
    ...options.host ? { [INTERNAL_HOOK_HOST]: options.host } : {},
    ...options.skillRoot ? { [INTERNAL_SKILL_ROOT]: options.skillRoot } : {}
  };
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
  const hostIndex = process.argv.indexOf("--host");
  const rawHost = hostIndex >= 0 ? process.argv[hostIndex + 1] : void 0;
  const host = normalizeShellHookHost(rawHost);
  if (rawHost && !host) {
    console.error(`unknown hook host: ${rawHost}`);
    return 1;
  }
  const skillRootIndex = process.argv.indexOf("--skill-root");
  const skillRoot = skillRootIndex >= 0 ? process.argv[skillRootIndex + 1] : void 0;
  return runHookCommand(process.argv[2] ?? "help", void 0, {
    ...host ? { host } : {},
    ...skillRoot ? { skillRoot } : {}
  });
}
var isMain = process.argv[1] ? fileURLToPath(import.meta.url) === resolve7(process.argv[1]) : false;
var invokedAsHookRunner = process.argv[1] ? /^hook-runner\.(js|mjs|ts)$/.test(basename2(process.argv[1])) : false;
if (isMain && invokedAsHookRunner) {
  process.exitCode = await main();
}
export {
  hookBlockOutcome,
  hookContextEnvelope,
  runHookCommand
};
