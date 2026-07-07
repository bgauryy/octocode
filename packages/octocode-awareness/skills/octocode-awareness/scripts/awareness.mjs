#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (w) => {
  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;
  console.error(w?.stack ?? String(w));
});

// bin/awareness.ts
import { writeFileSync, mkdirSync as mkdirSync2 } from "node:fs";
import { dirname as dirname2 } from "node:path";
import { DatabaseSync as DatabaseSync2 } from "node:sqlite";

// src/db.ts
import { DatabaseSync } from "node:sqlite";
import { mkdirSync } from "node:fs";
import { join, resolve as resolve2, dirname } from "node:path";
import { homedir, platform } from "node:os";

// src/helpers.ts
import { resolve } from "node:path";
var MEMORY_LABELS = /* @__PURE__ */ new Set([
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
]);
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
  if (!value) return "OTHER";
  const cleaned = String(value).trim().toUpperCase().replace(/[\s-]+/g, "_");
  return MEMORY_LABELS.has(cleaned) ? cleaned : "OTHER";
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

// src/db.ts
var DEFAULT_DB_NAME = "awareness.sqlite3";
var MEMORY_HOME_ENV = "OCTOCODE_MEMORY_HOME";
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
  const db2 = new DatabaseSync(dbPath2);
  db2.exec("PRAGMA foreign_keys = ON");
  db2.exec("PRAGMA busy_timeout = 5000");
  db2.exec("PRAGMA journal_mode = WAL");
  initDb(db2);
  _db = db2;
  return db2;
}
function initDb(db2) {
  db2.exec(`
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

    CREATE TABLE IF NOT EXISTS tasks (
      task_id        TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      rationale      TEXT NOT NULL,
      test_plan      TEXT NOT NULL,
      status         TEXT NOT NULL CHECK(status IN ('PENDING','ACTIVE','SUCCESS','FAILED')) DEFAULT 'ACTIVE',
      workspace_path TEXT,
      artifact       TEXT,
      files_json     TEXT NOT NULL DEFAULT '[]',
      created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS locks (
      lock_id     TEXT PRIMARY KEY,
      file_path   TEXT NOT NULL,
      task_id     TEXT NOT NULL,
      agent_id    TEXT NOT NULL,
      session_id  TEXT,
      lock_type   TEXT NOT NULL CHECK(lock_type IN ('SHARED','EXCLUSIVE')),
      acquired_at TEXT NOT NULL,
      expires_at  TEXT,
      FOREIGN KEY(task_id) REFERENCES tasks(task_id) ON DELETE CASCADE,
      UNIQUE(file_path, task_id)
    );

    CREATE TABLE IF NOT EXISTS task_log (
      event_id   TEXT PRIMARY KEY,
      task_id    TEXT,
      agent_id   TEXT NOT NULL,
      event_type TEXT NOT NULL,
      message    TEXT NOT NULL,
      created_at TEXT NOT NULL,
      FOREIGN KEY(task_id) REFERENCES tasks(task_id) ON DELETE SET NULL
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
      quality        TEXT NOT NULL CHECK(quality IN ('good','bad','handoff')) DEFAULT 'good',
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
      task_id        TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
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
      task_id      TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
      created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
  `);
  ensureColumn(db2, "sessions", "artifact", "TEXT");
  ensureColumn(db2, "memories", "artifact", "TEXT");
  ensureColumn(db2, "tasks", "artifact", "TEXT");
  ensureColumn(db2, "refinements", "artifact", "TEXT");
  ensureColumn(db2, "signals", "artifact", "TEXT");
  ensureColumn(db2, "agents", "artifact", "TEXT");
  ensureColumn(db2, "edit_log", "artifact", "TEXT");
  ensureColumn(db2, "harness_log", "workspace_path", "TEXT");
  ensureColumn(db2, "harness_log", "artifact", "TEXT");
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

    CREATE INDEX IF NOT EXISTS idx_tasks_status       ON tasks(status);
    CREATE INDEX IF NOT EXISTS idx_tasks_agent_status ON tasks(agent_id, status);
    CREATE INDEX IF NOT EXISTS idx_tasks_workspace    ON tasks(workspace_path);
    CREATE INDEX IF NOT EXISTS idx_tasks_scope        ON tasks(workspace_path, artifact);

    CREATE INDEX IF NOT EXISTS idx_locks_file_path   ON locks(file_path);
    CREATE INDEX IF NOT EXISTS idx_locks_agent_id    ON locks(agent_id);
    CREATE INDEX IF NOT EXISTS idx_locks_acquired_at ON locks(acquired_at);
    CREATE INDEX IF NOT EXISTS idx_locks_expires_at  ON locks(expires_at);
    CREATE INDEX IF NOT EXISTS idx_locks_session_id  ON locks(session_id);

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
    CREATE INDEX IF NOT EXISTS idx_edit_log_task        ON edit_log(task_id);
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
}
function tableColumns(db2, tableName) {
  const rows = db2.prepare(`PRAGMA table_info(${tableName})`).all();
  return new Set(rows.map((r) => r.name));
}
function ensureColumn(db2, tableName, columnName, columnType) {
  if (tableColumns(db2, tableName).has(columnName)) return;
  db2.exec(`ALTER TABLE ${tableName} ADD COLUMN ${columnName} ${columnType}`);
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
  return [...tags, label].filter(Boolean).join(" ");
}
function rebuildFts(db2) {
  db2.exec("DELETE FROM memories_fts");
  const rows = db2.prepare(
    "SELECT memory_id, task_context, observation, tags_json, label FROM memories"
  ).all();
  const insert = db2.prepare(
    "INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
  );
  for (const row of rows) {
    insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
  }
}
function referenceKind(reference) {
  if (/^https?:\/\//.test(reference)) return "url";
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.\-]*):/);
  return m ? m[1].toLowerCase() : "other";
}
function replaceMemoryReferences(db2, memoryId, references) {
  db2.prepare("DELETE FROM memory_refs WHERE memory_id = ?").run(memoryId);
  const insert = db2.prepare(
    "INSERT OR REPLACE INTO memory_refs(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)"
  );
  references.forEach((ref, i) => insert.run(memoryId, ref, referenceKind(ref), i));
}
function evictExpiredLocks(db2) {
  db2.prepare("DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?").run(utcNow());
}

// src/memory.ts
import { randomUUID } from "node:crypto";

// src/git.ts
import { spawnSync } from "node:child_process";
import { basename } from "node:path";
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
function fillScope(partial, cwd) {
  const scope = {
    workspace_path: partial.workspace_path ?? null,
    artifact: partial.artifact ?? null,
    repo: partial.repo ?? null,
    ref: partial.ref ?? null
  };
  const git = detectGit(scope.workspace_path ?? cwd ?? process.cwd());
  if (!git.is_repo) return scope;
  if (git.root) scope.workspace_path = git.root;
  if (!scope.repo && git.repo) scope.repo = git.repo;
  if (!scope.ref && git.branch) scope.ref = git.branch;
  return scope;
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
function findSimilarMemories(db2, text, limit = 3, excludeMemoryId = null, scopeOptions = {}) {
  const queryTokens = textTokens(text);
  if (queryTokens.size === 0) return [];
  const candidates = lexicalSearch(
    db2,
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
function fallbackSearch(db2, conditions, params, limit) {
  const sql = `
    SELECT m.*, 0 AS _bm25
    FROM memories m
    WHERE ${conditions.join(" AND ")}
    ORDER BY m.importance DESC, m.created_at DESC
    LIMIT ?
  `;
  return db2.prepare(sql).all(...params, limit);
}
function applyScopeConditions(conditions, params, options = {}) {
  const artifact = normalizeArtifact(options.artifact);
  const scope = fillScope(
    {
      workspace_path: options.workspacePath ?? null,
      artifact,
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
  let rows;
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
      rows = fallbackSearch(db2, conditions, params, limit);
    }
  } else {
    rows = fallbackSearch(db2, conditions, params, limit);
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
function insertMemory(db2, params) {
  const {
    agentId = "agent",
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
    artifact,
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
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const halfLifeDefault = LABEL_HALF_LIFE_DAYS[normalizedLabel] ?? null;
  let noveltyScore = 0;
  let similarMemoryIds = [];
  const superseded = [];
  db2.exec("BEGIN IMMEDIATE");
  try {
    const similar = params.preComputedSimilar ?? findSimilarMemories(db2, `${taskContext} ${observation}`, 3, null, {
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd
    });
    noveltyScore = Math.max(0, Math.min(1, 1 - (similar[0]?.similarity ?? 0)));
    similarMemoryIds = similar.map((m) => m.memory_id);
    db2.prepare(`
      INSERT INTO memories (
        memory_id, agent_id, task_context, observation, importance,
        label, tags_json, workspace_path, artifact, repo, ref,
        file_tree_fingerprint, novelty_score, created_at, updated_at,
        last_accessed_at, access_count, failure_signature, valid_from, valid_to, decay_half_life_days
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)
    `).run(
      memoryId,
      agentId,
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
        replaceMemoryReferences(db2, memoryId, refList);
      } catch (e) {
        if (!(e instanceof Error && e.message.includes("no such table"))) throw e;
      }
    }
    if (hasFts(db2)) {
      db2.prepare(
        "INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
      ).run(
        memoryId,
        taskContext,
        observation,
        ftsTermsForRow({
          tags_json: JSON.stringify(tagList),
          label: normalizedLabel
        })
      );
    }
    for (const oldId of supersedes) {
      const r = db2.prepare(`
        UPDATE memories
        SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
            valid_to = COALESCE(valid_to, ?), expired_at = ?
        WHERE memory_id = ? AND memory_id <> ?
      `).run(memoryId, createdAt, validFromVal, createdAt, oldId, memoryId);
      if (r.changes) superseded.push(oldId);
    }
    db2.exec("COMMIT");
  } catch (e) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
  return {
    memoryId,
    memory: {
      memory_id: memoryId,
      agent_id: agentId,
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
function getMemory(db2, params = {}) {
  const {
    query = "",
    limit: limitRaw = 3,
    minImportance: minImpRaw = 1,
    label,
    tags = [],
    smart = false,
    workspacePath,
    artifact,
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
  const labels = label ? Array.isArray(label) ? label.map(normalizeLabel) : [normalizeLabel(label)] : [];
  let memories = lexicalSearch(
    db2,
    query,
    limit * SCORING_PREFETCH_FACTOR,
    minImportance,
    tags,
    labels,
    states,
    {
      workspacePath: workspacePath ?? cwdParam ?? null,
      artifact,
      repo: repoArg,
      ref: refArg,
      strictScope,
      globalOnly,
      cwd: cwdParam
    }
  );
  const resolvedScope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwdParam ?? workspacePath ?? process.cwd()
  );
  if (files.length > 0) {
    const normFiles = new Set(
      files.map((f) => normalizeFilePath(f, cwdParam ?? workspacePath ?? void 0) ?? f)
    );
    const normFileRefs = new Set([...normFiles].map((f) => `file:${f}`));
    memories = memories.filter(
      (m) => m.references.some((r) => normFiles.has(r) || normFileRefs.has(r))
    );
  }
  if (references.length > 0) {
    const refSet = new Set(references);
    try {
      const fromTable = /* @__PURE__ */ new Set();
      for (const ref of references) {
        const rows = db2.prepare(
          "SELECT memory_id FROM memory_refs WHERE reference = ?"
        ).all(ref);
        rows.forEach((r) => fromTable.add(r.memory_id));
      }
      if (fromTable.size > 0) {
        const existingIds = new Set(memories.map((m) => m.memory_id));
        const missingIds = [...fromTable].filter((id) => !existingIds.has(id));
        if (missingIds.length > 0) {
          const ph = missingIds.map(() => "?").join(",");
          const extraConditions = [
            `m.memory_id IN (${ph})`,
            `m.importance >= ?`,
            `m.state IN (${states.map(() => "?").join(",")})`
          ];
          const extraParams = [...missingIds, minImportance, ...states];
          if (globalOnly) {
            extraConditions.push("m.workspace_path IS NULL", "m.artifact IS NULL", "m.repo IS NULL", "m.ref IS NULL");
          } else {
            applyScopeConditions(extraConditions, extraParams, {
              workspacePath: resolvedScope.workspace_path,
              artifact: resolvedScope.artifact,
              repo: resolvedScope.repo,
              ref: resolvedScope.ref,
              strictScope,
              cwd: cwdParam
            });
          }
          const extra = db2.prepare(
            `SELECT m.*, 0 AS _bm25 FROM memories m WHERE ${extraConditions.join(" AND ")}`
          ).all(...extraParams);
          for (const row of extra) {
            const mem = rowToMemory(row);
            mem.lexical = 0;
            mem.score = decayScore(mem, 0);
            memories.push(mem);
          }
        }
      }
      memories = memories.filter((m) => fromTable.has(m.memory_id));
    } catch {
      memories = memories.filter((m) => (m.references ?? []).some((r) => refSet.has(r)));
    }
  }
  if (regex.length > 0 || fileRegex.length > 0) {
    const compileRegex = (pattern) => {
      try {
        return new RegExp(pattern);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`invalid regex ${JSON.stringify(pattern)}: ${message}`);
      }
    };
    const compiledRegex = regex.map(compileRegex);
    const compiledFileRegex = fileRegex.map(compileRegex);
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
  if (asOf) {
    const asOfDate = new Date(asOf);
    if (isNaN(asOfDate.getTime())) {
      throw new Error(`invalid --as-of value "${asOf}" \u2014 expected ISO 8601 date string (e.g. 2024-06-01T00:00:00Z)`);
    }
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
  bumpAccess(db2, memories.map((m) => m.memory_id));
  const mode = hasFts(db2) ? "lexical" : "fallback";
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
function forgetMemory(db2, params) {
  const { memoryIds = [], tags = [], before, dryRun = false } = params;
  let { maxImportance } = params;
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
  const where = selectorGroups.join(" OR ");
  const rows = db2.prepare(
    `SELECT memory_id FROM memories WHERE ${where}`
  ).all(...bindParams);
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
    db2.exec("BEGIN IMMEDIATE");
    try {
      db2.prepare(`DELETE FROM memories WHERE memory_id IN (${ph})`).run(...ids);
      if (hasFts(db2)) {
        db2.prepare(`DELETE FROM memories_fts WHERE memory_id IN (${ph})`).run(...ids);
      }
      try {
        db2.prepare(`DELETE FROM memory_refs WHERE memory_id IN (${ph})`).run(...ids);
      } catch {
      }
      db2.exec("COMMIT");
    } catch (e) {
      try {
        db2.exec("ROLLBACK");
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
function mineWeakness(db2, params = {}) {
  const { minCount = 2, limit = 20, cwd } = params;
  const wsPath = params.workspacePath ?? (cwd ? fillScope({ workspace_path: null }, cwd).workspace_path : null);
  const artifact = normalizeArtifact(params.artifact);
  const conditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
  const bindParams = [];
  if (wsPath) {
    conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
    bindParams.push(wsPath);
  }
  if (artifact) {
    conditions.push("(artifact = ? OR artifact IS NULL)");
    bindParams.push(artifact);
  }
  if (params.agentId) {
    conditions.push("agent_id = ?");
    bindParams.push(params.agentId);
  }
  const fetchLimit = limit * 3;
  const rows = db2.prepare(`
    SELECT failure_signature,
           count(*) AS freq,
           avg(importance) AS avg_imp,
           count(*) * avg(importance) AS score,
           group_concat(memory_id, ',') AS ids,
           group_concat(DISTINCT label) AS labels
    FROM memories
    WHERE ${conditions.join(" AND ")}
    GROUP BY failure_signature
    HAVING freq >= ?
    ORDER BY score DESC
    LIMIT ?
  `).all(...bindParams, minCount, fetchLimit);
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
      if (row.score > existing.total_score - row.score) existing.raw_sig = row.failure_signature;
    } else {
      mergedMap.set(base, {
        base_sig: base,
        raw_sig: row.failure_signature,
        total_freq: row.freq,
        total_score: row.score,
        importance_sum: row.avg_imp * row.freq,
        ids: row.ids.split(","),
        labels: new Set(row.labels.split(",").filter(Boolean)),
        surfaces: new Set(surface ? [surface] : [])
      });
    }
  }
  const merged = [...mergedMap.values()].sort((a, b) => b.total_score - a.total_score);
  const repMap = /* @__PURE__ */ new Map();
  const allRawSigs = merged.map((m) => m.raw_sig);
  if (allRawSigs.length > 0) {
    const ph = allRawSigs.map(() => "?").join(",");
    const repRows = db2.prepare(
      `SELECT failure_signature, observation, max(importance)
       FROM memories
       WHERE failure_signature IN (${ph}) AND state = 'ACTIVE'
       GROUP BY failure_signature`
    ).all(...allRawSigs);
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
  const totals = db2.prepare(
    `SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
     FROM memories WHERE failure_signature IS NOT NULL AND state = 'ACTIVE'`
  ).get();
  return { ok: true, clusters: selected, total_signatures: totals.sigs, total_memories: totals.mems };
}

// src/audit.ts
import { randomUUID as randomUUID2 } from "node:crypto";
import { createHash } from "node:crypto";

// src/sql/audit.ts
var HARNESS_LOG_INSERT = `
  INSERT INTO harness_log (
    harness_id, session_id, agent_id, workspace_path, artifact, event_type,
    payload_json, memory_id, task_id, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;

// src/audit.ts
function insertHarnessLog(db2, params) {
  const harnessId = "harness_" + randomUUID2();
  const now = utcNow();
  const payloadJson = params.payload !== void 0 ? JSON.stringify(params.payload) : null;
  db2.prepare(HARNESS_LOG_INSERT).run(
    harnessId,
    params.sessionId ?? null,
    params.agentId,
    params.workspacePath ?? null,
    normalizeArtifact(params.artifact),
    params.eventType,
    payloadJson,
    params.memoryId ?? null,
    params.taskId ?? null,
    now
  );
  return harnessId;
}

// src/docs.ts
var DEFAULT_MIN_EDITS_SINCE_SYNC = 5;
var DEFAULT_MIN_LINES_SINCE_SYNC = 50;
function lastEditTimestamp(db2, filePath, workspacePath, artifact) {
  const conditions = ["file_path = ?"];
  const binds = [filePath];
  if (workspacePath) {
    conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
    binds.push(workspacePath);
  }
  if (artifact) {
    conditions.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact);
  }
  const row = db2.prepare(
    `SELECT MAX(created_at) AS ts FROM edit_log WHERE ${conditions.join(" AND ")}`
  ).get(...binds);
  return row?.ts ?? null;
}
function sourceActivitySince(db2, sourceDirs, since, workspacePath, artifact) {
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
  if (artifact) {
    conditions.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact);
  }
  const rows = db2.prepare(
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
function mineDocStaleness(db2, params) {
  const minEdits = params.minEditsSinceSync ?? DEFAULT_MIN_EDITS_SINCE_SYNC;
  const minLines = params.minLinesSinceSync ?? DEFAULT_MIN_LINES_SINCE_SYNC;
  const workspacePath = params.workspacePath ?? null;
  const artifact = normalizeArtifact(params.artifact);
  const entries = params.targets.map((target) => {
    const docLastSyncedAt = lastEditTimestamp(db2, target.docFile, workspacePath, artifact);
    const activity = sourceActivitySince(db2, target.sourceDirs, docLastSyncedAt, workspacePath, artifact);
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
function proposeDocRefresh(db2, entry, params) {
  const sinceLabel = entry.doc_last_synced_at ?? "doc was last tracked (no prior edit_log record)";
  return insertHarnessLog(db2, {
    agentId: params.agentId,
    sessionId: params.sessionId ?? null,
    workspacePath: params.workspacePath ?? null,
    artifact: params.artifact ?? null,
    eventType: "propose",
    payload: {
      failure_signature: "doc-staleness",
      target_file: entry.doc_file,
      proposed_change: `Refresh ${entry.doc_file} \u2014 ${entry.edits_since_sync} edit(s) / ${entry.lines_changed_since_sync} line(s) changed across ${entry.source_dirs.join(", ")} since ${sinceLabel}.`,
      evidence: {
        edits_since_sync: entry.edits_since_sync,
        lines_changed_since_sync: entry.lines_changed_since_sync,
        files_touched: entry.files_touched,
        doc_last_synced_at: entry.doc_last_synced_at,
        latest_source_edit_at: entry.latest_source_edit_at
      }
    }
  });
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
   WHERE state IN ('open','ongoing') AND quality <> 'handoff'
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;
var REFINEMENTS_SELECT_BY_WORKSPACE = `SELECT ${COLS} FROM refinements
   WHERE (workspace_path = ? OR workspace_path IS NULL)
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;
var REFINEMENTS_DELETE = `DELETE FROM refinements WHERE refinement_id IN `;

// src/refinements.ts
function insertRefinement(db2, params) {
  const {
    agentId = "agent",
    reasoning,
    remember,
    quality = "good",
    state = "open",
    workspacePath,
    artifact,
    repo: repoArg,
    ref: refArg,
    files = [],
    cwd
  } = params;
  const refinementId = "ref_" + randomUUID3().replace(/-/g, "");
  const now = utcNow();
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  db2.prepare(REFINEMENTS_INSERT).run(
    refinementId,
    agentId,
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
      agent_id: agentId,
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
function getRefinements(db2, params = {}) {
  const {
    workspacePath,
    artifact,
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
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const queryParams = [...states];
  const stateFilter = `state IN (${states.map(() => "?").join(",")})`;
  let sql = `SELECT * FROM refinements WHERE ${stateFilter}`;
  if (quality) {
    sql += " AND quality = ?";
    queryParams.push(quality);
  } else if (!includeHandoffs) {
    sql += " AND quality <> 'handoff'";
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
  const rows = db2.prepare(sql).all(...queryParams);
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
  return { count: refinements.length, refinements };
}
function updateRefinement(db2, params) {
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
  const r = db2.prepare(
    `UPDATE refinements SET ${sets.join(", ")} WHERE refinement_id = ?`
  ).run(...binds, refinementId);
  if (r.changes === 0) return { updated: false, refinement: null };
  const row = db2.prepare("SELECT * FROM refinements WHERE refinement_id = ?").get(refinementId);
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
function deleteRefinement(db2, params) {
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
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact);
  }
  const rows = db2.prepare(
    `SELECT refinement_id FROM refinements WHERE ${where.join(" AND ")}`
  ).all(...binds);
  const ids = rows.map((r) => r.refinement_id);
  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, refinement_ids: ids };
  }
  if (ids.length > 0) {
    const delPh = ids.map(() => "?").join(",");
    db2.prepare(`${REFINEMENTS_DELETE}(${delPh})`).run(...ids);
  }
  return { deleted: ids.length, refinement_ids: ids };
}

// src/intents.ts
import { randomUUID as randomUUID4 } from "node:crypto";
import { isAbsolute, resolve as resolve3 } from "node:path";
var MAX_LOCK_TTL_MS = 10 * 6e4;
function effectiveTtlMs(ttlMs) {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}
function expiresAtFromNow(ttlMs) {
  return new Date(Date.now() + effectiveTtlMs(ttlMs)).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function workspaceRoot(workspacePath) {
  return workspacePath ? resolve3(workspacePath) : process.cwd();
}
function resolveTargetFiles(targetFiles = [], workspacePath) {
  const root = workspaceRoot(workspacePath);
  return targetFiles.map((file) => isAbsolute(file) ? resolve3(file) : resolve3(root, file));
}
function preFlightIntent(db2, params) {
  const {
    agentId = "agent",
    sessionId = null,
    workspacePath,
    artifact,
    rationale = "agent write operation",
    testPlan = "post-edit verification",
    targetFiles = [],
    lockType = "EXCLUSIVE",
    ttlMs = MAX_LOCK_TTL_MS
  } = params;
  const taskId = "task_" + randomUUID4().replace(/-/g, "");
  const now = utcNow();
  const wsPath = workspaceRoot(workspacePath);
  const artifactScope = normalizeArtifact(artifact);
  const absFiles = resolveTargetFiles(targetFiles, wsPath);
  evictExpiredLocks(db2);
  db2.exec("BEGIN IMMEDIATE");
  try {
    const conflicts = [];
    for (const absPath of absFiles) {
      const conflictMode = lockType === "SHARED" ? "fl.lock_type = 'EXCLUSIVE'" : "1 = 1";
      const existing = db2.prepare(`
        SELECT fl.*, ai.agent_id AS task_agent_id FROM locks fl
        JOIN tasks ai ON ai.task_id = fl.task_id
        WHERE fl.file_path = ?
          AND ai.agent_id <> ?
          AND ai.status = 'ACTIVE'
          AND ${conflictMode}
          AND (fl.expires_at IS NULL OR fl.expires_at > ?)
      `).all(absPath, agentId, now);
      conflicts.push(...existing);
    }
    if (conflicts.length > 0) {
      db2.exec("ROLLBACK");
      return {
        ok: false,
        conflict: true,
        conflicts: conflicts.map((c) => ({
          file_path: c.file_path,
          lock_type: c.lock_type,
          agent_id: c.task_agent_id ?? c.agent_id,
          acquired_at: c.acquired_at,
          expires_at: c.expires_at
        }))
      };
    }
    if (sessionId) {
      db2.prepare(
        `INSERT OR IGNORE INTO sessions (session_id, agent_id, workspace_path, artifact, started_at)
         VALUES (?, ?, ?, ?, ?)`
      ).run(sessionId, agentId, wsPath, artifactScope, now);
    }
    db2.prepare(`
      INSERT INTO tasks
        (task_id, agent_id, session_id, rationale, test_plan, status, workspace_path, artifact, files_json, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?)
    `).run(taskId, agentId, sessionId, rationale, testPlan, wsPath, artifactScope, JSON.stringify(absFiles), now, now);
    const expiresAt = expiresAtFromNow(ttlMs);
    const acquiredLocks = [];
    for (const absPath of absFiles) {
      const lockId = "lock_" + randomUUID4().replace(/-/g, "");
      db2.prepare(`
        INSERT OR REPLACE INTO locks
          (lock_id, file_path, task_id, agent_id, session_id, lock_type, acquired_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(lockId, absPath, taskId, agentId, sessionId, lockType, now, expiresAt);
      acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
    }
    db2.exec("COMMIT");
    return {
      ok: true,
      task: {
        task_id: taskId,
        agent_id: agentId,
        session_id: sessionId,
        lock_type: lockType,
        workspace_path: wsPath,
        artifact: artifactScope,
        target_files: absFiles,
        locks: acquiredLocks.map((l) => ({
          lock_id: l.lock_id,
          file_path: l.file_path,
          lock_type: l.lock_type,
          agent_id: agentId,
          session_id: sessionId,
          acquired_at: now,
          expires_at: l.expires_at
        })),
        status: "ACTIVE",
        created_at: now
      }
    };
  } catch (e) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
}
function releaseFileLock(db2, params) {
  const {
    agentId = "agent",
    sessionId = null,
    workspacePath = null,
    artifact = null,
    taskId = null,
    targetFiles = [],
    status: statusArg = "SUCCESS",
    verified = false,
    verifiedNote
  } = params;
  const requestedSuccessWithoutVerification = statusArg === "SUCCESS" && !verified;
  const effectiveStatus = verified ? "SUCCESS" : requestedSuccessWithoutVerification ? "PENDING" : statusArg;
  const now = utcNow();
  const whereClauses = ["fl.agent_id = ?"];
  const whereParams = [agentId];
  if (sessionId) {
    whereClauses.push("fl.session_id = ?");
    whereParams.push(sessionId);
  }
  const artifactScope = normalizeArtifact(artifact);
  if (workspacePath || artifactScope) {
    whereClauses.push("ai.task_id = fl.task_id");
  }
  if (workspacePath) {
    whereClauses.push("ai.workspace_path = ?");
    whereParams.push(workspaceRoot(workspacePath));
  }
  if (artifactScope) {
    whereClauses.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    whereParams.push(artifactScope);
  }
  if (taskId) {
    whereClauses.push("fl.task_id = ?");
    whereParams.push(taskId);
  }
  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => "?").join(",");
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }
  const where = whereClauses.join(" AND ");
  const locks = db2.prepare(
    `SELECT fl.lock_id, fl.task_id, fl.file_path
       FROM locks fl${workspacePath || artifactScope ? ", tasks ai" : ""}
      WHERE ${where}`
  ).all(...whereParams);
  const deleteClauses = ["agent_id = ?"];
  const deleteParams = [agentId];
  if (sessionId) {
    deleteClauses.push("session_id = ?");
    deleteParams.push(sessionId);
  }
  if (taskId) {
    deleteClauses.push("task_id = ?");
    deleteParams.push(taskId);
  }
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => "?").join(",");
    deleteClauses.push(`file_path IN (${ph})`);
    deleteParams.push(...absFiles);
  }
  const taskIds = [.../* @__PURE__ */ new Set([
    ...taskId ? [taskId] : [],
    ...locks.map((l) => l.task_id)
  ])];
  db2.exec("BEGIN IMMEDIATE");
  try {
    const lockIds = locks.map((lock) => lock.lock_id);
    if (lockIds.length > 0) {
      db2.prepare(`DELETE FROM locks WHERE lock_id IN (${lockIds.map(() => "?").join(",")})`).run(...lockIds);
    } else if (taskId && !workspacePath && !artifactScope) {
      db2.prepare(`DELETE FROM locks WHERE ${deleteClauses.join(" AND ")}`).run(...deleteParams);
    }
    for (const tid of taskIds) {
      const remaining = db2.prepare("SELECT 1 FROM locks WHERE task_id = ? LIMIT 1").get(tid);
      if (!remaining) {
        db2.prepare(
          "UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND agent_id = ?"
        ).run(effectiveStatus, now, tid, agentId);
        if (verified && verifiedNote) {
          try {
            db2.prepare(
              `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
               VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
            ).run("evt_" + randomUUID4().replace(/-/g, ""), tid, agentId, verifiedNote, now);
          } catch {
          }
        }
      }
    }
    db2.exec("COMMIT");
  } catch (e) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
  return {
    agent_id: agentId,
    status: effectiveStatus,
    released: locks.length > 0 || Boolean(taskId),
    locks_released: locks.length,
    task_ids: taskIds,
    updated_at: now,
    ...requestedSuccessWithoutVerification ? { unverifiedConclusion: "SUCCESS requested without --verified; stored as PENDING until verify records the test result." } : {}
  };
}

// src/reflect.ts
import { resolve as resolve4 } from "node:path";
var VALID_OUTCOMES = ["worked", "partial", "failed"];
var NEXT_MSG = "memory_refine_get \u2192 repo fixes for the next agent \xB7 mine-weakness (CLI) \u2192 recurring failures \xB7 memory_digest export_doc:true \u2192 preview harness improvements. A human merges.";
function normalizeScopePaths(paths = [], prefix, baseCwd) {
  const base = baseCwd ?? process.cwd();
  return [...new Set(paths.filter(Boolean).map((p) => {
    const abs = p.startsWith("/") ? p : resolve4(base, p);
    return `${prefix}:${abs}`;
  }))];
}
function reflect(db2, params) {
  const {
    agentId = "agent",
    task,
    outcome,
    lesson,
    worked,
    didntWork,
    fixRepo,
    fixHarness,
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
    artifact,
    repo: repoArg,
    ref: refArg,
    cwd
  } = params;
  const resolvedOutcome = VALID_OUTCOMES.includes(outcome ?? "") ? outcome : "partial";
  const bits = [`[reflection:${resolvedOutcome}] ${task}`];
  if (worked) bits.push(`worked: ${worked}`);
  if (didntWork) bits.push(`didn't work: ${didntWork}`);
  if (judgmentNote) bits.push(`judgment: ${judgmentNote}`);
  if (fixHarness) bits.push(`harness fix: ${fixHarness}`);
  const narrative = bits.join(" | ");
  const observation = lesson ? bits.length > 1 ? `${lesson}  (${narrative})` : lesson : narrative;
  const importance = impArg != null ? Number(impArg) : REFLECTION_IMPORTANCE[resolvedOutcome] ?? 5;
  const hasEvalFailures = evalFailures.length > 0;
  const tags = [
    "reflection",
    resolvedOutcome,
    ...fixHarness ? ["harness"] : [],
    ...hasEvalFailures ? ["eval"] : []
  ];
  const failSig = failSigArg ?? evalFailures.find((f) => f.failure_signature)?.failure_signature ?? null;
  const sig = failSig ?? (resolvedOutcome === "failed" && fixHarness ? "harness:reflection|outcome:failed" : null);
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const scopeReferences = [
    ...references,
    ...normalizeScopePaths(file ? [file] : [], "file", cwd),
    ...normalizeScopePaths(files, "file", cwd),
    ...normalizeScopePaths(folders, "dir", cwd)
  ];
  const { memoryId, similarMemoryIds, noveltyScore } = insertMemory(db2, {
    agentId,
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
    const { memoryId: evalMemId } = insertMemory(db2, {
      agentId,
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
    const { refinementId: rid } = insertRefinement(db2, {
      agentId,
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
  try {
    insertHarnessLog(db2, {
      agentId,
      eventType: "reflect",
      memoryId,
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      payload: {
        outcome: resolvedOutcome,
        novelty_score: noveltyScore,
        harness_fix: Boolean(fixHarness),
        refinement_id: refinementId,
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
import { spawnSync as spawnSync2 } from "node:child_process";
import { randomUUID as randomUUID6 } from "node:crypto";
import { isAbsolute as isAbsolute2, resolve as resolve5 } from "node:path";

// src/notifications.ts
import { randomUUID as randomUUID5 } from "node:crypto";

// src/sql/tasks.ts
var TASKS_SELECT_PENDING_IDS = `SELECT task_id FROM tasks WHERE status = 'PENDING' AND agent_id = ? {DYNAMIC_WHERE}`;
var TASKS_SELECT_STATUS = `SELECT agent_id, status FROM tasks WHERE task_id = ?`;
var TASKS_UPDATE_PENDING_VERIFIED = `UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND status = 'PENDING'`;
var TASKS_UPDATE_PENDING_VERIFIED_BY_AGENT = `UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND agent_id = ? AND status = 'PENDING'`;
var TASKS_UPDATE_PENDING_TO_FAILED = `UPDATE tasks SET status = 'FAILED', updated_at = ? WHERE task_id = ? AND status = 'PENDING'`;
var TASKS_UPDATE_ACTIVE_TO_FAILED = `UPDATE tasks SET status = 'FAILED', updated_at = ? WHERE task_id = ? AND status = 'ACTIVE'`;
var TASK_LOG_INSERT_VERIFIED = `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'VERIFIED', ?, ?)`;
var TASK_LOG_INSERT_ABANDONED = `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`;
var TASK_LOG_INSERT_STALE_ABANDONED = `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'stale active (no live locks) abandoned by audit-unverified --abandon', ?)`;

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
function insertNotification(db2, params) {
  const {
    agentId,
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
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd()
  );
  const signalId = "ntf_" + randomUUID5().replace(/-/g, "");
  const createdAt = utcNow();
  const wsPath = scope.workspace_path ?? process.cwd();
  let threadId;
  if (inReplyTo) {
    const parent = db2.prepare(SIGNALS_SELECT_THREAD_ID).get(inReplyTo);
    if (!parent) {
      throw new Error(`insertNotification: parent signal ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
    }
    threadId = parent.thread_id;
  } else {
    threadId = signalId;
  }
  db2.prepare(SIGNALS_INSERT).run(
    signalId,
    wsPath,
    scope.artifact,
    scope.repo,
    scope.ref,
    agentId,
    toAgent,
    kind,
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
function getNotifications(db2, params) {
  const {
    agentId,
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
    binds.push(agentId);
    where.push("n.from_agent <> ?");
    binds.push(agentId);
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
  const allBinds = unreadOnly ? [agentId, ...binds] : binds;
  const sql = `
    ${SIGNALS_SELECT_BASE}
    ${joinClause}
    ${whereClause}
    ${SIGNALS_SELECT_ORDER_LIMIT}
  `;
  const rows = db2.prepare(sql).all(...allBinds, limit);
  const signals = rows.map(rowToNotification);
  if (markRead && signals.length > 0) {
    const now = utcNow();
    const insertRead = db2.prepare(SIGNAL_READS_INSERT_IGNORE);
    for (const n of signals) {
      insertRead.run(n.signal_id, agentId, now);
    }
  }
  return { count: signals.length, signals, unread_only: unreadOnly };
}
function resolveNotification(db2, params) {
  const { notificationIds = [], threadId = null, cwd } = params;
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    cwd ?? process.cwd()
  );
  const resolved = [];
  const now = utcNow();
  if (notificationIds.length > 0) {
    const ph = notificationIds.map(() => "?").join(",");
    const where = [`signal_id IN (${ph})`, "status = 'open'"];
    const binds = [...notificationIds];
    const rows = db2.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(" AND ")} RETURNING signal_id`
    ).all(now, ...binds);
    resolved.push(...rows.map((r) => r.signal_id));
  }
  if (threadId) {
    const where = ["thread_id = ?", "status = 'open'"];
    const binds = [threadId];
    appendSignalScope(where, binds, scope, "");
    const rows = db2.prepare(
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
function acknowledgeNotifications(db2, agentId, notificationIds = [], threadId = null, params = {}) {
  const where = ["status = 'open'", "(to_agent IS NULL OR to_agent = ?)", "from_agent <> ?"];
  const binds = [agentId, agentId];
  if (notificationIds.length > 0) {
    where.push(`signal_id IN (${notificationIds.map(() => "?").join(",")})`);
    binds.push(...notificationIds);
  }
  if (threadId) {
    where.push("thread_id = ?");
    binds.push(threadId);
  }
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    params.cwd ?? process.cwd()
  );
  if (notificationIds.length === 0) {
    appendSignalScope(where, binds, scope, "");
  }
  const rows = db2.prepare(`SELECT signal_id FROM signals WHERE ${where.join(" AND ")}`).all(...binds);
  const ids = rows.map((r) => r.signal_id);
  const uniqueIds = [...new Set(ids)];
  if (uniqueIds.length === 0) return { acknowledged: 0, signal_ids: [] };
  const now = utcNow();
  const insertRead = db2.prepare(SIGNAL_READS_INSERT_IGNORE);
  let acknowledged = 0;
  for (const id of uniqueIds) {
    const result = insertRead.run(id, agentId, now);
    acknowledged += result.changes;
  }
  return { acknowledged, signal_ids: uniqueIds };
}
function agentSignal(db2, params) {
  switch (params.action) {
    case "publish":
    case "reply": {
      const toAgents = params.toAgents?.length ? params.toAgents : [null];
      const results = toAgents.map((toAgent) => insertNotification(db2, {
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
      const result = getNotifications(db2, {
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
      const result = resolveNotification(db2, {
        notificationIds: params.notificationIds ?? [],
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
        ...acknowledgeNotifications(db2, params.agentId, params.notificationIds ?? [], params.threadId ?? null, {
          workspacePath: params.workspacePath,
          artifact: params.artifact,
          cwd: params.cwd
        })
      };
    }
  }
}
function pruneNotifications(db2, params) {
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
  const rows = db2.prepare(
    `${SIGNALS_SELECT_IDS_FOR_PRUNE} ${whereClause}`
  ).all(...binds);
  const ids = rows.map((r) => r.signal_id);
  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, signal_ids: ids };
  }
  if (ids.length > 0) {
    const ph = ids.map(() => "?").join(",");
    db2.prepare(SIGNALS_DELETE_BY_IDS(ph)).run(...ids);
    db2.prepare(SIGNAL_READS_DELETE_BY_SIGNAL_IDS(ph)).run(...ids);
  }
  return { deleted: ids.length, signal_ids: ids };
}

// src/maintenance.ts
function pruneStale(db2, params = {}) {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) : params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId = typeof params.agent_id === "string" ? params.agent_id : typeof params.agentId === "string" ? params.agentId : null;
  const workspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const artifact = normalizeArtifact(params.artifact);
  const rawTarget = params.target_file ?? params.targetFile;
  const targetFiles = (Array.isArray(rawTarget) ? rawTarget : rawTarget != null ? [rawTarget] : []).map(String).filter(Boolean);
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
  if (agentId) {
    conditions.push("l.agent_id = ?");
    binds.push(agentId);
  }
  if (targetFiles.length > 0) {
    conditions.push(`l.file_path IN (${targetFiles.map(() => "?").join(",")})`);
    binds.push(...targetFiles);
  }
  const scopedByTask = Boolean(workspacePath || artifact);
  if (workspacePath) {
    conditions.push("t.workspace_path = ?");
    binds.push(resolve5(workspacePath));
  }
  if (artifact) {
    conditions.push("(t.artifact = ? OR t.artifact IS NULL)");
    binds.push(artifact);
  }
  const where = conditions.join(" AND ");
  let staleLocks = [];
  try {
    const from = scopedByTask ? "locks l JOIN tasks t ON t.task_id = l.task_id" : "locks l";
    staleLocks = db2.prepare(
      `SELECT l.lock_id, l.task_id FROM ${from} WHERE ${where}`
    ).all(...binds);
  } catch {
  }
  if (dryRun) {
    return { pruned_locks: 0, updated_tasks: 0, dry_run: true, would_prune: staleLocks.length };
  }
  if (staleLocks.length === 0) {
    return { pruned_locks: 0, updated_tasks: 0 };
  }
  const affectedTaskIds = [...new Set(staleLocks.map((l) => l.task_id))];
  let updatedTasks = 0;
  db2.exec("BEGIN IMMEDIATE");
  try {
    const ph = staleLocks.map(() => "?").join(",");
    db2.prepare(`DELETE FROM locks WHERE lock_id IN (${ph})`).run(...staleLocks.map((l) => l.lock_id));
    for (const tid of affectedTaskIds) {
      const remaining = db2.prepare("SELECT 1 FROM locks WHERE task_id = ? LIMIT 1").get(tid);
      if (!remaining) {
        const r = db2.prepare(
          "UPDATE tasks SET status = 'PENDING', updated_at = ? WHERE task_id = ? AND status = 'ACTIVE'"
        ).run(now, tid);
        if (r.changes) updatedTasks++;
      }
    }
    db2.exec("COMMIT");
  } catch (e) {
    try {
      db2.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
  return { pruned_locks: staleLocks.length, updated_tasks: updatedTasks };
}
function openRefinementCount(db2, params = {}) {
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    params.cwd ?? process.cwd()
  );
  const queryParams = [];
  let sql = "SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";
  if (!params.includeHandoffs) sql += " AND quality <> 'handoff'";
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
  const artifact = normalizeArtifact(params.artifact);
  const format = params.format ?? "json";
  const agentId = String(params.agent_id ?? params.agentId ?? "agent");
  const notifyCwd = wsPath ?? params.cwd ?? process.cwd();
  const items = [];
  try {
    const inbox = getNotifications(db2, {
      agentId,
      workspacePath: wsPath,
      artifact,
      unreadOnly: true,
      markRead: false,
      limit: 5,
      cwd: notifyCwd
    });
    for (const n of inbox.signals) {
      const target = n.to_agent ? `to ${n.to_agent}` : "broadcast";
      const fileSuffix = n.files.length > 0 ? ` files=${n.files.join(", ")}` : "";
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
    if (artifact) {
      overrideConds.push("(artifact = ? OR artifact IS NULL)");
      overrideBinds.push(artifact);
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
    if (artifact) {
      conditions.push("(artifact = ? OR artifact IS NULL)");
      bindParams.push(artifact);
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
    if (artifact) {
      wkConditions.push("(artifact = ? OR artifact IS NULL)");
      wkParams.push(artifact);
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
    const refCount = openRefinementCount(db2, { workspacePath: wsPath, artifact, cwd: notifyCwd });
    if (refCount > 0) {
      items.push({ kind: "refinement", text: `\u{1F4CB} ${refCount} open refinement(s) pending` });
    }
  } catch {
  }
  if (items.length === 0) {
    return { ok: true, count: 0, notifications: [], schema_version: 1 };
  }
  const result = {
    ok: true,
    count: items.length,
    notifications: items,
    schema_version: 1
  };
  if (format === "hook") {
    const lines = [
      `\u{1F9E0} Memory brief (${items.length}):`,
      ...items.map((i) => `  \u2022 ${i.text}`)
    ];
    result.additionalContext = lines.join("\n");
  }
  return result;
}
function gitDirtyFiles(workspacePath) {
  if (!workspacePath) return [];
  try {
    const result = spawnSync2("git", ["-C", workspacePath, "status", "--short"], {
      encoding: "utf8",
      timeout: 5e3
    });
    if (result.status !== 0) return [];
    return String(result.stdout).split("\n").map((line) => line.trim()).filter(Boolean).map((line) => line.slice(3).trim()).filter(Boolean);
  } catch {
    return [];
  }
}
function sessionCapture(db2, params = {}) {
  const agentId = String(params.agent_id ?? params.agentId ?? "agent");
  const reason = params.reason ? String(params.reason) : null;
  const workspaceInput = params.workspace ?? params.workspace_path ?? params.workspacePath;
  const rawWorkspacePath = typeof workspaceInput === "string" && workspaceInput.trim() ? resolve5(workspaceInput.trim()) : null;
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
  const taskWorkspaceCandidates = [...new Set([workspacePath, rawWorkspacePath].filter((value) => Boolean(value)))];
  const artifact = scope.artifact;
  const workspacePlaceholders = taskWorkspaceCandidates.map(() => "?").join(",");
  const taskRows = db2.prepare(
    `SELECT task_id, rationale, test_plan, status, files_json, created_at, updated_at
     FROM tasks
     WHERE agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path IN (${workspacePlaceholders}) OR workspace_path IS NULL)
       AND (? IS NULL OR artifact = ? OR artifact IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId, ...taskWorkspaceCandidates, artifact, artifact);
  const files = [...new Set(taskRows.flatMap((row) => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeTasks = taskRows.filter((row) => row.status === "ACTIVE").length;
  const pendingTasks = taskRows.filter((row) => row.status === "PENDING").length;
  let consolidationOpportunities = 0;
  try {
    const cConds = ["novelty_score IS NOT NULL", "novelty_score < 0.2", "state = 'ACTIVE'"];
    const cBinds = [];
    if (workspacePath) {
      cConds.push("(workspace_path = ? OR workspace_path IS NULL)");
      cBinds.push(workspacePath);
    }
    if (artifact) {
      cConds.push("(artifact = ? OR artifact IS NULL)");
      cBinds.push(artifact);
    }
    consolidationOpportunities = db2.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE ${cConds.join(" AND ")}`
    ).get(...cBinds).c;
  } catch {
  }
  if (taskRows.length === 0 && dirtyFiles.length === 0) {
    return {
      ok: true,
      captured: false,
      refinement_id: null,
      pending_tasks: 0,
      active_tasks: 0,
      files: [],
      dirty_files: [],
      reason,
      consolidation_opportunities: consolidationOpportunities
    };
  }
  const now = utcNow();
  const refinementId = "ref_" + randomUUID6().replace(/-/g, "");
  const capturedFiles = [.../* @__PURE__ */ new Set([...files, ...dirtyFiles])];
  const statusSummary = taskRows.map((row) => {
    const rowFiles = parseJsonList(row.files_json);
    const fileSuffix = rowFiles.length > 0 ? ` files=${rowFiles.join(", ")}` : "";
    return `${row.status} ${row.task_id}: ${row.rationale}; verify=${row.test_plan}${fileSuffix}`;
  });
  const reasoning = [
    `Session capture for ${agentId}${reason ? ` (${reason})` : ""}.`,
    `Unresolved tasks: ${taskRows.length} (${activeTasks} active, ${pendingTasks} pending).`,
    dirtyFiles.length > 0 ? `Dirty files: ${dirtyFiles.join(", ")}.` : null,
    statusSummary.length > 0 ? `Task details: ${statusSummary.join(" | ")}` : null
  ].filter(Boolean).join(" ");
  const remember = [
    `Review session handoff for ${agentId}: ${activeTasks} active and ${pendingTasks} pending tasks remain.`,
    capturedFiles.length > 0 ? `Touched files: ${capturedFiles.join(", ")}.` : null,
    dirtyFiles.length > 0 ? "Check dirty git state before continuing." : null,
    pendingTasks > 0 ? "Run the recorded verification before claiming completion." : null
  ].filter(Boolean).join(" ");
  db2.prepare(
    `INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, repo, ref,
       artifact, files_json, reasoning, remember, quality, state, created_at, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'handoff', 'open', ?, ?)`
  ).run(
    refinementId,
    agentId,
    workspacePath,
    scope.repo,
    scope.ref,
    artifact,
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
    pending_tasks: pendingTasks,
    active_tasks: activeTasks,
    files: capturedFiles,
    dirty_files: dirtyFiles,
    reason,
    consolidation_opportunities: consolidationOpportunities
  };
}
function waitForLock(db2, params = {}) {
  const targetFiles = Array.isArray(params.target_files) ? params.target_files : Array.isArray(params.targetFiles) ? params.targetFiles : [];
  const agentId = params.agent_id ?? params.agentId ?? "agent";
  const workspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const artifact = normalizeArtifact(params.artifact);
  const waitMs = Number(params.wait_ms ?? params.waitMs ?? 6e4);
  const retryMs = Number(params.retry_interval_ms ?? params.retryIntervalMs ?? 5e3);
  const requestedLockType = String(
    params.requestedLockType ?? params.requested_lock_type ?? params.lockType ?? params.lock_type ?? "EXCLUSIVE"
  ).toUpperCase();
  const start = Date.now();
  if (targetFiles.length === 0) {
    return { ok: true, waited_ms: 0, lock_free: true };
  }
  const root = workspacePath ? resolve5(workspacePath) : process.cwd();
  const absTargetFiles = targetFiles.map((file) => isAbsolute2(file) ? resolve5(file) : resolve5(root, file));
  const ph = absTargetFiles.map(() => "?").join(",");
  const lockTypeFilter = requestedLockType === "EXCLUSIVE" ? "" : "AND fl.lock_type = 'EXCLUSIVE'";
  const scopeClauses = [];
  const scopeBinds = [];
  if (workspacePath) {
    scopeClauses.push("AND ai.workspace_path = ?");
    scopeBinds.push(root);
  }
  if (artifact) {
    scopeClauses.push("AND (ai.artifact = ? OR ai.artifact IS NULL)");
    scopeBinds.push(artifact);
  }
  const lockStmt = db2.prepare(
    `SELECT fl.file_path, ai.agent_id, fl.expires_at
     FROM locks fl
     JOIN tasks ai ON ai.task_id = fl.task_id
     WHERE fl.file_path IN (${ph})
       AND ai.agent_id <> ?
       AND ai.status = 'ACTIVE'
       ${lockTypeFilter}
       ${scopeClauses.join("\n       ")}
       AND (fl.expires_at IS NULL OR fl.expires_at > ?)`
  );
  const sleepBuf = new Int32Array(new SharedArrayBuffer(4));
  const checkLocks = () => {
    const now = (/* @__PURE__ */ new Date()).toISOString();
    return lockStmt.all(...absTargetFiles, agentId, ...scopeBinds, now);
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
function digest(db2, params = {}) {
  const retentionDays = Number(params.retention_days ?? 90);
  const handoffRetentionDays = Number(params.refinement_handoff_retention_days ?? params.refinementHandoffRetentionDays ?? 7);
  const doneRetentionDays = Number(params.refinement_done_retention_days ?? params.refinementDoneRetentionDays ?? 30);
  const now = (/* @__PURE__ */ new Date()).toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 864e5).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 864e5).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 864e5).toISOString();
  const refinementRetentionSql = `SELECT COUNT(*) AS c FROM refinements
     WHERE (quality = 'handoff' AND updated_at < ?)
        OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?)`;
  if (params.dry_run) {
    const wouldArchive = db2.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
    ).get(now).c;
    const wouldPruneOld = db2.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE state = 'SUPERSEDED' AND updated_at < ?`
    ).get(cutoff).c;
    const wouldPruneLocks = db2.prepare(
      `SELECT COUNT(*) AS c FROM locks WHERE expires_at IS NOT NULL AND expires_at < ?`
    ).get(now).c;
    const wouldPruneRefinements = db2.prepare(refinementRetentionSql).get(handoffCutoff, doneCutoff).c;
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      pruned_refinements: 0,
      fts_rebuilt: false,
      schema_version: 1,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements
    };
  }
  const archiveRes = db2.prepare(
    `UPDATE memories
     SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
     WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
  ).run(now, now, now);
  const deleteRes = db2.prepare(
    `DELETE FROM memories
     WHERE state = 'SUPERSEDED' AND updated_at < ?`
  ).run(cutoff);
  const { pruned_locks } = pruneStale(db2, {});
  const pruneRefinementsRes = db2.prepare(
    `DELETE FROM refinements
     WHERE (quality = 'handoff' AND updated_at < ?)
        OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?)`
  ).run(handoffCutoff, doneCutoff);
  let ftsRebuilt = false;
  try {
    if (hasFts(db2)) {
      rebuildFts(db2);
      ftsRebuilt = true;
    }
  } catch {
  }
  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks,
    pruned_refinements: pruneRefinementsRes.changes,
    fts_rebuilt: ftsRebuilt,
    schema_version: 1
  };
}
function getWorkspaceStatus(db2, params = {}) {
  const wsPath = params.workspace_path ?? null;
  const artifact = normalizeArtifact(params.artifact);
  evictExpiredLocks(db2);
  const memoryScope = ["state = 'ACTIVE'"];
  const memoryScopeParams = [];
  if (wsPath) {
    memoryScope.push("(workspace_path = ? OR workspace_path IS NULL)");
    memoryScopeParams.push(wsPath);
  }
  if (artifact) {
    memoryScope.push("(artifact = ? OR artifact IS NULL)");
    memoryScopeParams.push(artifact);
  }
  const activeMemories = db2.prepare(
    `SELECT COUNT(*) AS c FROM memories WHERE ${memoryScope.join(" AND ")}`
  ).get(...memoryScopeParams).c;
  const taskScopeParts = [];
  const taskScopeParams = [];
  if (wsPath) {
    taskScopeParts.push("workspace_path = ?");
    taskScopeParams.push(wsPath);
  }
  if (artifact) {
    taskScopeParts.push("(artifact = ? OR artifact IS NULL)");
    taskScopeParams.push(artifact);
  }
  const taskScope = taskScopeParts.length > 0 ? ` AND ${taskScopeParts.join(" AND ")}` : "";
  const pendingTasks = db2.prepare(
    `SELECT COUNT(*) AS c FROM tasks WHERE status = 'PENDING'${taskScope}`
  ).get(...taskScopeParams).c;
  const activeTasks = db2.prepare(
    `SELECT COUNT(*) AS c FROM tasks WHERE status = 'ACTIVE'${taskScope}`
  ).get(...taskScopeParams).c;
  const openRefinements = openRefinementCount(db2, {
    workspacePath: wsPath,
    artifact,
    repo: params.repo,
    cwd: params.cwd
  });
  const lockWhereParts = [];
  const lockParams = [];
  if (wsPath) {
    lockWhereParts.push("ai.workspace_path = ?");
    lockParams.push(wsPath);
  }
  if (artifact) {
    lockWhereParts.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    lockParams.push(artifact);
  }
  const lockWhere = lockWhereParts.length > 0 ? `WHERE ${lockWhereParts.join(" AND ")}` : "";
  const locks = db2.prepare(
    `SELECT fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact, fl.task_id,
            fl.lock_type, fl.acquired_at, fl.expires_at
     FROM locks fl
     JOIN tasks ai ON ai.task_id = fl.task_id
     ${lockWhere}
     ORDER BY fl.acquired_at DESC
     LIMIT 50`
  ).all(...lockParams);
  return {
    ok: true,
    active_memories: activeMemories,
    pending_tasks: pendingTasks,
    active_tasks: activeTasks,
    open_refinements: openRefinements,
    locks,
    schema_version: 1
  };
}
function exportMemoryDoc(db2, params = {}) {
  const wsPath = params.workspace_path ?? null;
  const artifact = normalizeArtifact(params.artifact);
  const now = (/* @__PURE__ */ new Date()).toISOString().slice(0, 10);
  const conds = ["state = 'ACTIVE'"];
  const bindParams = [];
  if (wsPath) {
    conds.push("(workspace_path = ? OR workspace_path IS NULL)");
    bindParams.push(wsPath);
  }
  if (artifact) {
    conds.push("(artifact = ? OR artifact IS NULL)");
    bindParams.push(artifact);
  }
  const rows = db2.prepare(
    `SELECT memory_id, label, importance, task_context, observation,
            tags_json, repo, ref, failure_signature, created_at
     FROM memories
     WHERE ${conds.join(" AND ")}
     ORDER BY importance DESC, created_at DESC`
  ).all(...bindParams);
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
      if (m.failure_signature) lines.push(`**Failure signature:** ${m.failure_signature}`);
      if (m.repo) lines.push(`**Repo:** ${m.repo}${m.ref ? ` @ ${m.ref}` : ""}`);
      lines.push(`**Created:** ${m.created_at.slice(0, 10)}`, "");
    }
  }
  return lines.join("\n");
}
function exportHarness(db2, params = {}) {
  const limit = Number(params.limit ?? 10);
  const minImportance = Number(params.min_importance ?? params.minImportance ?? 7);
  const wsPath = params.workspace_path ?? null;
  const artifact = normalizeArtifact(params.artifact);
  const harnessOnly = Boolean(params.harness_only ?? params.harnessOnly ?? false);
  const scopeConds = [];
  const scopeParams = [];
  if (wsPath) {
    scopeConds.push("(workspace_path = ? OR workspace_path IS NULL)");
    scopeParams.push(wsPath);
  }
  if (artifact) {
    scopeConds.push("(artifact = ? OR artifact IS NULL)");
    scopeParams.push(artifact);
  }
  const scopeSql = scopeConds.length > 0 ? `AND ${scopeConds.join(" AND ")}` : "";
  const harnessRows = db2.prepare(
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
    const generalRows = db2.prepare(
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
    return { count: 0, harness_count: 0, markdown: "<!-- No harness or high-importance memories to export -->", memories: [] };
  }
  const harnessCount = memories.filter((m) => m.tier === "harness").length;
  const lines = [
    "## Agent lessons (generated by octocode-awareness \xB7 memory_digest export_doc:true)",
    "",
    "<!-- Tier 1: harness proposals from memory_reflect fix_harness: -->",
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
  return { count: memories.length, harness_count: harnessCount, markdown: lines.join("\n"), memories };
}

// src/verify.ts
import { randomUUID as randomUUID7 } from "node:crypto";
var VALID_VERIFY_STATUSES = /* @__PURE__ */ new Set(["SUCCESS", "FAILED"]);
function auditUnverified(db2, params = {}) {
  const where = ["status = 'PENDING'"];
  const binds = [];
  if (params.agentId) {
    where.push("agent_id = ?");
    binds.push(params.agentId);
  }
  if (params.workspacePath) {
    where.push("workspace_path = ?");
    binds.push(params.workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact);
  }
  const rows = db2.prepare(
    `SELECT task_id, agent_id, status, test_plan, rationale, workspace_path, artifact, files_json, created_at
     FROM tasks
     WHERE ${where.join(" AND ")}
     ORDER BY created_at ASC`
  ).all(...binds);
  const unverified = rows.map((r) => ({
    task_id: r.task_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    rationale: r.rationale,
    target_files: parseJsonList(r.files_json),
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at
  }));
  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db2.prepare(TASKS_UPDATE_PENDING_TO_FAILED).run(now, intent.task_id);
      try {
        db2.prepare(TASK_LOG_INSERT_ABANDONED).run(
          "evt_" + randomUUID7().replace(/-/g, ""),
          intent.task_id,
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
      `NOT EXISTS (
        SELECT 1 FROM locks fl
        WHERE fl.task_id = ai.task_id
          AND (fl.expires_at IS NULL OR fl.expires_at > ?)
      )`
    ];
    const staleBinds = [nowIso];
    if (params.agentId) {
      staleWhere.push("ai.agent_id = ?");
      staleBinds.push(params.agentId);
    }
    if (params.workspacePath) {
      staleWhere.push("ai.workspace_path = ?");
      staleBinds.push(params.workspacePath);
    }
    if (artifact) {
      staleWhere.push("(ai.artifact = ? OR ai.artifact IS NULL)");
      staleBinds.push(artifact);
    }
    const staleRows = db2.prepare(
      `SELECT ai.task_id, ai.agent_id, ai.rationale, ai.workspace_path, ai.artifact, ai.files_json, ai.created_at
       FROM tasks ai
       WHERE ${staleWhere.join(" AND ")}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds);
    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        task_id: r.task_id,
        agent_id: r.agent_id,
        status: "ACTIVE",
        rationale: r.rationale,
        target_files: parseJsonList(r.files_json),
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
      db2.prepare(TASKS_UPDATE_ACTIVE_TO_FAILED).run(now, intent.task_id);
      try {
        db2.prepare(TASK_LOG_INSERT_STALE_ABANDONED).run(
          "evt_" + randomUUID7().replace(/-/g, ""),
          intent.task_id,
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
function markVerified(db2, params) {
  const { agentId = "agent", allPending = false, workspacePath, message } = params;
  const artifact = normalizeArtifact(params.artifact);
  const taskId = params.taskId ?? "";
  const status = params.status ?? "SUCCESS";
  if (allPending) {
    const dynWhere = [
      workspacePath ? " AND workspace_path = ?" : "",
      artifact ? " AND (artifact = ? OR artifact IS NULL)" : ""
    ].join("");
    const selectSql = TASKS_SELECT_PENDING_IDS.replace("{DYNAMIC_WHERE}", dynWhere);
    const selectBinds = [agentId];
    if (workspacePath) selectBinds.push(workspacePath);
    if (artifact) selectBinds.push(artifact);
    const rows = db2.prepare(selectSql).all(...selectBinds);
    const now2 = utcNow();
    const ids = [];
    for (const row of rows) {
      db2.prepare(TASKS_UPDATE_PENDING_VERIFIED).run(status, now2, row.task_id);
      ids.push(row.task_id);
      if (message) {
        try {
          db2.prepare(TASK_LOG_INSERT_VERIFIED).run(
            "evt_" + randomUUID7().replace(/-/g, ""),
            row.task_id,
            agentId,
            message,
            now2
          );
        } catch {
        }
      }
    }
    return { ok: true, task_id: null, task_ids: ids, count: ids.length, status, updated_at: now2 };
  }
  if (!taskId) {
    return { ok: false, error: "--task-id is required (or use --all-pending)", task_id: null };
  }
  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" \u2014 must be SUCCESS or FAILED`,
      task_id: taskId
    };
  }
  const now = utcNow();
  const result = db2.prepare(TASKS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
    status,
    now,
    taskId,
    agentId
  );
  if (result.changes === 0) {
    const row = db2.prepare(TASKS_SELECT_STATUS).get(taskId);
    if (!row) {
      return { ok: false, error: `no task found with task_id=${taskId}`, task_id: taskId };
    }
    if (row.agent_id !== agentId) {
      return {
        ok: false,
        error: `task ${taskId} belongs to agent "${row.agent_id}", not "${agentId}"`,
        task_id: taskId
      };
    }
    return {
      ok: false,
      error: `task ${taskId} has status "${row.status}" \u2014 only PENDING tasks can be verified`,
      task_id: taskId
    };
  }
  if (message) {
    try {
      db2.prepare(TASK_LOG_INSERT_VERIFIED).run(
        "evt_" + randomUUID7().replace(/-/g, ""),
        taskId,
        agentId,
        message,
        now
      );
    } catch {
    }
  }
  return { ok: true, task_id: taskId, status, updated_at: now };
}

// bin/awareness.ts
if (parseInt(process.version.slice(1), 10) < 22) {
  process.stderr.write(`awareness requires Node >=22 (got ${process.version})
`);
  process.exit(1);
}
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
  "regex",
  "file_regex",
  "to_agent",
  "kind"
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
var KNOWN_FLAGS = {
  "tell-memory": ["agent_id", "task_context", "observation", "importance", "label", "tag", "reference", "supersedes", "failure_signature", "valid_from", "valid_to", "workspace", "artifact", "repo", "ref", "file", "file_tree_fingerprint"],
  "get-memory": ["query", "limit", "min_importance", "label", "tag", "smart", "workspace", "artifact", "repo", "ref", "state", "sort", "global_only", "strict_scope", "as_of", "reference", "regex", "file_regex", "file", "explain", "semantic"],
  "forget": ["memory_id", "tag", "tags", "before", "max_importance", "dry_run"],
  "reflect": ["agent_id", "task", "outcome", "lesson", "worked", "didnt_work", "fix_repo", "fix_file", "fix_harness", "failure_signature", "importance", "judgment_note", "duo", "eval_failure_json", "workspace", "artifact", "repo", "ref"],
  "refine-set": ["agent_id", "reasoning", "remember", "quality", "state", "workspace", "artifact", "repo", "ref", "file", "refinement_id"],
  "refine-get": ["workspace", "artifact", "repo", "ref", "quality", "include_handoffs", "state", "limit"],
  "refine-delete": ["refinement_id", "workspace", "artifact", "dry_run"],
  "pre-flight-intent": ["agent_id", "workspace", "artifact", "rationale", "test_plan", "plan_doc_ref", "target_file", "file", "lock_type", "ttl_minutes", "ttl_seconds", "wait_seconds"],
  "release-file-lock": ["agent_id", "task_id", "target_file", "file", "status", "verified", "verified_note", "workspace", "artifact"],
  "status": ["workspace", "artifact", "limit"],
  "workspace-status": ["workspace", "artifact"],
  "init": [],
  "self-test": [],
  "prune-stale-locks": ["older_than_minutes", "expired_only", "agent_id", "target_file", "workspace", "artifact", "dry_run"],
  "audit-unverified": ["agent_id", "workspace", "artifact", "abandon"],
  "verify": ["task_id", "all_pending", "agent_id", "status", "message", "workspace", "artifact"],
  "mine-weakness": ["agent_id", "workspace", "artifact", "min_count", "limit", "cwd"],
  "doc-staleness": ["agent_id", "workspace", "artifact", "targets_json", "min_edits", "min_lines", "propose", "session_id"],
  "export-harness": ["limit", "min_importance", "workspace", "artifact"],
  "memory-index": ["limit", "min_importance", "out", "stdout", "workspace", "artifact", "repo", "ref"],
  "notify": ["agent_id", "to", "kind", "subject", "body", "file", "ref_id", "in_reply_to", "importance", "workspace", "artifact", "repo", "ref"],
  "agent-signal": ["action", "agent_id", "workspace", "artifact", "repo", "ref", "kind", "subject", "body", "to_agent", "file", "ref_id", "importance", "in_reply_to", "thread_id", "signal_id", "all", "mark_read", "limit"],
  "notify-get": ["agent_id", "workspace", "artifact", "repo", "ref", "all", "mark_read", "kind", "thread_id", "limit", "format"],
  "notify-resolve": ["signal_id", "thread_id", "workspace", "artifact"],
  "notify-prune": ["signal_id", "resolved", "older_than_days", "dry_run", "workspace", "artifact"],
  "session-capture": ["agent_id", "workspace", "artifact", "repo", "ref", "reason", "cwd"],
  "wait-for-lock": ["agent_id", "target_file", "file", "workspace", "artifact", "lock_type", "wait_seconds", "retry_interval"],
  "digest": ["retention_days", "dry_run", "export_doc", "workspace", "artifact"]
};
function validateFlags(command2, args2) {
  const known = KNOWN_FLAGS[command2];
  if (!known) return [];
  const allowed = /* @__PURE__ */ new Set([...known, ...GLOBAL_FLAGS]);
  return Object.keys(args2).filter((k) => k !== "_" && !allowed.has(k));
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
function emit(payload, exitCode2 = 0, opts2 = {}) {
  payload["ok"] = payload["ok"] ?? exitCode2 === 0;
  payload["schema_version"] = 1;
  const compact2 = opts2.compact === true || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
  process.stdout.write((compact2 ? JSON.stringify(payload) : JSON.stringify(payload, null, 2)) + "\n");
  return exitCode2;
}
function die(message, extras = {}) {
  process.stdout.write(JSON.stringify({ ok: false, error: message, schema_version: 1, ...extras }, null, 2) + "\n");
  process.exit(1);
}
function cmdTellMemory(db2, args2, dbPath2, opts2) {
  const agentId = String(args2["agent_id"] ?? "agent");
  const taskContext = String(args2["task_context"] ?? "");
  const observation = String(args2["observation"] ?? "");
  const importanceScore = args2["importance"];
  if (!taskContext) die("--task-context is required");
  if (!observation) die("--observation is required");
  const imp = parseInt(String(importanceScore), 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die("--importance must be 1\u201310");
  const rawTag = args2["tag"];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawRef = args2["reference"];
  const references = Array.isArray(rawRef) ? rawRef : rawRef ? [String(rawRef)] : [];
  const rawSup = args2["supersedes"];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args2["label"];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? "");
  const { memory, superseded, noveltyScore, similarMemoryIds } = insertMemory(db2, {
    agentId,
    taskContext,
    observation,
    importance: imp,
    label: normalizeLabel(label),
    tags,
    references,
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
  return emit(payload, 0, opts2);
}
function cmdGetMemory(db2, args2, dbPath2, opts2) {
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
  const result = getMemory(db2, {
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
    sort: String(args2["sort"] ?? "smart"),
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
  if (args2["semantic"]) {
    payload["warnings"] = [
      "semantic ranking is unavailable in the CLI (no embedding source); results use lexical FTS + decay. Use the library storeEmbedding()/semanticSearch() API for semantic recall."
    ];
  }
  return emit(payload, 0, opts2);
}
function cmdRefineSet(db2, args2, dbPath2, opts2) {
  const rawState = args2["state"];
  const stateVal = Array.isArray(rawState) ? rawState[0] : String(rawState ?? "open");
  const rawFile = args2["file"];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];
  const rawRefId = args2["refinement_id"];
  const refinementId = Array.isArray(rawRefId) ? rawRefId[0] : rawRefId ? String(rawRefId) : null;
  if (refinementId && refinementId !== "true") {
    const update = updateRefinement(db2, {
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
  const { refinement } = insertRefinement(db2, {
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
function cmdRefineGet(db2, args2, dbPath2, opts2) {
  const rawState = args2["state"];
  const states = rawState ? Array.isArray(rawState) ? rawState : [String(rawState)] : void 0;
  const result = getRefinements(db2, {
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
function cmdReflect(db2, args2, dbPath2, opts2) {
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
  const result = reflect(db2, {
    agentId: String(args2["agent_id"] ?? "agent"),
    task: String(args2["task"]),
    outcome: String(args2["outcome"] ?? "partial"),
    lesson: args2["lesson"] ? String(args2["lesson"]) : null,
    worked: args2["worked"] ? String(args2["worked"]) : null,
    didntWork: args2["didnt_work"] ? String(args2["didnt_work"]) : null,
    fixRepo: args2["fix_repo"] ? String(args2["fix_repo"]) : null,
    fixHarness: args2["fix_harness"] ? String(args2["fix_harness"]) : null,
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
function cmdPreFlightIntent(db2, args2, dbPath2, opts2) {
  const rawTarget = args2["target_file"] ?? args2["file"];
  const targetFiles = Array.isArray(rawTarget) ? rawTarget : rawTarget ? [String(rawTarget)] : [];
  const ttlMinutes = args2["ttl_minutes"] ? parseInt(String(args2["ttl_minutes"]), 10) : null;
  const ttlSeconds = args2["ttl_seconds"] ? parseInt(String(args2["ttl_seconds"]), 10) : null;
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1)) die("--ttl-minutes must be >= 1");
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1)) die("--ttl-seconds must be >= 1");
  const ttlMs = ttlMinutes != null ? ttlMinutes * 6e4 : ttlSeconds != null ? ttlSeconds * 1e3 : null;
  const claimParams = {
    agentId: String(args2["agent_id"] ?? "agent"),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    rationale: String(args2["rationale"] ?? "agent write operation"),
    testPlan: String(args2["test_plan"] ?? "post-edit verification"),
    targetFiles,
    lockType: String(args2["lock_type"] ?? "EXCLUSIVE"),
    ttlMs
  };
  let result = preFlightIntent(db2, claimParams);
  const waitSeconds = args2["wait_seconds"] ? parseInt(String(args2["wait_seconds"]), 10) : null;
  if (!result.ok && waitSeconds != null && waitSeconds > 0) {
    const wait = waitForLock(db2, {
      agent_id: claimParams.agentId,
      target_files: targetFiles,
      workspace: claimParams.workspacePath ?? void 0,
      artifact: claimParams.artifact ?? void 0,
      lock_type: claimParams.lockType,
      wait_ms: waitSeconds * 1e3
    });
    if (wait.lock_free) result = preFlightIntent(db2, claimParams);
  }
  if (!result.ok) return emit({ db_path: dbPath2, ...result }, 2, opts2);
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdAuditUnverified(db2, args2, dbPath2, opts2) {
  const result = auditUnverified(db2, {
    agentId: args2["agent_id"] ? String(args2["agent_id"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    abandon: Boolean(args2["abandon"])
  });
  return emit({ db_path: dbPath2, ...result }, result.count > 0 ? 1 : 0, opts2);
}
function cmdVerify(db2, args2, dbPath2, opts2) {
  const allPending = Boolean(args2["all_pending"]);
  if (!allPending && !args2["task_id"]) {
    return emit({ error: "--task-id is required (or use --all-pending)" }, 1, opts2);
  }
  const statusArg = args2["status"] ? String(args2["status"]) : "SUCCESS";
  if (statusArg !== "SUCCESS" && statusArg !== "FAILED") {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts2);
  }
  const result = markVerified(db2, {
    taskId: args2["task_id"] ? String(args2["task_id"]) : void 0,
    agentId: String(args2["agent_id"] ?? "agent"),
    allPending,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    message: args2["message"] ? String(args2["message"]) : void 0,
    status: statusArg
  });
  return emit({ db_path: dbPath2, ...result }, result.ok ? 0 : 1, opts2);
}
function cmdReleaseFileLock(db2, args2, dbPath2, opts2) {
  const rawTarget = args2["target_file"] ?? args2["file"];
  const targetFiles = rawTarget ? Array.isArray(rawTarget) ? rawTarget : [String(rawTarget)] : [];
  if (!args2["task_id"] && targetFiles.length === 0) {
    return emit({ error: "release-file-lock requires --task-id or --target-file" }, 1, opts2);
  }
  const result = releaseFileLock(db2, {
    agentId: String(args2["agent_id"] ?? "agent"),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    taskId: args2["task_id"] ? String(args2["task_id"]) : null,
    targetFiles,
    status: String(args2["status"] ?? "SUCCESS"),
    verified: Boolean(args2["verified"]),
    verifiedNote: args2["verified_note"] ? String(args2["verified_note"]) : void 0
  });
  if ("unverifiedConclusion" in result) {
    return emit({ db_path: dbPath2, ...result, ok: false }, 2, opts2);
  }
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdMemoryIndex(db2, args2, dbPath2, opts2) {
  const limit = args2["limit"] ? parseInt(String(args2["limit"]), 10) : 30;
  const minImportance = args2["min_importance"] ? parseInt(String(args2["min_importance"]), 10) : 1;
  const stdout = Boolean(args2["stdout"]);
  const wsPath = args2["workspace"] ? String(args2["workspace"]) : null;
  const conds = [];
  const binds = [minImportance];
  let sql = `SELECT memory_id, label, importance, task_context, observation, tags_json, created_at
     FROM memories WHERE state = 'ACTIVE' AND importance >= ?`;
  if (wsPath) {
    sql += " AND (workspace_path = ? OR workspace_path IS NULL)";
    binds.push(wsPath);
  }
  if (args2["artifact"]) {
    sql += " AND (artifact = ? OR artifact IS NULL)";
    binds.push(String(args2["artifact"]));
  }
  if (args2["repo"]) {
    sql += " AND (repo = ? OR repo IS NULL)";
    binds.push(String(args2["repo"]));
  }
  if (args2["ref"]) {
    sql += " AND (ref = ? OR ref IS NULL)";
    binds.push(String(args2["ref"]));
  }
  sql += " ORDER BY importance DESC, access_count DESC, last_accessed_at DESC LIMIT ?";
  binds.push(limit);
  void conds;
  const rows = db2.prepare(sql).all(...binds);
  const now = (/* @__PURE__ */ new Date()).toISOString().slice(0, 10);
  const lines = [
    `# Memory Index \u2014 ${now}`,
    `<!-- Auto-generated by awareness memory-index. Regenerate after recording or forgetting memories. -->`,
    "",
    `**${rows.length} active memories** (importance \u2265 ${minImportance}, sorted by salience)`,
    ""
  ];
  for (const m of rows) {
    const tags = (() => {
      try {
        return JSON.parse(m.tags_json).join(", ");
      } catch {
        return "";
      }
    })();
    lines.push(`## [${m.label}:${m.importance}] ${m.task_context.slice(0, 80)}`);
    lines.push(`> ${m.observation.slice(0, 200)}`);
    if (tags) lines.push(`*Tags: ${tags}*`);
    lines.push("");
  }
  const content = lines.join("\n");
  if (stdout) {
    process.stdout.write(content + "\n");
    return 0;
  }
  const outPath = args2["out"] ? String(args2["out"]) : null;
  const targetPath = outPath ?? resolveDbPath(null).replace("awareness.sqlite3", "MEMORY.md");
  try {
    mkdirSync2(dirname2(targetPath), { recursive: true });
    writeFileSync(targetPath, content, "utf8");
  } catch (err) {
    return emit({ db_path: dbPath2, error: `Could not write MEMORY.md: ${err.message}` }, 1, opts2);
  }
  return emit({ db_path: dbPath2, ok: true, path: targetPath, count: rows.length }, 0, opts2);
}
function cmdForget(db2, args2, dbPath2, opts2) {
  const rawIds = args2["memory_id"];
  const memoryIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const rawTags = [args2["tag"], args2["tags"]].flatMap((v) => Array.isArray(v) ? v : v && v !== true ? [String(v)] : []);
  const tags = rawTags;
  const result = forgetMemory(db2, {
    memoryIds,
    tags,
    before: args2["before"] ? String(args2["before"]) : void 0,
    maxImportance: args2["max_importance"] ? parseInt(String(args2["max_importance"]), 10) : void 0,
    dryRun: Boolean(args2["dry_run"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdRefineDelete(db2, args2, dbPath2, opts2) {
  const rawIds = args2["refinement_id"];
  const refinementIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  if (refinementIds.length === 0) return emit({ error: "--refinement-id is required" }, 1, opts2);
  const result = deleteRefinement(db2, {
    refinementIds,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : void 0,
    artifact: args2["artifact"] ? String(args2["artifact"]) : void 0,
    dryRun: Boolean(args2["dry_run"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdExportHarness(db2, args2, dbPath2, opts2) {
  const result = exportHarness(db2, {
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : void 0,
    min_importance: args2["min_importance"] ? parseInt(String(args2["min_importance"]), 10) : void 0,
    workspace_path: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdDocStaleness(db2, args2, dbPath2, opts2) {
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
  const artifact = args2["artifact"] ? String(args2["artifact"]) : null;
  const result = mineDocStaleness(db2, {
    targets,
    workspacePath,
    artifact,
    minEditsSinceSync: args2["min_edits"] ? Number(args2["min_edits"]) : void 0,
    minLinesSinceSync: args2["min_lines"] ? Number(args2["min_lines"]) : void 0
  });
  const proposed = [];
  if (Boolean(args2["propose"])) {
    const agentId = String(args2["agent_id"] ?? "agent");
    const sessionId = args2["session_id"] ? String(args2["session_id"]) : null;
    for (const entry of result.entries) {
      if (!entry.stale) continue;
      const harnessId = proposeDocRefresh(db2, entry, { agentId, sessionId, workspacePath, artifact });
      proposed.push({ target_file: entry.doc_file, harness_id: harnessId });
    }
  }
  return emit({ db_path: dbPath2, ...result, proposed }, 0, opts2);
}
function cmdNotify(db2, args2, dbPath2, opts2) {
  if (!args2["agent_id"]) return emit({ error: "--agent-id is required" }, 1, opts2);
  if (!args2["kind"]) return emit({ error: "--kind is required" }, 1, opts2);
  if (!args2["subject"]) return emit({ error: "--subject is required" }, 1, opts2);
  const rawFiles = args2["file"];
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [String(rawFiles)] : [];
  const rawRefIds = args2["ref_id"];
  const refIds = Array.isArray(rawRefIds) ? rawRefIds : rawRefIds ? [String(rawRefIds)] : [];
  const result = insertNotification(db2, {
    agentId: String(args2["agent_id"]),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    toAgent: args2["to"] ? String(args2["to"]) : null,
    kind: String(args2["kind"]),
    subject: String(args2["subject"]),
    body: args2["body"] ? String(args2["body"]) : null,
    files,
    refIds,
    inReplyTo: args2["in_reply_to"] ? String(args2["in_reply_to"]) : null,
    importance: args2["importance"] ? parseInt(String(args2["importance"]), 10) : 5
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdNotifyGet(db2, args2, dbPath2, opts2) {
  if (!args2["agent_id"]) return emit({ error: "--agent-id is required" }, 1, opts2);
  const rawKinds = args2["kind"];
  const kinds = Array.isArray(rawKinds) ? rawKinds : rawKinds ? [String(rawKinds)] : [];
  const result = getNotifications(db2, {
    agentId: String(args2["agent_id"]),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    kinds,
    threadId: args2["thread_id"] ? String(args2["thread_id"]) : null,
    unreadOnly: args2["all"] ? false : true,
    markRead: Boolean(args2["mark_read"]),
    limit: args2["limit"] ? parseInt(String(args2["limit"]), 10) : 20
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdNotifyResolve(db2, args2, dbPath2, opts2) {
  const rawIds = args2["signal_id"];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = resolveNotification(db2, {
    notificationIds,
    threadId: args2["thread_id"] ? String(args2["thread_id"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdAgentSignal(db2, args2, dbPath2, opts2) {
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
  const rawNotificationIds = args2["signal_id"];
  const notificationIds = Array.isArray(rawNotificationIds) ? rawNotificationIds : rawNotificationIds ? [String(rawNotificationIds)] : [];
  const result = agentSignal(db2, {
    action,
    agentId: String(args2["agent_id"]),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    kind: args2["kind"] && !Array.isArray(args2["kind"]) ? String(args2["kind"]) : void 0,
    subject: args2["subject"] ? String(args2["subject"]) : void 0,
    body: args2["body"] ? String(args2["body"]) : null,
    toAgents,
    files,
    refs,
    importance: args2["importance"] ? parseInt(String(args2["importance"]), 10) : void 0,
    inReplyTo: args2["in_reply_to"] ? String(args2["in_reply_to"]) : null,
    threadId: args2["thread_id"] ? String(args2["thread_id"]) : null,
    notificationIds,
    unreadOnly: args2["all"] ? false : args2["unread_only"],
    markRead: Boolean(args2["mark_read"]),
    kinds
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdNotifyPrune(db2, args2, dbPath2, opts2) {
  const rawIds = args2["signal_id"];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = pruneNotifications(db2, {
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    notificationIds,
    resolvedOnly: Boolean(args2["resolved"]),
    olderThanDays: args2["older_than_days"] ? parseInt(String(args2["older_than_days"]), 10) : void 0,
    dryRun: Boolean(args2["dry_run"])
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdStatus(db2, dbPath2, args2, opts2) {
  evictExpiredLocks(db2);
  const wsPath = args2["workspace"] ? String(args2["workspace"]) : null;
  const artifact = args2["artifact"] ? String(args2["artifact"]) : null;
  const memScope = [];
  const memScopeBinds = [];
  if (wsPath) {
    memScope.push("(workspace_path = ? OR workspace_path IS NULL)");
    memScopeBinds.push(wsPath);
  }
  if (artifact) {
    memScope.push("(artifact = ? OR artifact IS NULL)");
    memScopeBinds.push(artifact);
  }
  const memWhere = memScope.length > 0 ? `WHERE ${memScope.join(" AND ")}` : "";
  const memCount = db2.prepare(`SELECT COUNT(*) AS count FROM memories ${memWhere}`).get(...memScopeBinds).count;
  const memStates = Object.fromEntries(
    db2.prepare(`SELECT state, COUNT(*) AS count FROM memories ${memWhere} GROUP BY state`).all(...memScopeBinds).map((r) => [r.state, r.count])
  );
  const memLabels = Object.fromEntries(
    db2.prepare(`SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM memories ${memWhere} GROUP BY label`).all(...memScopeBinds).map((r) => [r.label, r.count])
  );
  const taskScope = ["status='ACTIVE'"];
  const taskBinds = [];
  if (wsPath) {
    taskScope.push("workspace_path = ?");
    taskBinds.push(wsPath);
  }
  if (artifact) {
    taskScope.push("(artifact = ? OR artifact IS NULL)");
    taskBinds.push(artifact);
  }
  const activeTasks = db2.prepare(`SELECT COUNT(*) AS count FROM tasks WHERE ${taskScope.join(" AND ")}`).get(...taskBinds).count;
  const limit = Math.min(100, Math.max(1, parseInt(String(args2["limit"] ?? "20"), 10) || 20));
  const lockWhere = [];
  const lockBinds = [];
  if (wsPath) {
    lockWhere.push("ai.workspace_path = ?");
    lockBinds.push(wsPath);
  }
  if (artifact) {
    lockWhere.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    lockBinds.push(artifact);
  }
  const locks = db2.prepare(
    `SELECT fl.file_path, fl.task_id, ai.agent_id, ai.workspace_path, ai.artifact, fl.lock_type, fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN tasks ai ON ai.task_id = fl.task_id
       ${lockWhere.length > 0 ? `WHERE ${lockWhere.join(" AND ")}` : ""}
       ORDER BY fl.acquired_at DESC LIMIT ?`
  ).all(...lockBinds, limit);
  const openRefinements = db2.prepare(
    `SELECT COUNT(*) AS count FROM refinements
      WHERE state IN ('open','ongoing')
      ${wsPath ? "AND (workspace_path = ? OR workspace_path IS NULL)" : ""}
      ${artifact ? "AND (artifact = ? OR artifact IS NULL)" : ""}`
  ).get(...[...wsPath ? [wsPath] : [], ...artifact ? [artifact] : []]).count;
  return emit({
    db_path: dbPath2,
    fts_enabled: hasFts(db2),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    active_task_count: activeTasks,
    open_refinements: openRefinements,
    locks,
    workspace_path: wsPath,
    artifact
  }, 0, opts2);
}
function cmdInit(db2, dbPath2, opts2) {
  const memCount = db2.prepare("SELECT COUNT(*) AS count FROM memories").get().count;
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
var HELP = `usage: awareness <command> [options]

commands: tell-memory  get-memory  forget  reflect  refine-set  refine-get  refine-delete
          pre-flight-intent  release-file-lock  status  workspace-status  init  self-test
          prune-stale-locks  audit-unverified  verify  mine-weakness  doc-staleness  export-harness  memory-index
          notify  agent-signal  notify-get  notify-resolve  notify-prune  session-capture  wait-for-lock  digest

common options:
  --db <path>     Override DB path (default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3)
  --compact       Compact JSON output (or OCTOCODE_AWARENESS_COMPACT=1)

tell-memory:
  --agent-id <id>  --task-context <text>  --observation <text>
  --importance <1-10>  --label <LABEL>  [--tag <t>]...  [--reference <r>]...

get-memory:
  --query <text>  [--limit <n>]  [--min-importance <n>]  [--label <L>]  [--smart]
  [--reference <r>]...  [--regex <pattern>]...  [--file-regex <pat>]...  [--file <path>]...
  [--sort smart|importance|recent|accessed]  [--state ACTIVE|SUPERSEDED]...
  [--strict-scope]  [--global-only]  [--as-of <ISO>]  [--explain]
  --explain: attach per-result score_components (importance/recency/access/relevance)

forget:
  [--memory-id <id>]...  [--tag <t>]...  [--before <ISO>]  [--max-importance <n>]  [--dry-run]

refine-delete:
  --refinement-id <id>...  [--workspace <path>]  [--dry-run]

export-harness:
  [--limit <n>]  [--min-importance <n>]  [--workspace <path>]
  preview top lessons as an AGENTS.md block

notify:
  --agent-id <id>  --kind claim|handoff|question|reply|blocker|request|decision|fyi
  --subject <text>  [--to <agent-id>]  [--body <text>]  [--file <path>]...
  [--ref-id <id>]...  [--in-reply-to <signal-id>]  [--importance <1-10>]

notify-resolve:
  [--signal-id <id>]...  [--thread-id <id>]

notify-prune:
  [--signal-id <id>]...  [--resolved]  [--older-than-days <n>]  [--dry-run]

reflect:
  --agent-id <id>  --task <text>  --outcome worked|partial|failed
  [--lesson <text>]  [--worked <text>]  [--didnt-work <text>]
  [--fix-repo <text>]  [--fix-file <path>]...  [--fix-harness <text>]
  [--failure-signature <sig>]  [--importance <1-10>]  [--judgment-note <text>]
  [--duo]  [--eval-failure-json '<[{id,dimension?,failure_signature?,suggested_lesson?}]>']
  --duo emits an advisory reflection_duo packet (not stored); eval failures
  become eval-tagged memories clustered by failure_signature

refine-set:
  new:    --agent-id <id> --reasoning <text> --remember <text>
          [--quality good|bad|handoff]  [--state open|ongoing|done]  [--file <path>]...
  update: --refinement-id <id> plus only the flags to change (e.g. --state done)

refine-get:
  [--state open|ongoing|done]...  [--quality good|bad|handoff]  [--include-handoffs]
  session handoffs are hidden unless --include-handoffs or --quality handoff is passed

prune-stale-locks:
  [--older-than-minutes <n>]  [--expired-only]  [--agent-id <id>]
  [--target-file <path>]...  [--dry-run]
  expired locks always qualify; --older-than-minutes also catches old live locks

workspace-status:
  [--workspace <path>] [--artifact <name>]   show active locks, agent tasks, and memory counts

mine-weakness:
  [--agent-id <id>]  [--workspace <path>]  [--min-count <n>]  [--limit <n>]
  find recurring failure patterns grouped by failure_signature

doc-staleness:
  --targets-json '<[{"docFile":"pkg/ARCHITECTURE.md","sourceDirs":["pkg/src"]}]>'
  [--workspace <path>]  [--min-edits <n>]  [--min-lines <n>]
  [--propose]  [--agent-id <id>]  [--session-id <id>]
  compares edit_log activity under sourceDirs against docFile's own last edit_log
  timestamp; --propose records a harness_log 'propose' event (failure_signature
  'doc-staleness') for each stale entry

digest:
  [--retention-days <n>]  [--dry-run]  [--export-doc [path]]
  archive expired memories, prune old superseded rows/refinements, rebuild FTS
  --dry-run: preview counts without mutating anything
  --export-doc: write a markdown memory report to .octocode/memory-reports/

pre-flight-intent:
  --agent-id <id>  [--workspace <path>]  [--target-file <path>]...  [--ttl-minutes <n>]

release-file-lock:
  --agent-id <id>  (--task-id <id> | --target-file <path>)  [--status SUCCESS|PENDING|FAILED]
  [--verified]  [--verified-note <text>]

audit-unverified:
  [--agent-id <id>]  [--workspace <path>]  [--artifact <name>]  [--abandon]
  exits 1 when unverified (PENDING) tasks exist; exits 0 when clear
  --abandon: dismiss all PENDING tasks as FAILED (clear orphaned sessions)

verify:
  (--task-id <id> | --all-pending)  --agent-id <id>
  [--status SUCCESS|FAILED]  [--message <text>]  [--workspace <path>]  [--artifact <name>]
  marks a PENDING task as verified; --all-pending clears every PENDING task for this agent
`;
var rawArgv = process.argv.slice(2);
if (rawArgv.length === 0 || rawArgv.includes("--help") || rawArgv.includes("-h")) {
  process.stdout.write(HELP + "\n");
  process.exit(0);
}
var { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
var [rawCommand, ...rest] = filteredArgv;
var command = rawCommand?.replace(/_/g, "-");
var args = parseArgs(rest ?? []);
if (globalDb) args["db"] = globalDb;
if (command && KNOWN_FLAGS[command]) {
  const unknown = validateFlags(command, args);
  if (unknown.length > 0) {
    process.stdout.write(JSON.stringify({
      ok: false,
      error: `unknown flag(s) for ${command}: ${unknown.map((f) => `--${f.replace(/_/g, "-")}`).join(", ")}`,
      known_flags: KNOWN_FLAGS[command].map((f) => `--${f.replace(/_/g, "-")}`),
      schema_version: 1
    }, null, 2) + "\n");
    process.exit(1);
  }
}
var dbPath = resolveDbPath(globalDb ?? null);
var compact = args["compact"] === true || process.env["OCTOCODE_AWARENESS_COMPACT"] === "1";
var opts = { compact };
if (!command) {
  process.stdout.write("No command given. Run --help for usage.\n");
  process.exit(1);
}
if (command === "self-test") {
  process.exit(cmdSelfTest(opts));
}
var db;
try {
  db = connectDb(dbPath);
} catch (err) {
  process.stderr.write(`awareness: failed to connect DB at ${dbPath}: ${String(err)}
`);
  process.exit(1);
}
var exitCode = 0;
try {
  switch (command) {
    case "tell-memory":
      exitCode = cmdTellMemory(db, args, dbPath, opts);
      break;
    case "get-memory":
      exitCode = cmdGetMemory(db, args, dbPath, opts);
      break;
    case "reflect":
      exitCode = cmdReflect(db, args, dbPath, opts);
      break;
    case "refine-set":
      exitCode = cmdRefineSet(db, args, dbPath, opts);
      break;
    case "refine-get":
      exitCode = cmdRefineGet(db, args, dbPath, opts);
      break;
    case "pre-flight-intent":
      exitCode = cmdPreFlightIntent(db, args, dbPath, opts);
      break;
    case "release-file-lock":
      exitCode = cmdReleaseFileLock(db, args, dbPath, opts);
      break;
    case "status":
      exitCode = cmdStatus(db, dbPath, args, opts);
      break;
    case "init":
      exitCode = cmdInit(db, dbPath, opts);
      break;
    case "prune-stale-locks":
      exitCode = emit({ db_path: dbPath, ...pruneStale(db, args) }, 0, opts);
      break;
    case "audit-unverified":
      exitCode = cmdAuditUnverified(db, args, dbPath, opts);
      break;
    case "verify":
      exitCode = cmdVerify(db, args, dbPath, opts);
      break;
    case "notify-get": {
      const ngFormat = String(args["format"] ?? "json");
      const ngAgentId = args["agent_id"];
      if (ngAgentId && ngFormat !== "hook") {
        exitCode = cmdNotifyGet(db, args, dbPath, opts);
      } else {
        const ngParams = {
          workspace: args["workspace"],
          artifact: args["artifact"],
          format: ngFormat,
          agent_id: ngAgentId
        };
        const ngResult = notifyGet(db, ngParams);
        if (ngFormat === "hook" && ngResult["additionalContext"]) {
          exitCode = emit({ additionalContext: ngResult["additionalContext"] }, 0, opts);
        } else {
          exitCode = emit({ db_path: dbPath, ...ngResult }, 0, opts);
        }
      }
      break;
    }
    case "session-capture":
      exitCode = emit({
        db_path: dbPath,
        ...sessionCapture(db, {
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
      exitCode = emit({ db_path: dbPath, ...mineWeakness(db, mwParams) }, 0, opts);
      break;
    }
    case "doc-staleness":
      exitCode = cmdDocStaleness(db, args, dbPath, opts);
      break;
    case "workspace-status": {
      const wsStatusResult = getWorkspaceStatus(db, {
        workspace_path: args["workspace"],
        artifact: args["artifact"]
      });
      exitCode = emit({ db_path: dbPath, ...wsStatusResult }, 0, opts);
      break;
    }
    case "digest": {
      const retDays = args["retention_days"] ? Number(args["retention_days"]) : void 0;
      const isDryRun = Boolean(args["dry_run"] ?? args["dry-run"]);
      const digestResult = digest(db, {
        ...retDays !== void 0 ? { retention_days: retDays } : {},
        ...isDryRun ? { dry_run: true } : {}
      });
      const payload = { db_path: dbPath, ...digestResult };
      if (!isDryRun && (args["export_doc"] ?? args["export-doc"])) {
        try {
          const wsPath = args["workspace"] ?? process.cwd();
          const artifact = args["artifact"];
          const { mkdirSync: mkdirSync3, writeFileSync: writeFileSync2 } = await import("node:fs");
          const { join: join2 } = await import("node:path");
          const docDir = join2(wsPath, ".octocode", "memory-reports");
          mkdirSync3(docDir, { recursive: true });
          const dateStr = (/* @__PURE__ */ new Date()).toISOString().slice(0, 16).replace("T", "-").replace(":", "");
          const docPath = typeof (args["export_doc"] ?? args["export-doc"]) === "string" ? args["export_doc"] ?? args["export-doc"] : join2(docDir, `memory-report-${dateStr}.md`);
          writeFileSync2(docPath, exportMemoryDoc(db, { workspace_path: wsPath, artifact }), "utf8");
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
      const waitSecs = args["wait_seconds"] ? parseInt(String(args["wait_seconds"]), 10) : null;
      const retrySecs = args["retry_interval"] ? parseInt(String(args["retry_interval"]), 10) : null;
      const waitResult = waitForLock(db, {
        agent_id: args["agent_id"],
        target_files: waitTargets,
        workspace: args["workspace"],
        artifact: args["artifact"],
        lock_type: args["lock_type"],
        wait_ms: waitSecs != null ? waitSecs * 1e3 : void 0,
        retry_interval_ms: retrySecs != null ? retrySecs * 1e3 : void 0
      });
      exitCode = emit({ db_path: dbPath, ...waitResult }, waitResult.lock_free ? 0 : 2, opts);
      break;
    }
    case "memory-index":
      exitCode = cmdMemoryIndex(db, args, dbPath, opts);
      break;
    case "forget":
      exitCode = cmdForget(db, args, dbPath, opts);
      break;
    case "refine-delete":
      exitCode = cmdRefineDelete(db, args, dbPath, opts);
      break;
    case "export-harness":
      exitCode = cmdExportHarness(db, args, dbPath, opts);
      break;
    case "notify":
      exitCode = cmdNotify(db, args, dbPath, opts);
      break;
    case "agent-signal":
      exitCode = cmdAgentSignal(db, args, dbPath, opts);
      break;
    case "notify-resolve":
      exitCode = cmdNotifyResolve(db, args, dbPath, opts);
      break;
    case "notify-prune":
      exitCode = cmdNotifyPrune(db, args, dbPath, opts);
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
//# sourceMappingURL=awareness.js.map
