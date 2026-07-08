#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (w) => {
  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;
  console.error(w?.stack ?? String(w));
});

// bin/awareness.ts
import { writeFileSync as writeFileSync4, mkdirSync as mkdirSync5, existsSync as existsSync4 } from "node:fs";
import { spawnSync as spawnSync7 } from "node:child_process";
import { dirname as dirname6, isAbsolute as isAbsolute6, join as join8, resolve as resolve11 } from "node:path";
import { DatabaseSync as DatabaseSync2 } from "node:sqlite";
import { fileURLToPath as fileURLToPath3 } from "node:url";

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
  const db3 = new DatabaseSync(dbPath2);
  db3.exec("PRAGMA foreign_keys = ON");
  db3.exec("PRAGMA busy_timeout = 5000");
  db3.exec("PRAGMA journal_mode = WAL");
  initDb(db3);
  _db = db3;
  return db3;
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

    CREATE TABLE IF NOT EXISTS tasks (
      task_id        TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
	      rationale      TEXT NOT NULL,
	      test_plan      TEXT NOT NULL,
	      plan_doc_ref   TEXT,
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
`;
function initDb(db3) {
  db3.exec(SCHEMA_DDL);
  migrateExistingTables(db3);
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
  if (stale.c === 0) return { pruned_locks: 0, updated_tasks: 0 };
  db3.exec("SAVEPOINT evict_expired_locks");
  try {
    db3.exec("CREATE TEMP TABLE IF NOT EXISTS temp_expired_lock_tasks(task_id TEXT PRIMARY KEY)");
    db3.exec("DELETE FROM temp_expired_lock_tasks");
    db3.prepare(
      `INSERT OR IGNORE INTO temp_expired_lock_tasks(task_id)
       SELECT task_id FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?`
    ).run(now);
    const deleteRes = db3.prepare(
      "DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?"
    ).run(now);
    const updateRes = db3.prepare(
      `UPDATE tasks
       SET status = 'PENDING', updated_at = ?
       WHERE status = 'ACTIVE'
         AND task_id IN (SELECT task_id FROM temp_expired_lock_tasks)
         AND NOT EXISTS (SELECT 1 FROM locks WHERE locks.task_id = tasks.task_id)`
    ).run(now);
    db3.exec("DELETE FROM temp_expired_lock_tasks");
    db3.exec("RELEASE SAVEPOINT evict_expired_locks");
    return { pruned_locks: deleteRes.changes, updated_tasks: updateRes.changes };
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
  const labels = label ? Array.isArray(label) ? label.map(normalizeLabel) : [normalizeLabel(label)] : [];
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
  return { ok: true, clusters: selected, total_signatures: totals.sigs, total_memories: totals.mems };
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
function loadMemoriesByIds(db3, memoryIds) {
  const ids = [...new Set(memoryIds.filter(Boolean))];
  if (ids.length === 0) return [];
  const placeholders = ids.map(() => "?").join(", ");
  const rows = db3.prepare(
    `SELECT * FROM memories WHERE memory_id IN (${placeholders}) AND state = 'ACTIVE'`
  ).all(...ids);
  const byId = new Map(rows.map((row) => [row.memory_id, rowToMemory(row)]));
  attachMemoryReferences(db3, [...byId.values()]);
  return ids.map((id) => byId.get(id)).filter((row) => Boolean(row));
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

// src/audit.ts
import { randomUUID as randomUUID2 } from "node:crypto";
import { createHash } from "node:crypto";

// src/sql/audit.ts
var EDIT_LOG_INSERT = `
  INSERT INTO edit_log (
    edit_id, session_id, task_id, agent_id,
    file_path, operation, old_file_path,
    lines_added, lines_removed, content_hash,
    workspace_path, artifact, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;
var HARNESS_LOG_INSERT = `
  INSERT INTO harness_log (
    harness_id, session_id, agent_id, workspace_path, artifact, event_type,
    payload_json, memory_id, task_id, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;

// src/audit.ts
function insertEditLog(db3, params) {
  const editId = "edit_" + randomUUID2();
  const now = utcNow();
  db3.prepare(EDIT_LOG_INSERT).run(
    editId,
    params.sessionId ?? null,
    params.taskId ?? null,
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
    params.taskId ?? null,
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
  const entries = params.targets.map((target) => {
    const docLastSyncedAt = lastEditTimestamp(db3, target.docFile, workspacePath, artifact2);
    const activity = sourceActivitySince(db3, target.sourceDirs, docLastSyncedAt, workspacePath, artifact2);
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
  if (!existsSync(root)) {
    return { ok: true, count: 0, docs: [], root };
  }
  const docs = readdirSync(root).filter((name) => name.endsWith(".md")).sort((a, b) => a.localeCompare(b)).map((name) => parseDocFile(join3(root, name)));
  return {
    ok: true,
    count: docs.length,
    docs: docs.map((doc) => ({
      name: doc.name,
      title: doc.title,
      description: doc.description,
      kind: doc.kind,
      path: doc.path
    })),
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
   WHERE state IN ('open','ongoing') AND quality <> 'handoff'
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
  return { count: refinements.length, refinements };
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
import { randomUUID as randomUUID4 } from "node:crypto";
import { isAbsolute, resolve as resolve4 } from "node:path";
var MAX_LOCK_TTL_MS = 10 * 6e4;
var VALID_RELEASE_STATUSES = /* @__PURE__ */ new Set(["PENDING", "SUCCESS", "FAILED"]);
function effectiveTtlMs(ttlMs) {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}
function expiresAtFromNow(ttlMs) {
  return new Date(Date.now() + effectiveTtlMs(ttlMs)).toISOString().replace(/\.\d{3}Z$/, "Z");
}
function workspaceScopeRoot(workspacePath) {
  const candidate = workspacePath ?? process.cwd();
  return normalizeWorkspacePath(candidate, candidate) ?? resolve4(candidate);
}
function workspaceFileBase(workspacePath) {
  return workspacePath ? resolve4(workspacePath) : process.cwd();
}
function resolveTargetFiles(targetFiles = [], workspacePath) {
  const root = workspaceFileBase(workspacePath);
  return targetFiles.map((file) => isAbsolute(file) ? resolve4(file) : resolve4(root, file));
}
function preFlightIntent(db3, params) {
  const {
    agentId: agentId2 = "agent",
    sessionId = null,
    workspacePath,
    artifact: artifact2,
    rationale = "agent write operation",
    testPlan = "post-edit verification",
    planDocRef = null,
    targetFiles = [],
    lockType = "EXCLUSIVE",
    ttlMs = MAX_LOCK_TTL_MS
  } = params;
  const taskId = "task_" + randomUUID4().replace(/-/g, "");
  const now = utcNow();
  const wsPath = workspaceScopeRoot(workspacePath);
  const artifactScope = normalizeArtifact(artifact2);
  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  evictExpiredLocks(db3);
  db3.exec("BEGIN IMMEDIATE");
  try {
    const conflicts = [];
    for (const absPath of absFiles) {
      const conflictMode = lockType === "SHARED" ? "fl.lock_type = 'EXCLUSIVE'" : "1 = 1";
      const existing = db3.prepare(`
        SELECT fl.*, ai.agent_id AS task_agent_id FROM locks fl
        JOIN tasks ai ON ai.task_id = fl.task_id
        WHERE fl.file_path = ?
          AND ai.agent_id <> ?
          AND ai.status = 'ACTIVE'
          AND ${conflictMode}
          AND (fl.expires_at IS NULL OR fl.expires_at > ?)
      `).all(absPath, agentId2, now);
      conflicts.push(...existing);
    }
    if (conflicts.length > 0) {
      db3.exec("ROLLBACK");
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
      db3.prepare(
        `INSERT OR IGNORE INTO sessions (session_id, agent_id, workspace_path, artifact, started_at)
         VALUES (?, ?, ?, ?, ?)`
      ).run(sessionId, agentId2, wsPath, artifactScope, now);
    }
    db3.prepare(`
      INSERT INTO tasks
        (task_id, agent_id, session_id, rationale, test_plan, plan_doc_ref, status, workspace_path, artifact, files_json, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?)
    `).run(taskId, agentId2, sessionId, rationale, testPlan, planDocRef, wsPath, artifactScope, JSON.stringify(absFiles), now, now);
    const expiresAt = expiresAtFromNow(ttlMs);
    const acquiredLocks = [];
    for (const absPath of absFiles) {
      const lockId = "lock_" + randomUUID4().replace(/-/g, "");
      db3.prepare(`
        INSERT OR REPLACE INTO locks
          (lock_id, file_path, task_id, agent_id, session_id, lock_type, acquired_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(lockId, absPath, taskId, agentId2, sessionId, lockType, now, expiresAt);
      acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
    }
    db3.exec("COMMIT");
    return {
      ok: true,
      task: {
        task_id: taskId,
        agent_id: agentId2,
        session_id: sessionId,
        lock_type: lockType,
        workspace_path: wsPath,
        artifact: artifactScope,
        plan_doc_ref: planDocRef,
        target_files: absFiles,
        locks: acquiredLocks.map((l) => ({
          lock_id: l.lock_id,
          file_path: l.file_path,
          lock_type: l.lock_type,
          agent_id: agentId2,
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
      db3.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
}
function releaseFileLock(db3, params) {
  const {
    agentId: agentId2 = "agent",
    sessionId = null,
    workspacePath = null,
    artifact: artifact2 = null,
    taskId = null,
    targetFiles = [],
    status: statusArg = "SUCCESS",
    verified = false,
    verifiedNote
  } = params;
  if (!VALID_RELEASE_STATUSES.has(String(statusArg))) {
    throw new Error(`releaseFileLock status must be PENDING, SUCCESS, or FAILED; got "${statusArg}"`);
  }
  const requestedStatus = String(statusArg);
  const requestedSuccessWithoutVerification = requestedStatus === "SUCCESS" && !verified;
  const effectiveStatus = requestedSuccessWithoutVerification ? "PENDING" : requestedStatus;
  const now = utcNow();
  const whereClauses = ["fl.agent_id = ?"];
  const whereParams = [agentId2];
  if (sessionId) {
    whereClauses.push("fl.session_id = ?");
    whereParams.push(sessionId);
  }
  const artifactScope = normalizeArtifact(artifact2);
  if (workspacePath || artifactScope) {
    whereClauses.push("ai.task_id = fl.task_id");
  }
  if (workspacePath) {
    whereClauses.push("ai.workspace_path = ?");
    whereParams.push(workspaceScopeRoot(workspacePath));
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
  const locks = db3.prepare(
    `SELECT fl.lock_id, fl.task_id, fl.file_path
       FROM locks fl${workspacePath || artifactScope ? ", tasks ai" : ""}
      WHERE ${where}`
  ).all(...whereParams);
  const deleteClauses = ["agent_id = ?"];
  const deleteParams = [agentId2];
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
  db3.exec("BEGIN IMMEDIATE");
  try {
    const lockIds = locks.map((lock) => lock.lock_id);
    if (lockIds.length > 0) {
      db3.prepare(`DELETE FROM locks WHERE lock_id IN (${lockIds.map(() => "?").join(",")})`).run(...lockIds);
    } else if (taskId && !workspacePath && !artifactScope) {
      db3.prepare(`DELETE FROM locks WHERE ${deleteClauses.join(" AND ")}`).run(...deleteParams);
    }
    for (const tid of taskIds) {
      const remaining = db3.prepare("SELECT 1 FROM locks WHERE task_id = ? LIMIT 1").get(tid);
      if (!remaining) {
        db3.prepare(
          "UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND agent_id = ?"
        ).run(effectiveStatus, now, tid, agentId2);
        if (verified && verifiedNote) {
          try {
            db3.prepare(
              `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
               VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
            ).run("evt_" + randomUUID4().replace(/-/g, ""), tid, agentId2, verifiedNote, now);
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
    released: locks.length > 0 || Boolean(taskId),
    locks_released: locks.length,
    task_ids: taskIds,
    updated_at: now,
    ...requestedSuccessWithoutVerification ? { unverifiedConclusion: "SUCCESS requested without --verified; stored as PENDING until verify records the test result." } : {}
  };
}

// src/reflect.ts
import { resolve as resolve5 } from "node:path";
var VALID_OUTCOMES = ["worked", "partial", "failed"];
var NEXT_MSG = "memory_refine_get \u2192 repo fixes for the next agent \xB7 octocode-awareness reflect mine-weakness/maintenance digest \u2192 recurring failures and harness previews. A human merges.";
function normalizeScopePaths(paths = [], prefix, baseCwd) {
  const base = baseCwd ?? process.cwd();
  return [...new Set(paths.filter(Boolean).map((p) => {
    const abs = p.startsWith("/") ? p : resolve5(base, p);
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
import { spawnSync as spawnSync3 } from "node:child_process";
import { randomUUID as randomUUID6 } from "node:crypto";
import { isAbsolute as isAbsolute2, resolve as resolve6 } from "node:path";

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
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd()
  );
  const signalId = "ntf_" + randomUUID5().replace(/-/g, "");
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
  const rows = db3.prepare(sql).all(...allBinds, limit);
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
var SESSION_CAPTURE_FILE_LIMIT = 40;
var SESSION_CAPTURE_VISIBLE_FILE_LIMIT = 20;
var SESSION_CAPTURE_TASK_DETAIL_LIMIT = 8;
var SESSION_CAPTURE_TASK_FILE_LIMIT = 8;
var SESSION_CAPTURE_TEXT_LIMIT = 180;
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
    const base = rawWorkspacePath ? resolve6(rawWorkspacePath) : process.cwd();
    return isAbsolute2(file) ? resolve6(file) : resolve6(base, file);
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
    conditions.push("l.agent_id = ?");
    binds.push(agentId2);
  }
  if (targetFiles.length > 0) {
    conditions.push(`l.file_path IN (${targetFiles.map(() => "?").join(",")})`);
    binds.push(...targetFiles);
  }
  const scopedByTask = Boolean(workspacePath || artifact2);
  if (workspacePath) {
    conditions.push("t.workspace_path = ?");
    binds.push(workspacePath);
  }
  if (artifact2) {
    conditions.push("(t.artifact = ? OR t.artifact IS NULL)");
    binds.push(artifact2);
  }
  const where = conditions.join(" AND ");
  let staleLocks = [];
  try {
    const from = scopedByTask ? "locks l JOIN tasks t ON t.task_id = l.task_id" : "locks l";
    staleLocks = db3.prepare(
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
  db3.exec("BEGIN IMMEDIATE");
  try {
    const ph = staleLocks.map(() => "?").join(",");
    db3.prepare(`DELETE FROM locks WHERE lock_id IN (${ph})`).run(...staleLocks.map((l) => l.lock_id));
    for (const tid of affectedTaskIds) {
      const remaining = db3.prepare("SELECT 1 FROM locks WHERE task_id = ? LIMIT 1").get(tid);
      if (!remaining) {
        const r = db3.prepare(
          "UPDATE tasks SET status = 'PENDING', updated_at = ? WHERE task_id = ? AND status = 'ACTIVE'"
        ).run(now, tid);
        if (r.changes) updatedTasks++;
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
  return { pruned_locks: staleLocks.length, updated_tasks: updatedTasks };
}
function openRefinementCount(db3, params = {}) {
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
    const result = spawnSync3("git", ["-C", workspacePath, "status", "--short"], {
      encoding: "utf8",
      timeout: 5e3
    });
    if (result.status !== 0) return [];
    return String(result.stdout).split("\n").map((line) => line.trim()).filter(Boolean).map((line) => line.slice(3).trim()).filter(Boolean);
  } catch {
    return [];
  }
}
function sessionCapture(db3, params = {}) {
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
  const taskWorkspaceCandidates = [...new Set([workspacePath, rawWorkspacePath].filter((value) => Boolean(value)))];
  const artifact2 = scope.artifact;
  const workspacePlaceholders = taskWorkspaceCandidates.map(() => "?").join(",");
  const taskRows2 = db3.prepare(
    `SELECT task_id, rationale, test_plan, plan_doc_ref, status, files_json, created_at, updated_at
     FROM tasks
     WHERE agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path IN (${workspacePlaceholders}) OR workspace_path IS NULL)
       AND (? IS NULL OR artifact = ? OR artifact IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId2, ...taskWorkspaceCandidates, artifact2, artifact2);
  const files = [...new Set(taskRows2.flatMap((row) => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeTasks = taskRows2.filter((row) => row.status === "ACTIVE").length;
  const pendingTasks = taskRows2.filter((row) => row.status === "PENDING").length;
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
  if (taskRows2.length === 0 && dirtyFiles.length === 0) {
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
  const allCapturedFiles = [.../* @__PURE__ */ new Set([...files, ...dirtyFiles])];
  const capturedFiles = allCapturedFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const capturedDirtyFiles = dirtyFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const statusSummary = taskRows2.slice(0, SESSION_CAPTURE_TASK_DETAIL_LIMIT).map((row) => {
    const rowFiles2 = parseJsonList(row.files_json);
    const shownFiles = rowFiles2.slice(0, SESSION_CAPTURE_TASK_FILE_LIMIT);
    const omittedFiles = rowFiles2.length - shownFiles.length;
    const fileSuffix = rowFiles2.length > 0 ? ` files=${shownFiles.join(", ")}${omittedFiles > 0 ? ` (+${omittedFiles} more)` : ""}` : "";
    const planSuffix = row.plan_doc_ref ? ` plan=${row.plan_doc_ref}` : "";
    return `${row.status} ${row.task_id}: ${compactText(row.rationale)}; verify=${compactText(row.test_plan)}${planSuffix}${fileSuffix}`;
  });
  const omittedTaskDetails = taskRows2.length - statusSummary.length;
  const reasoning = [
    `Session capture for ${agentId2}${reason ? ` (${reason})` : ""}.`,
    `Unresolved tasks: ${taskRows2.length} (${activeTasks} active, ${pendingTasks} pending).`,
    listSummary("Dirty files", dirtyFiles),
    statusSummary.length > 0 ? `Task details: ${statusSummary.join(" | ")}${omittedTaskDetails > 0 ? ` | ${omittedTaskDetails} more tasks omitted` : ""}` : null
  ].filter(Boolean).join(" ");
  const remember = [
    `Review session handoff for ${agentId2}: ${activeTasks} active and ${pendingTasks} pending tasks remain.`,
    listSummary("Touched files", allCapturedFiles),
    dirtyFiles.length > 0 ? "Check dirty git state before continuing." : null,
    pendingTasks > 0 ? "Run the recorded verification before claiming completion." : null
  ].filter(Boolean).join(" ");
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
    pending_tasks: pendingTasks,
    active_tasks: activeTasks,
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
  const waitMs = Number(params.wait_ms ?? params.waitMs ?? 6e4);
  const retryMs = Number(params.retry_interval_ms ?? params.retryIntervalMs ?? 5e3);
  const requestedLockType = String(
    params.requestedLockType ?? params.requested_lock_type ?? params.lockType ?? params.lock_type ?? "EXCLUSIVE"
  ).toUpperCase();
  const start = Date.now();
  if (targetFiles.length === 0) {
    return { ok: true, waited_ms: 0, lock_free: true };
  }
  const root = rawWorkspacePath ? resolve6(rawWorkspacePath) : process.cwd();
  const absTargetFiles = targetFiles.map((file) => isAbsolute2(file) ? resolve6(file) : resolve6(root, file));
  const ph = absTargetFiles.map(() => "?").join(",");
  const lockTypeFilter = requestedLockType === "EXCLUSIVE" ? "" : "AND fl.lock_type = 'EXCLUSIVE'";
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
  const taskScopeParts = [];
  const taskScopeParams = [];
  if (wsPath) {
    taskScopeParts.push("workspace_path = ?");
    taskScopeParams.push(wsPath);
  }
  if (artifact2) {
    taskScopeParts.push("(artifact = ? OR artifact IS NULL)");
    taskScopeParams.push(artifact2);
  }
  const taskScope = taskScopeParts.length > 0 ? ` AND ${taskScopeParts.join(" AND ")}` : "";
  const pendingTasks = db3.prepare(
    `SELECT COUNT(*) AS c FROM tasks WHERE status = 'PENDING'${taskScope}`
  ).get(...taskScopeParams).c;
  const activeTasks = db3.prepare(
    `SELECT COUNT(*) AS c FROM tasks WHERE status = 'ACTIVE'${taskScope}`
  ).get(...taskScopeParams).c;
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
    return { count: 0, harness_count: 0, markdown: "<!-- No harness or high-importance memories to export -->", memories: [] };
  }
  const harnessCount = memories.filter((m) => m.tier === "harness").length;
  const lines = [
    "## Agent lessons (generated by octocode-awareness \xB7 reflect export-harness)",
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
    `SELECT task_id, agent_id, status, test_plan, plan_doc_ref, rationale, workspace_path, artifact, files_json, created_at
     FROM tasks
     WHERE ${where.join(" AND ")}
     ORDER BY created_at ASC`
  ).all(...binds);
  const unverified = rows.map((r) => ({
    task_id: r.task_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    plan_doc_ref: r.plan_doc_ref,
    rationale: r.rationale,
    target_files: parseJsonList(r.files_json),
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at
  }));
  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db3.prepare(TASKS_UPDATE_PENDING_TO_FAILED).run(now, intent.task_id);
      try {
        db3.prepare(TASK_LOG_INSERT_ABANDONED).run(
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
    if (workspacePath) {
      staleWhere.push("ai.workspace_path = ?");
      staleBinds.push(workspacePath);
    }
    if (artifact2) {
      staleWhere.push("(ai.artifact = ? OR ai.artifact IS NULL)");
      staleBinds.push(artifact2);
    }
    const staleRows = db3.prepare(
      `SELECT ai.task_id, ai.agent_id, ai.rationale, ai.plan_doc_ref, ai.workspace_path, ai.artifact, ai.files_json, ai.created_at
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
        plan_doc_ref: r.plan_doc_ref,
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
      db3.prepare(TASKS_UPDATE_ACTIVE_TO_FAILED).run(now, intent.task_id);
      try {
        db3.prepare(TASK_LOG_INSERT_STALE_ABANDONED).run(
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
function markVerified(db3, params) {
  const { agentId: agentId2 = "agent", allPending = false, message } = params;
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact2 = normalizeArtifact(params.artifact);
  const taskId = params.taskId ?? "";
  const status = params.status ?? "SUCCESS";
  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" \u2014 must be SUCCESS or FAILED`,
      task_id: taskId || null
    };
  }
  if (allPending) {
    const dynWhere = [
      workspacePath ? " AND workspace_path = ?" : "",
      artifact2 ? " AND (artifact = ? OR artifact IS NULL)" : ""
    ].join("");
    const selectSql = TASKS_SELECT_PENDING_IDS.replace("{DYNAMIC_WHERE}", dynWhere);
    const selectBinds = [agentId2];
    if (workspacePath) selectBinds.push(workspacePath);
    if (artifact2) selectBinds.push(artifact2);
    const rows = db3.prepare(selectSql).all(...selectBinds);
    const now2 = utcNow();
    const ids = [];
    for (const row of rows) {
      db3.prepare(TASKS_UPDATE_PENDING_VERIFIED).run(status, now2, row.task_id);
      ids.push(row.task_id);
      if (message) {
        try {
          db3.prepare(TASK_LOG_INSERT_VERIFIED).run(
            "evt_" + randomUUID7().replace(/-/g, ""),
            row.task_id,
            agentId2,
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
  const now = utcNow();
  const result = db3.prepare(TASKS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
    status,
    now,
    taskId,
    agentId2
  );
  if (result.changes === 0) {
    const row = db3.prepare(TASKS_SELECT_STATUS).get(taskId);
    if (!row) {
      return { ok: false, error: `no task found with task_id=${taskId}`, task_id: taskId };
    }
    if (row.agent_id !== agentId2) {
      return {
        ok: false,
        error: `task ${taskId} belongs to agent "${row.agent_id}", not "${agentId2}"`,
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
      db3.prepare(TASK_LOG_INSERT_VERIFIED).run(
        "evt_" + randomUUID7().replace(/-/g, ""),
        taskId,
        agentId2,
        message,
        now
      );
    } catch {
    }
  }
  return { ok: true, task_id: taskId, status, updated_at: now };
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
import { existsSync as existsSync2, mkdirSync as mkdirSync2, readFileSync as readFileSync2, writeFileSync } from "node:fs";
import { homedir as homedir2 } from "node:os";
import { dirname as dirname4, isAbsolute as isAbsolute3, join as join4, relative, resolve as resolve7, sep } from "node:path";
var WRITE_MATCHER = "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch";
var HOSTS = /* @__PURE__ */ new Set(["claude", "codex", "cursor"]);
function hooksInstallUsage() {
  return `usage: octocode-awareness hooks install|check|remove [options]

Install, check, dry-run, or remove octocode-awareness lifecycle hooks.

Targets:
  --host claude         Write Claude Code hooks to .claude/settings.json (install default).
  --host codex         Write Codex hooks to .codex/hooks.json.
  --host cursor        Write Cursor hooks to .cursor/hooks.json.

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
  if (!existsSync2(settingsPath)) return {};
  const raw = readFileSync2(settingsPath, "utf8");
  const parsed = JSON.parse(raw);
  return parsed && typeof parsed === "object" ? parsed : {};
}
function hookCommand(name, params) {
  const abs = join4(params.hookDir, name);
  if (params.host === "codex" || params.host === "cursor" || params.globalMode) return abs;
  const rel = relative(params.projectDir, abs);
  if (rel && !rel.startsWith("..") && !isAbsolute3(rel)) {
    return "${CLAUDE_PROJECT_DIR}/" + rel.split(sep).join("/");
  }
  return abs;
}
function specsFor(host, params) {
  const command2 = (name) => hookCommand(name, { host, ...params });
  if (host === "cursor") {
    return [
      { event: "preToolUse", matcher: WRITE_MATCHER, command: command2("pre-edit.sh") },
      { event: "preToolUse", matcher: WRITE_MATCHER, command: command2("harness-guard.sh") },
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
      { event: "PreToolUse", matcher: WRITE_MATCHER, command: command2("harness-guard.sh") },
      { event: "PostToolUse", matcher: WRITE_MATCHER, command: command2("post-edit.sh") },
      { event: "Stop", command: command2("stop-verify.sh") },
      { event: "SubagentStop", command: command2("stop-verify.sh") },
      { event: "PreCompact", command: command2("session-end.sh") },
      { event: "UserPromptSubmit", command: command2("notify-deliver.sh") }
    ];
  }
  return [
    { event: "PreToolUse", matcher: WRITE_MATCHER, command: command2("pre-edit.sh") },
    { event: "PreToolUse", matcher: WRITE_MATCHER, command: command2("harness-guard.sh") },
    { event: "PostToolUse", matcher: WRITE_MATCHER, command: command2("post-edit.sh") },
    { event: "Stop", command: command2("stop-verify.sh") },
    { event: "SubagentStop", command: command2("stop-verify.sh") },
    { event: "SessionEnd", command: command2("session-end.sh") },
    { event: "UserPromptSubmit", command: command2("notify-deliver.sh") }
  ];
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
    return sameAwarenessCommand(group.command, spec.command) && group.timeout === 20 && matcherMatches(group.matcher, spec.matcher) && !Array.isArray(group.hooks);
  }
  return matcherMatches(group.matcher, spec.matcher) && (group.hooks ?? []).some((hook) => hook.type === "command" && sameAwarenessCommand(hook.command, spec.command) && hook.timeout === 20);
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
  const projectDir = resolve7(opt(argv, "--project-dir", cwd));
  const config = targetConfig(host);
  const settingsPath = globalMode ? join4(home, config.dir, config.file) : join4(projectDir, config.dir, config.file);
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
  const status = {
    host,
    settingsPath,
    hooks,
    installed_all: checks.every((check) => check.installed),
    missing: checks.filter((check) => !check.present).map((check) => check.key),
    drifted: checks.filter((check) => check.drifted).map((check) => check.key),
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
    mkdirSync2(dirname4(settingsPath), { recursive: true });
    writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + "\n");
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
import { existsSync as existsSync3, readFileSync as readFileSync3, statSync } from "node:fs";
import { join as join6, resolve as resolve9 } from "node:path";

// src/repo-context.ts
import { spawnSync as spawnSync4 } from "node:child_process";
import { mkdirSync as mkdirSync3, realpathSync as realpathSync2, writeFileSync as writeFileSync2 } from "node:fs";
import { isAbsolute as isAbsolute4, join as join5, relative as relative2, resolve as resolve8 } from "node:path";
var AWARENESS_QUERY_VIEWS = [
  "all",
  "repo-profile",
  "memories",
  "gotchas",
  "lessons",
  "tasks",
  "locks",
  "agents",
  "signals",
  "refinements",
  "files",
  "activity",
  "workboard"
];
var VIEW_SET = new Set(AWARENESS_QUERY_VIEWS);
var CSV_VIEWS = ["memories", "gotchas", "lessons", "agents", "tasks", "locks", "signals", "refinements", "files", "activity", "workboard"];
var PROJECTION_MARKDOWN_BUDGETS = {
  "AGENTS.md": { max_lines: 80, role: "agent start summary" },
  "MEMORY.md": { max_lines: 200, role: "active memory index" },
  "GOTCHAS.md": { max_lines: 200, role: "gotcha index" },
  "LEARN.md": { max_lines: 200, role: "lesson/opportunity index" },
  "BOOKMARKS.md": { max_lines: 200, role: "learnable resource index" }
};
var ATTEND_COMPACT_BUDGET = { max_lines: 120, max_json_bytes: 8 * 1024 };
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
  const cwd = params.cwd ? resolve8(params.cwd) : process.cwd();
  const rawWorkspace = params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd;
  const workspacePath = rawWorkspace ? resolve8(String(rawWorkspace)) : null;
  return {
    workspacePath,
    workspacePaths: workspacePath ? workspaceAliases(workspacePath) : [],
    artifact: params.artifact ? String(params.artifact) : null,
    repo: params.repo ? String(params.repo) : null,
    ref: params.ref ? String(params.ref) : null
  };
}
function workspaceAliases(workspacePath) {
  const aliases = /* @__PURE__ */ new Set([workspacePath]);
  try {
    aliases.add(realpathSync2.native(workspacePath));
  } catch {
    try {
      aliases.add(realpathSync2(workspacePath));
    } catch {
    }
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
  const absolute = isAbsolute4(trimmed) ? resolve8(trimmed) : resolve8(workspacePath ?? process.cwd(), trimmed);
  return [`file:${absolute}`, `%${trimmed}%`];
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
  return withReferences(db3, rows).map((row) => ({
    memory_id: row.memory_id,
    label: row.label,
    importance: row.importance,
    task_context: row.task_context,
    observation: row.observation,
    tags: parseJsonList(row.tags_json),
    references: row.references ?? [],
    failure_signature: row.failure_signature,
    agent_id: row.agent_id,
    workspace_path: row.workspace_path,
    artifact: row.artifact,
    repo: row.repo,
    ref: row.ref,
    created_at: row.created_at,
    updated_at: row.updated_at
  }));
}
function taskRows(db3, params) {
  const scope = scopeFromParams(params);
  const where = [];
  const binds = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ["rationale", "test_plan", "plan_doc_ref", "files_json", "agent_id"]);
  addStateFilter(where, binds, stringList(params.state), "status", (state) => state.toUpperCase());
  const since = params.since?.trim();
  if (since) {
    where.push("created_at >= ?");
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT task_id, agent_id, session_id, rationale, test_plan, plan_doc_ref, status,
            workspace_path, artifact, files_json, created_at, updated_at
       FROM tasks
       ${sqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    task_id: String(row["task_id"]),
    agent_id: String(row["agent_id"]),
    status: String(row["status"]),
    rationale: String(row["rationale"]),
    test_plan: String(row["test_plan"]),
    plan_doc_ref: row["plan_doc_ref"] ?? null,
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
  addTextFilter(where, binds, params.query, ["l.file_path", "l.agent_id", "t.rationale"]);
  const agentId2 = params.agentId ?? params.agent_id;
  if (agentId2) {
    where.push("l.agent_id = ?");
    binds.push(agentId2);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const rows = db3.prepare(
    `SELECT l.lock_id, l.file_path, l.task_id, l.agent_id, l.session_id, l.lock_type,
            l.acquired_at, l.expires_at, t.workspace_path, t.artifact, t.status
       FROM locks l
       JOIN tasks t ON t.task_id = l.task_id
       ${sqlWhere}
      ORDER BY datetime(l.acquired_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit));
  return rows.map((row) => ({
    lock_id: String(row["lock_id"]),
    file_path: String(row["file_path"]),
    task_id: String(row["task_id"]),
    agent_id: String(row["agent_id"]),
    lock_type: String(row["lock_type"]),
    task_status: String(row["status"]),
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
function trackFile(map, filePath, source, date) {
  const clean = filePath.startsWith("file:") ? filePath.slice("file:".length) : filePath;
  if (!clean) return;
  const row = map.get(clean) ?? {
    file_path: clean,
    memories: 0,
    gotchas: 0,
    tasks: 0,
    locks: 0,
    refinements: 0,
    signals: 0,
    edits: 0,
    last_seen_at: null
  };
  const current = Number(row[source] ?? 0);
  row[source] = current + 1;
  if (date && (!row["last_seen_at"] || String(date) > String(row["last_seen_at"]))) row["last_seen_at"] = date;
  map.set(clean, row);
}
function fileRows(db3, params) {
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
    trackFile(files, ref.reference, "memories", ref.created_at);
    if (ref.label === "GOTCHA") trackFile(files, ref.reference, "gotchas", ref.created_at);
  }
  for (const row of taskRows(db3, { ...params, limit: 500 })) {
    for (const file of row["files"]) trackFile(files, file, "tasks", String(row["created_at"]));
  }
  for (const row of lockRows(db3, { ...params, limit: 500 })) {
    trackFile(files, String(row["file_path"]), "locks", String(row["acquired_at"]));
  }
  for (const row of refinementRows(db3, { ...params, limit: 500 })) {
    for (const file of row["files"]) trackFile(files, file, "refinements", String(row["updated_at"]));
  }
  for (const row of signalRows(db3, { ...params, limit: 500 })) {
    for (const file of row["files"]) trackFile(files, file, "signals", String(row["created_at"]));
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
    trackFile(files, edit.file_path, "edits", edit.created_at);
    if (edit.old_file_path) trackFile(files, edit.old_file_path, "edits", edit.created_at);
  }
  return [...files.values()].sort((a, b) => {
    const scoreA = Number(a["locks"] ?? 0) * 10 + Number(a["gotchas"] ?? 0) * 6 + Number(a["memories"] ?? 0) * 4 + Number(a["tasks"] ?? 0) * 3 + Number(a["edits"] ?? 0);
    const scoreB = Number(b["locks"] ?? 0) * 10 + Number(b["gotchas"] ?? 0) * 6 + Number(b["memories"] ?? 0) * 4 + Number(b["tasks"] ?? 0) * 3 + Number(b["edits"] ?? 0);
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
  addExactScope(taskWhere, taskBinds, scope);
  const lockWhere = [];
  const lockBinds = [];
  addExactScope(lockWhere, lockBinds, scope, "t");
  const refinementWhere = ["state IN ('open','ongoing')"];
  const refinementBinds = [];
  addExactScope(refinementWhere, refinementBinds, scope);
  const signalWhere = ["status = 'open'"];
  const signalBinds = [];
  addExactScope(signalWhere, signalBinds, scope);
  return [
    { metric: "active_memories", count: countWhere(db3, "memories", memWhere, memBinds) },
    { metric: "gotchas", count: memoryRows(db3, { ...params, view: "gotchas", limit: 500 }, { gotchas: true }).length },
    { metric: "lessons", count: memoryRows(db3, { ...params, view: "lessons", limit: 500 }, { lessons: true }).length },
    { metric: "tasks", count: countWhere(db3, "tasks", taskWhere, taskBinds) },
    { metric: "active_locks", count: countWhere(db3, "locks l JOIN tasks t ON t.task_id = l.task_id", lockWhere, lockBinds) },
    { metric: "open_refinements", count: countWhere(db3, "refinements", refinementWhere, refinementBinds) },
    { metric: "open_signals", count: countWhere(db3, "signals", signalWhere, signalBinds) },
    { metric: "known_agents", count: agentRows(db3, { ...params, limit: 500 }).length },
    { metric: "tracked_files", count: fileRows(db3, { ...params, limit: 500 }).length }
  ];
}
function activityRows(db3, params) {
  const limit = limitOf(params.limit);
  const rows = [];
  for (const row of memoryRows(db3, { ...params, limit })) {
    rows.push({
      kind: "memory",
      id: String(row["memory_id"]),
      title: `${row["label"]}: ${summarize(String(row["task_context"]), 80)}`,
      detail: summarize(String(row["observation"]), 180),
      agent_id: String(row["agent_id"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of taskRows(db3, { ...params, limit })) {
    rows.push({
      kind: "task",
      id: String(row["task_id"]),
      title: `${row["status"]}: ${summarize(String(row["rationale"]), 100)}`,
      detail: summarize(String(row["test_plan"]), 180),
      agent_id: String(row["agent_id"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of signalRows(db3, { ...params, limit })) {
    rows.push({
      kind: "signal",
      id: String(row["signal_id"]),
      title: `${row["kind"]}: ${summarize(String(row["subject"]), 100)}`,
      detail: summarize(String(row["body"] ?? ""), 180),
      agent_id: String(row["from_agent"]),
      created_at: String(row["created_at"])
    });
  }
  for (const row of refinementRows(db3, { ...params, limit })) {
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
  const raw = row["files"];
  return Array.isArray(raw) ? raw.map(String) : [];
}
function groupKey(parts) {
  return parts.map((part) => String(part ?? "").trim().toLowerCase().replace(/\s+/g, " ")).join("|");
}
function pushLimited(columns, counts, column, row, limit) {
  counts[column] = (counts[column] ?? 0) + 1;
  const rows = columns[column] ?? [];
  if (rows.length < limit) {
    rows.push({ column, ...row });
    columns[column] = rows;
  }
}
function compactIds(rows, key) {
  return rows.map((row) => String(row[key] ?? "")).filter(Boolean);
}
function representativeDate(rows) {
  return rows.map((row) => String(row["updated_at"] ?? row["created_at"] ?? row["acquired_at"] ?? "")).filter(Boolean).sort().at(-1) ?? null;
}
function workboardRows(db3, params) {
  const limit = limitOf(params.limit, 10, 50);
  const columns = {
    Inbox: [],
    Verify: [],
    Ready: [],
    Claimed: [],
    RecentDone: [],
    MemoryReview: [],
    ProjectionHealth: []
  };
  const counts = {};
  const openSignals = signalRows(db3, { ...params, state: ["open"], limit: 200, includeBodies: false });
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
  const handoffs = refinementRows(db3, { ...params, state: ["open", "ongoing"], limit: 200 }).filter((row) => String(row["quality"]) === "handoff");
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
  const pendingTasks = taskRows(db3, { ...params, state: ["PENDING"], limit: 500 });
  const taskGroups = /* @__PURE__ */ new Map();
  for (const row of pendingTasks) {
    const key = groupKey([
      String(row["status"]),
      String(row["rationale"]),
      String(row["test_plan"]),
      rowFiles(row).sort().join(","),
      String(row["agent_id"])
    ]);
    const list = taskGroups.get(key) ?? [];
    list.push(row);
    taskGroups.set(key, list);
  }
  for (const group of [...taskGroups.values()].sort((a, b) => String(representativeDate(b) ?? "").localeCompare(String(representativeDate(a) ?? "")))) {
    const row = group[0];
    pushLimited(columns, counts, "Verify", {
      item_type: "task",
      id: String(row["task_id"]),
      title: summarize(String(row["rationale"]), 120),
      detail: summarize(String(row["test_plan"]), 180),
      agent_id: String(row["agent_id"]),
      status: String(row["status"]),
      count: group.length,
      raw_ids: compactIds(group, "task_id"),
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: representativeDate(group)
    }, limit);
  }
  for (const row of refinementRows(db3, { ...params, state: ["open", "ongoing"], limit: 200 }).filter((row2) => String(row2["quality"]) !== "handoff")) {
    pushLimited(columns, counts, "Ready", {
      item_type: "refinement",
      id: String(row["refinement_id"]),
      title: summarize(String(row["remember"]), 120),
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
  for (const row of lockRows(db3, { ...params, limit: 200 })) {
    pushLimited(columns, counts, "Claimed", {
      item_type: "lock",
      id: String(row["lock_id"]),
      title: String(row["file_path"]),
      detail: `task=${row["task_id"]} ${row["lock_type"]}`,
      agent_id: String(row["agent_id"]),
      status: String(row["task_status"]),
      raw_ids: [String(row["lock_id"]), String(row["task_id"])],
      files: [String(row["file_path"])],
      created_at: String(row["acquired_at"]),
      expires_at: row["expires_at"] ?? null
    }, limit);
  }
  for (const row of taskRows(db3, { ...params, state: ["SUCCESS", "FAILED"], limit: 200 })) {
    pushLimited(columns, counts, "RecentDone", {
      item_type: "task",
      id: String(row["task_id"]),
      title: `${row["status"]}: ${summarize(String(row["rationale"]), 100)}`,
      detail: summarize(String(row["test_plan"]), 180),
      agent_id: String(row["agent_id"]),
      status: String(row["status"]),
      raw_ids: [String(row["task_id"])],
      files: rowFiles(row),
      created_at: String(row["created_at"]),
      updated_at: String(row["updated_at"])
    }, limit);
  }
  for (const row of memoryRows(db3, { ...params, limit: 200 })) {
    const failureSignature = String(row["failure_signature"] ?? "");
    const refs = Array.isArray(row["references"]) ? row["references"] : [];
    const tags = Array.isArray(row["tags"]) ? row["tags"] : [];
    const reviewReasons = [
      refs.length === 0 ? "missing_refs" : null,
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
      raw_ids: [String(row["memory_id"])],
      files: refs.filter((ref) => ref.startsWith("file:")).map((ref) => ref.slice("file:".length)),
      created_at: String(row["created_at"]),
      updated_at: row["updated_at"] ?? null
    }, limit);
  }
  const profile = Object.fromEntries(repoProfileRows(db3, params).map((row) => [String(row["metric"]), Number(row["count"] ?? 0)]));
  const activeMemories = Number(profile["active_memories"] ?? 0);
  const taskCount = Number(profile["tasks"] ?? 0);
  const openRefinements = Number(profile["open_refinements"] ?? 0);
  const openSignalCount = Number(profile["open_signals"] ?? 0);
  const projectionWarnings2 = [
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
    case "tasks":
      return taskRows(db3, params);
    case "locks":
      return lockRows(db3, params);
    case "agents":
      return agentRows(db3, params);
    case "signals":
      return signalRows(db3, params);
    case "refinements":
      return refinementRows(db3, params);
    case "files":
      return fileRows(db3, params);
    case "activity":
      return activityRows(db3, params);
    case "workboard":
      return workboardRows(db3, params);
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
    main { padding: 20px 28px 40px; display: grid; gap: 28px; }
    h1 { margin: 0 0 8px; font-size: 24px; letter-spacing: 0; }
    h2 { margin: 0 0 12px; font-size: 18px; letter-spacing: 0; }
    .meta { color: color-mix(in srgb, CanvasText 62%, transparent); font-size: 13px; }
    table { width: 100%; border-collapse: collapse; font-size: 13px; }
    th, td { text-align: left; vertical-align: top; padding: 8px 10px; border-bottom: 1px solid color-mix(in srgb, CanvasText 12%, transparent); }
    th { font-weight: 650; white-space: nowrap; }
    td { max-width: 460px; overflow-wrap: anywhere; }
    section { overflow-x: auto; }
    code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
  </style>
</head>
<body>
  <header>
    <h1>${escapeHtml(title)}</h1>
    <div class="meta">Generated ${escapeHtml(result.generated_at)} for <code>${escapeHtml(result.workspace_path ?? "global")}</code></div>
  </header>
  <main>
    ${sections}
  </main>
</body>
</html>
`;
}
function writeAwarenessView(db3, params = {}) {
  const result = queryAwareness(db3, params);
  const workspacePath = scopeFromParams(params).workspacePath ?? process.cwd();
  const outPath = resolveWorkspaceOutputPath(params.out, workspacePath, join5(workspacePath, ".octocode", "awareness", "index.html"));
  mkdirSync3(join5(outPath, ".."), { recursive: true });
  writeFileSync2(outPath, renderAwarenessHtml(result), "utf8");
  return { ok: true, path: outPath, view: result.view, count: result.count };
}
function resolveWorkspaceOutputPath(output, workspacePath, defaultPath) {
  const target = output?.trim() || defaultPath;
  return isAbsolute4(target) ? resolve8(target) : resolve8(workspacePath, target);
}
function injectRepoContext(db3, params = {}) {
  const scope = scopeFromParams(params);
  const workspacePath = scope.workspacePath ?? process.cwd();
  const rawOutDir = params.outDir ?? params.out_dir;
  const outDir = resolveWorkspaceOutputPath(rawOutDir, workspacePath, join5(workspacePath, ".octocode"));
  const mode = normalizeMode(params.mode);
  const includeView = params.includeView ?? params.include_view ?? true;
  const check = params.check ?? true;
  const generatedAt = utcNow2();
  const queryParams = { ...params, workspacePath, limit: limitOf(params.limit, 50, 500) };
  const all = queryAwareness(db3, { ...queryParams, view: "all" });
  const filesWritten = [];
  const writtenContent = {};
  const warnings = [];
  function write(relPath, content) {
    const full = join5(outDir, relPath);
    mkdirSync3(join5(full, ".."), { recursive: true });
    writeFileSync2(full, content, "utf8");
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
  for (const view of CSV_VIEWS) {
    write(join5("awareness", "csv", `${view}.csv`), toCsv(sections[view]?.rows ?? []));
  }
  if (includeView) {
    write(join5("awareness", "index.html"), renderAwarenessHtml(all));
  }
  write(join5("references", "repo-map.md"), renderReferenceDoc("Repo Map", [
    "Generated overview of awareness-tracked files and activity.",
    "Use `.octocode/awareness/csv/files.csv` when filtering or sorting by file path.",
    "Use the live command `octocode-awareness query files --workspace <repo>` when freshness matters."
  ], sections["files"]?.rows ?? []));
  write(join5("references", "commands.md"), renderReferenceDoc("Awareness Commands", [
    "`octocode-awareness query <view>` reads the SQLite store for agents and scripts.",
    "`octocode-awareness query all --format html --out .octocode/awareness/index.html` writes a static human browser view; use `npx @octocodeai/octocode-awareness` only when no local CLI exists.",
    "`octocode-awareness repo inject --out .octocode` regenerates these Markdown, CSV, and HTML projections."
  ]));
  write(join5("references", "testing.md"), renderReferenceDoc("Testing And Verification", [
    "Treat generated memories as leads. Verify current files and command output before acting.",
    "Release locks with `verify mark` or `lock release --verified` after declared tests actually run.",
    "Record new durable failures with `reflect record --failure-signature` or `memory record --label GOTCHA`."
  ]));
  write(join5("references", "architecture.md"), renderReferenceDoc("Architecture Notes", [
    "The SQLite awareness DB is canonical. Files under `.octocode/` are generated projections.",
    "Keep workspace AGENTS.md concise and point agents here for repo-specific memory indexes.",
    "Do not edit generated CSV/Markdown snapshots by hand; regenerate after important memory changes."
  ]));
  if (check) {
    const ignored = gitCheckIgnored(workspacePath, outDir);
    if (ignored.ignored) {
      warnings.push(`generated path is gitignored: ${relative2(workspacePath, outDir) || outDir}; remove the ignore intentionally if this repo should share .octocode`);
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
    files: filesWritten.map((file) => relative2(workspacePath, file)),
    warnings
  };
  write(join5("awareness", "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
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
  const locks = (sections["locks"]?.rows ?? []).slice(0, 3);
  const lockTotal = (sections["locks"]?.rows ?? []).length;
  const projectionWarnings2 = [
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
    "- Live: `octocode-awareness attend|query|memory recall|workspace status --workspace <repo>`.",
    "- Wiki leads below are projections, not proof. After inject, append a root `AGENTS.md` \u2192 `.octocode/AGENTS.md` pointer if missing.",
    "",
    "## Snapshot",
    "",
    `- Memories ${counts["active_memories"] ?? 0} \xB7 Gotchas ${counts["gotchas"] ?? 0} \xB7 Lessons ${counts["lessons"] ?? 0} \xB7 Locks ${counts["active_locks"] ?? 0} \xB7 Refinements ${counts["open_refinements"] ?? 0} \xB7 Signals ${counts["open_signals"] ?? 0}`,
    "",
    "## Wiki And Memory Map",
    "",
    "- Gotchas \u2192 `.octocode/GOTCHAS.md` \xB7 live `query gotchas` / `memory recall`",
    "- Lessons \u2192 `.octocode/LEARN.md` \xB7 live `query lessons`",
    "- Memory index \u2192 `.octocode/MEMORY.md` \xB7 live `memory recall --smart`",
    "- Bookmarks \u2192 `.octocode/BOOKMARKS.md` \xB7 Files \u2192 `awareness/csv/files.csv` \xB7 Workboard \u2192 live `query workboard`",
    "",
    "## Read Before Editing",
    "",
    "- Read GOTCHAS + LEARN; filter `awareness/csv/files.csv` for affected paths.",
    "- Prefer live `attend` / `query` when freshness matters; `repo inject` after important memories.",
    "",
    "## Projection Health",
    "",
    "- Canonical DB: `~/.octocode/memory/awareness.sqlite3`. Manifest: `.octocode/awareness/manifest.json`.",
    ...projectionWarnings2.map((warning) => `- ${warning}`),
    ""
  ];
  if (locks.length > 0) {
    lines.push("## Active Locks", "");
    for (const lock of locks) lines.push(`- ${lock["file_path"]} - ${lock["agent_id"]} (${lock["lock_type"]})`);
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
  lines.push("- `.octocode/MEMORY.md` \xB7 `.octocode/GOTCHAS.md` \xB7 `.octocode/LEARN.md` \xB7 `.octocode/BOOKMARKS.md`");
  lines.push("- `.octocode/awareness/manifest.json` \xB7 `.octocode/references/`");
  lines.push("");
  return lines.join("\n");
}
function renderRowsDoc(title, rows, description, maxLines) {
  const ranked = [...rows].sort((a, b) => {
    const imp = Number(b["importance"] ?? 0) - Number(a["importance"] ?? 0);
    if (imp !== 0) return imp;
    return String(a["memory_id"] ?? a["task_id"] ?? "").localeCompare(String(b["memory_id"] ?? b["task_id"] ?? ""));
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
    const id = String(row["memory_id"] ?? row["refinement_id"] ?? row["task_id"] ?? row["signal_id"] ?? row["file_path"] ?? "item");
    const label = row["label"] ? `[${row["label"]}:${row["importance"] ?? ""}] ` : "";
    const titleText = row["task_context"] ?? row["subject"] ?? row["remember"] ?? row["rationale"] ?? row["file_path"] ?? id;
    const block = [`## ${label}${summarize(String(titleText), 100)}`];
    if (row["observation"]) block.push("", summarize(String(row["observation"]), 500));
    if (row["failure_signature"]) block.push("", `Failure signature: \`${row["failure_signature"]}\``);
    const refs = Array.isArray(row["references"]) ? row["references"] : [];
    if (refs.length > 0) block.push("", `Refs: ${refs.join(", ")}`);
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
    const sourceId = String(row["memory_id"] ?? "memory");
    const label = `${row["label"] ?? "MEMORY"}:${row["importance"] ?? ""}`.replace(/:$/, "");
    const title = summarize(String(row["task_context"] ?? row["observation"] ?? sourceId), 90);
    for (const rawRef of refs) {
      const ref = rawRef.trim();
      if (!ref) continue;
      const entry2 = byRef.get(ref) ?? { kind: bookmarkKind(ref), sourceIds: [], labels: [], titles: [] };
      if (!entry2.sourceIds.includes(sourceId)) entry2.sourceIds.push(sourceId);
      if (!entry2.labels.includes(label)) entry2.labels.push(label);
      if (!entry2.titles.includes(title)) entry2.titles.push(title);
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
    lines.push(`- \`${ref}\` - ${labelText}; source: ${sourceText}; ${titleText}`);
  }
  lines.push("");
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
  return `<section>
  <h2>${escapeHtml(name)} (${rows.length})</h2>
  ${rows.length === 0 ? '<p class="meta">No rows.</p>' : `<table>${renderHtmlTable(rows)}</table>`}
</section>`;
}
function renderHtmlTable(rows) {
  const keys = keysForRows(rows).slice(0, 12);
  const header = `<thead><tr>${keys.map((key) => `<th>${escapeHtml(key)}</th>`).join("")}</tr></thead>`;
  const body = rows.map((row) => `<tr>${keys.map((key) => `<td>${escapeHtml(cellToString(row[key]))}</td>`).join("")}</tr>`).join("\n");
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
  const sep2 = widths.map((w) => "-".repeat(w)).join("  ");
  const body = rows.map((row) => keys.map((key, i) => truncate(cellToString(row[key]), widths[i] ?? 40).padEnd(widths[i] ?? 40)).join("  "));
  return [line, sep2, ...body].join("\n") + "\n";
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
    const id = row["memory_id"] ?? row["task_id"] ?? row["signal_id"] ?? row["refinement_id"] ?? row["file_path"] ?? row["metric"] ?? "row";
    const label = row["label"] ? `[${cellToString(row["label"])}:${cellToString(row["importance"])}] ` : "";
    const title = row["task_context"] ?? row["subject"] ?? row["remember"] ?? row["rationale"] ?? row["metric"] ?? "";
    const text = row["observation"] ?? row["count"] ?? "";
    const extras = [];
    if (row["failure_signature"]) extras.push(`failure=${cellToString(row["failure_signature"])}`);
    if (Array.isArray(row["references"]) && row["references"].length > 0) extras.push(`refs=${row["references"].join(", ")}`);
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
  const candidate = isAbsolute4(path2) ? relative2(cwd, path2) : path2;
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
    commands: ["signal publish", "refinement set", "lock acquire", "verify audit"],
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
  const next = {};
  for (const key of ["column", "item_type", "id", "status", "agent_id", "quality", "count", "column_total", "omitted_count", "active_memories", "tasks", "open_refinements", "open_signals"]) {
    const value = row[key];
    if (value != null) next[key] = value;
  }
  if (typeof row["title"] === "string") next["title"] = summarize2(row["title"], 90);
  if (Array.isArray(row["reasons"])) next["reasons"] = row["reasons"].slice(0, 3);
  if (Array.isArray(row["files"])) {
    const files = row["files"];
    next["file_count"] = files.length;
    next["files"] = files.slice(0, 2);
    next["omitted_file_count"] = Math.max(0, files.length - 2);
  }
  if (Array.isArray(row["raw_ids"])) {
    const rawIds = row["raw_ids"];
    next["raw_id_count"] = rawIds.length;
    next["raw_ids"] = rawIds.slice(0, 5);
    next["omitted_raw_id_count"] = Math.max(0, rawIds.length - 5);
  }
  return next;
}
function compactWorkboard(grouped, limit) {
  return Object.fromEntries(Object.entries(grouped).map(([column, rows]) => [
    column,
    rows.slice(0, limit).map(compactRow)
  ]));
}
function compactVerificationTarget(row) {
  const compact2 = compactRow(row);
  return {
    id: compact2["id"] ?? null,
    status: compact2["status"] ?? null,
    title: compact2["title"] ?? null,
    count: compact2["count"] ?? null,
    raw_id_count: compact2["raw_id_count"] ?? null,
    raw_ids: compact2["raw_ids"] ?? [],
    column_total: compact2["column_total"] ?? null,
    omitted_count: compact2["omitted_count"] ?? null
  };
}
function uniqueStrings(values) {
  return [...new Set(values.filter(Boolean))];
}
function lineCount2(path2) {
  if (!existsSync3(path2)) return null;
  try {
    return readFileSync3(path2, "utf8").split(/\r?\n/).length;
  } catch {
    return null;
  }
}
function projectionStats(workspacePath) {
  return ["AGENTS.md", "MEMORY.md", "GOTCHAS.md", "LEARN.md", "BOOKMARKS.md", join6("awareness", "manifest.json")].map((file) => {
    const path2 = join6(workspacePath, ".octocode", file);
    let mtimeMs = null;
    try {
      mtimeMs = existsSync3(path2) ? statSync(path2).mtimeMs : null;
    } catch {
    }
    return { file: `.octocode/${file.replace(/\\/g, "/")}`, lines: lineCount2(path2), mtime_ms: mtimeMs };
  });
}
function manifestWarnings(workspacePath, stats) {
  const manifestPath = join6(workspacePath, ".octocode", "awareness", "manifest.json");
  if (!existsSync3(manifestPath)) return [".octocode/awareness/manifest.json missing; run repo inject when projection context is needed"];
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
      join6(workspacePath, ".octocode", "rfc", "homeostatic-awareness-loop", "RFC.md"),
      "RFC goals and decision for the awareness loop"
    );
    add(
      join6(workspacePath, ".octocode", "rfc", "homeostatic-awareness-loop", "IMPLEMENTATION.md"),
      "dependency-ordered build plan for workboard, attend, drive_state, and digest"
    );
    add(
      join6(workspacePath, "packages", "octocode-awareness", "skills", "octocode-awareness", "references", "homeostatic-loop.md"),
      "compact agent-facing organ and drive map"
    );
  }
  if (/(role.?dialogue|self.?reflection|tutor|student|builder|tester|alter.?ego|debate|duo)/.test(haystack)) {
    add(
      join6(workspacePath, "packages", "octocode-awareness", "skills", "octocode-awareness", "references", "self-reflection-dialogue.md"),
      "role-dialogue pattern for hard ideas without persona bloat"
    );
  }
  if (leads.length === 0) {
    add(join6(workspacePath, ".octocode", "AGENTS.md"), "generated repo context entrypoint, if present");
    add(join6(workspacePath, "AGENTS.md"), "workspace-level agent instructions");
  }
  return leads.slice(0, 4);
}
function chooseMode(query, evidenceCount, verifyCount, gapCount) {
  if (verifyCount > 0 && gapCount === 0) return "exploit";
  if (evidenceCount === 0 || /(design|rfc|brainstorm|research|unknown|approach|why|how)/i.test(query)) return verifyCount > 0 ? "mixed" : "explore";
  return gapCount > 0 ? "mixed" : "exploit";
}
function attendAwareness(db3, params = {}) {
  const cwd = params.cwd ? resolve9(params.cwd) : process.cwd();
  const workspacePath = resolve9(String(params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd));
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
  const verificationTargets = (rawWorkboard["Verify"] ?? []).slice(0, packetLimit).map((row) => compact2 ? compactVerificationTarget(row) : row);
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
    const references = compact2 ? allReferences.slice(0, 3) : allReferences;
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
      title: summarize2(memory.task_context, compact2 ? 90 : 120),
      summary: summarize2(memory.observation, compact2 ? 160 : 240),
      references,
      reference_count: allReferences.length,
      omitted_reference_count: Math.max(0, allReferences.length - references.length),
      why_selected: why,
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
      open_signals: profile["open_signals"] ?? 0
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
  const result = {
    ok: true,
    schema_version: 1,
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
    next: verificationTargets.length > 0 ? 'octocode-awareness verify audit --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact; then verify mark --all-pending after the declared test plan' : bloatWarnings.length > 0 ? 'octocode-awareness memory forget --workspace "$PWD" --dry-run --compact; then repo inject --workspace "$PWD" --compact to regenerate capped projections (digest does not shrink markdown)' : evidence.length > 0 ? "Treat evidence as leads; re-check cited files, then lock acquire before edits" : 'octocode-awareness attend --workspace "$PWD" --query "<narrower task>" --compact; or query workboard / workspace status'
  };
  if (explainOrgan) result.organ_reference = ORGAN_REFERENCE;
  return result;
}

// bin/hook-runner.ts
import { spawnSync as spawnSync6 } from "node:child_process";
import { createHash as createHash2 } from "node:crypto";
import { mkdirSync as mkdirSync4, readFileSync as readFileSync4, writeFileSync as writeFileSync3 } from "node:fs";
import { basename as basename3, dirname as dirname5, isAbsolute as isAbsolute5, join as join7, relative as relative3, resolve as resolve10 } from "node:path";
import { fileURLToPath as fileURLToPath2 } from "node:url";

// src/pi-hooks.ts
import path from "node:path";
import { spawnSync as spawnSync5 } from "node:child_process";
import { randomUUID as randomUUID8 } from "node:crypto";
import { realpathSync as realpathSync3 } from "node:fs";
var _sessionStartupToken = randomUUID8().slice(0, 8);
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
function extractPiWriteTargetPaths(toolName, input = {}, options = {}) {
  const normalizedToolName = String(toolName ?? "").toLowerCase();
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
  const command2 = typeof input === "string" ? input : firstString(payload.command, payload.patch, payload.text, payload.content);
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

// bin/hook-runner.ts
function readStdin() {
  return new Promise((resolve12) => {
    let raw = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      raw += chunk;
    });
    process.stdin.on("end", () => resolve12(raw));
    process.stdin.on("error", () => resolve12(raw));
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
    payload.session_id,
    payload.sessionId,
    payload.agent_id,
    payload.agentId,
    input.session_id,
    input.sessionId,
    input.agent_id,
    input.agentId
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
  const suffix = createHash2("sha1").update(scope).digest("hex").slice(0, 12);
  const fallback = `hook:${host.replace(/[^a-zA-Z0-9_.:-]/g, "_")}:${suffix}`;
  if (!warnedFallbackAgentId) {
    warnedFallbackAgentId = true;
    console.error(`octocode-awareness: OCTOCODE_AGENT_ID or host session id missing; using fallback agent id "${fallback}". Set OCTOCODE_AGENT_ID for reliable multi-agent lock isolation.`);
  }
  return fallback;
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
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? "";
  return extractPiWriteTargetPaths(toolName, input, { assumeWrite: true });
}
function resolveHookPath(file, cwd = process.cwd()) {
  return resolve10(cwd, file);
}
function isInsidePath(candidate, root) {
  const resolvedRoot = canonicalizePath(root);
  const resolvedCandidate = canonicalizePath(candidate);
  if (resolvedCandidate === resolvedRoot) return true;
  const rel = relative3(resolvedRoot, resolvedCandidate);
  return rel !== "" && !rel.startsWith("..") && !isAbsolute5(rel);
}
function db() {
  return connectDb(resolveDbPath(null));
}
function hookTaskStateFile() {
  const stateDir = join7(dirname5(resolveDbPath(null)), "hook-state");
  mkdirSync4(stateDir, { recursive: true });
  return join7(stateDir, "shell-hook-tasks.json");
}
function readHookTaskState() {
  try {
    return JSON.parse(readFileSync4(hookTaskStateFile(), "utf8"));
  } catch {
    return {};
  }
}
function writeHookTaskState(state) {
  writeFileSync3(hookTaskStateFile(), JSON.stringify(state, null, 2) + "\n", "utf8");
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
function hookTaskKey(payload, files, cwd) {
  const explicitId = hookEventId(payload);
  const identity = {
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve10(cwd),
    artifact: artifact(payload),
    event: explicitId,
    files: explicitId ? [] : files.map((file) => resolveHookPath(file, cwd)).sort()
  };
  return createHash2("sha1").update(JSON.stringify(identity)).digest("hex");
}
function recordHookTask(payload, files, cwd, taskId) {
  const state = readHookTaskState();
  const key = hookTaskKey(payload, files, cwd);
  const entries = state[key] ?? [];
  entries.push({
    taskId,
    files: files.map((file) => resolveHookPath(file, cwd)),
    createdAt: (/* @__PURE__ */ new Date()).toISOString()
  });
  state[key] = entries.slice(-20);
  writeHookTaskState(state);
}
function consumeHookTask(payload, files, cwd) {
  const state = readHookTaskState();
  const key = hookTaskKey(payload, files, cwd);
  const entries = state[key] ?? [];
  const entry2 = entries.shift();
  if (entries.length > 0) state[key] = entries;
  else delete state[key];
  writeHookTaskState(state);
  return entry2?.taskId ?? null;
}
function uniqueActiveHookTaskId(database, params) {
  const absFiles = params.files.map((file) => resolveHookPath(file, params.workspacePath));
  if (absFiles.length === 0) return null;
  const where = [
    "fl.agent_id = ?",
    "ai.status = 'ACTIVE'",
    `fl.file_path IN (${absFiles.map(() => "?").join(",")})`,
    "ai.workspace_path = ?"
  ];
  const binds = [
    params.agentId,
    ...absFiles,
    normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve10(params.workspacePath)
  ];
  if (params.artifact) {
    where.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    binds.push(params.artifact);
  }
  const rows = database.prepare(
    `SELECT DISTINCT fl.task_id
       FROM locks fl
       JOIN tasks ai ON ai.task_id = fl.task_id
      WHERE ${where.join(" AND ")}
      ORDER BY fl.task_id ASC`
  ).all(...binds);
  return rows.length === 1 ? rows[0].task_id : null;
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
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:pre-edit");
    const result = preFlightIntent(database, {
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? process.cwd(),
      artifact: artifact(payload),
      rationale: "auto: file edit via lifecycle hook",
      testPlan: "post-edit verification",
      targetFiles: files,
      ttlMs: 10 * 6e4
    });
    if (!result.ok) {
      console.error("octocode-awareness: target file is locked by another agent \u2014 edit blocked.");
      console.error(JSON.stringify(result));
      return 2;
    }
    recordHookTask(payload, files, workspace(payload) ?? process.cwd(), result.task.task_id);
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
    const correlatedTaskId = consumeHookTask(payload, files, hookWorkspace) ?? uniqueActiveHookTaskId(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      files
    });
    if (!correlatedTaskId) {
      console.error("octocode-awareness post-edit warning (continuing): could not identify a unique hook task to release; leaving locks for verify/cleanup.");
      return 0;
    }
    const release = releaseFileLock(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      taskId: correlatedTaskId,
      status: "PENDING"
    });
    const taskId = release.task_ids.length === 1 ? release.task_ids[0] : correlatedTaskId;
    for (const file of files) {
      insertEditLog(database, {
        agentId: hookAgentId,
        taskId,
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
  const skillRoot = process.env.OCTOCODE_SKILL_ROOT;
  if (!skillRoot) return 0;
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const insideSkill = files.some((file) => isInsidePath(resolveHookPath(file), skillRoot));
  if (!insideSkill) return 0;
  if (process.env.OCTOCODE_ALLOW_HARNESS_APPLY !== "1") {
    console.error("octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1. Edit blocked.");
    return 2;
  }
  const branch = gitBranchOf(skillRoot);
  if (branch === "main" || branch === "master") {
    console.error(`octocode-awareness: harness self-fix is never allowed on ${branch}. Create a dedicated branch first. Edit blocked.`);
    return 2;
  }
  if (!branch || branch === "HEAD") {
    if (process.env.OCTOCODE_HARNESS_BRANCH_OK !== "1") {
      console.error("octocode-awareness: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge. Edit blocked.");
      return 2;
    }
  }
  return 0;
}
function gitBranchOf(dir) {
  try {
    const r = spawnSync6("git", ["-C", dir, "rev-parse", "--abbrev-ref", "HEAD"], {
      encoding: "utf8",
      timeout: 5e3
    });
    return r.status === 0 ? String(r.stdout).trim() : null;
  } catch {
    return null;
  }
}
async function runStopVerify(payload) {
  if (process.env.OCTOCODE_NO_VERIFY_GATE === "1" || isStopHookActive(payload)) return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, "hook:stop-verify");
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      const parts = [];
      if (report.unverified.length > 0) {
        parts.push(report.unverified.map((u) => `${u.status}:${u.task_id}: ${u.test_plan}`).join("; "));
      }
      if (report.stale_active.length > 0) {
        parts.push("Stale active (lock expired): " + report.stale_active.map((s) => `${s.task_id}: ${s.rationale}`).join("; "));
      }
      console.error(`octocode-awareness: concluding with unverified work. ${parts.join(" | ")}`);
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
  const markerPath = join7(memoryHome2, ".last-digest-epoch-ms");
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
      mkdirSync4(memoryHome2, { recursive: true });
      writeFileSync3(markerPath, String(now), "utf8");
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
        additionalContext: result.additionalContext,
        additional_context: result.additionalContext
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
var isMain = process.argv[1] ? fileURLToPath2(import.meta.url) === resolve10(process.argv[1]) : false;
var invokedAsHookRunner = process.argv[1] ? /^hook-runner\.(js|mjs|ts)$/.test(basename3(process.argv[1])) : false;
if (isMain && invokedAsHookRunner) {
  process.exitCode = await main();
}

// bin/awareness.ts
var MAX_CLI_TTL_SECONDS = 10 * 60;
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
  "task_id",
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
  "forget": ["memory_id", "tag", "tags", "before", "max_importance", "workspace", "artifact", "repo", "ref", "dry_run"],
  "reflect": ["agent_id", "task", "outcome", "lesson", "worked", "didnt_work", "fix_repo", "fix_file", "fix_harness", "failure_signature", "importance", "judgment_note", "duo", "eval_failure_json", "workspace", "artifact", "repo", "ref"],
  "refine-set": ["agent_id", "reasoning", "remember", "quality", "state", "workspace", "artifact", "repo", "ref", "file", "refinement_id"],
  "refine-get": ["workspace", "artifact", "repo", "ref", "quality", "include_handoffs", "state", "limit"],
  "refine-delete": ["refinement_id", "workspace", "artifact", "dry_run"],
  "pre-flight-intent": ["agent_id", "workspace", "artifact", "rationale", "test_plan", "plan_doc_ref", "target_file", "file", "lock_type", "ttl_minutes", "ttl_seconds", "wait_seconds", "retry_interval"],
  "release-file-lock": ["agent_id", "task_id", "target_file", "file", "status", "verified", "verified_note", "workspace", "artifact"],
  "status": ["workspace", "artifact", "limit"],
  "init": [],
  "self-test": [],
  "prune-stale-locks": ["older_than_minutes", "expired_only", "agent_id", "target_file", "workspace", "artifact", "dry_run"],
  "audit-unverified": ["agent_id", "workspace", "artifact", "abandon"],
  "verify": ["task_id", "all_pending", "agent_id", "status", "message", "workspace", "artifact"],
  "mine-weakness": ["agent_id", "workspace", "artifact", "min_count", "limit", "cwd"],
  "doc-staleness": ["agent_id", "workspace", "artifact", "targets_json", "min_edits", "min_lines", "propose", "session_id"],
  "docs-catalog": ["action", "name"],
  "export-harness": ["limit", "min_importance", "workspace", "artifact"],
  "query": ["view", "query", "limit", "format", "out", "workspace", "artifact", "repo", "ref", "agent_id", "state", "label", "file", "since", "include_bodies"],
  "attend": ["query", "limit", "workspace", "artifact", "repo", "ref", "file", "include_bodies", "explain_organ"],
  "repo-inject": ["query", "limit", "out", "out_dir", "workspace", "artifact", "repo", "ref", "mode", "check", "include_view", "include_bodies"],
  "agent-registry": ["action", "agent_id", "agent_name", "workspace", "artifact", "context", "limit"],
  "agent-signal": ["action", "agent_id", "workspace", "artifact", "repo", "ref", "kind", "subject", "body", "to_agent", "file", "ref_id", "importance", "in_reply_to", "thread_id", "signal_id", "all", "unread_only", "mark_read", "limit", "format"],
  "notify-prune": ["signal_id", "resolved", "older_than_days", "dry_run", "workspace", "artifact"],
  "session-capture": ["agent_id", "workspace", "artifact", "repo", "ref", "reason", "cwd"],
  "wait-for-lock": ["agent_id", "target_file", "file", "workspace", "artifact", "lock_type", "wait_seconds", "retry_interval"],
  "digest": ["retention_days", "refinement_handoff_retention_days", "refinement_done_retention_days", "dry_run", "export_doc", "workspace", "artifact"],
  "hook-run": [],
  "hooks-install": ["host", "project_dir", "global", "check", "strict", "dry_run", "remove"],
  "schema": []
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
var COMMAND_ROUTES = {
  "memory record": { command: "tell-memory" },
  "memory recall": { command: "get-memory" },
  "memory forget": { command: "forget" },
  "workspace status": { command: "status" },
  "lock acquire": { command: "pre-flight-intent" },
  "lock release": { command: "release-file-lock" },
  "lock wait": { command: "wait-for-lock" },
  "lock prune": { command: "prune-stale-locks" },
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
  const here = dirname6(fileURLToPath3(import.meta.url));
  const candidates = [
    join8(here, "..", "skills", "octocode-awareness", "scripts"),
    // dist/skills/ — bundled, preferred
    join8(here, "..", "..", "skills", "octocode-awareness", "scripts"),
    // <packageRoot>/skills/ — source fallback
    here
    // dist/bin/ — last resort
  ];
  const scriptsDir = candidates.find(
    (candidate) => existsSync4(join8(candidate, "schema.mjs")) || existsSync4(join8(candidate, "hooks"))
  ) ?? candidates[0];
  return join8(scriptsDir, ...segments);
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
    label: normalizeLabel(label),
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
          const ranked = loadMemoriesByIds(db3, hits.map((hit) => hit.memory_id));
          const simById = new Map(hits.map((hit) => [hit.memory_id, hit.similarity]));
          for (const memory of ranked) {
            const similarity = simById.get(memory.memory_id) ?? 0;
            memory.score = similarity;
            memory.lexical = similarity;
          }
          bumpAccess(db3, ranked.map((memory) => memory.memory_id));
          payload["memories"] = ranked.slice(0, limit);
          payload["count"] = Math.min(ranked.length, limit);
          payload["mode"] = "semantic";
          payload["embedding_model"] = model;
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
function cmdPreFlightIntent(db3, args2, dbPath2, opts2) {
  const rawTarget = args2["target_file"] ?? args2["file"];
  const targetFiles = Array.isArray(rawTarget) ? rawTarget : rawTarget ? [String(rawTarget)] : [];
  const ttlMinutes = args2["ttl_minutes"] ? parseInt(String(args2["ttl_minutes"]), 10) : null;
  const ttlSeconds = args2["ttl_seconds"] ? parseInt(String(args2["ttl_seconds"]), 10) : null;
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1)) die("--ttl-minutes must be >= 1");
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1)) die("--ttl-seconds must be >= 1");
  if (ttlMinutes != null && ttlMinutes > 10) die("--ttl-minutes must be <= 10");
  if (ttlSeconds != null && ttlSeconds > MAX_CLI_TTL_SECONDS) die("--ttl-seconds must be <= 600");
  const ttlMs = ttlSeconds != null ? ttlSeconds * 1e3 : ttlMinutes != null ? ttlMinutes * 6e4 : null;
  const claimParams = {
    agentId: String(args2["agent_id"] ?? "agent"),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    rationale: String(args2["rationale"] ?? "agent write operation"),
    testPlan: String(args2["test_plan"] ?? "post-edit verification"),
    planDocRef: args2["plan_doc_ref"] ? String(args2["plan_doc_ref"]) : null,
    targetFiles,
    lockType: String(args2["lock_type"] ?? "EXCLUSIVE"),
    ttlMs
  };
  let result = preFlightIntent(db3, claimParams);
  const waitSeconds = args2["wait_seconds"] ? parseInt(String(args2["wait_seconds"]), 10) : null;
  if (!result.ok && waitSeconds != null && waitSeconds > 0) {
    const retrySeconds = args2["retry_interval"] ? parseInt(String(args2["retry_interval"]), 10) : null;
    const wait = waitForLock(db3, {
      agent_id: claimParams.agentId,
      target_files: targetFiles,
      workspace: claimParams.workspacePath ?? void 0,
      artifact: claimParams.artifact ?? void 0,
      lock_type: claimParams.lockType,
      wait_ms: waitSeconds * 1e3,
      retry_interval_ms: retrySeconds != null ? retrySeconds * 1e3 : void 0
    });
    if (wait.lock_free) result = preFlightIntent(db3, claimParams);
  }
  if (!result.ok) return emit({ db_path: dbPath2, ...result }, 2, opts2);
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdAuditUnverified(db3, args2, dbPath2, opts2) {
  const result = auditUnverified(db3, {
    agentId: args2["agent_id"] ? String(args2["agent_id"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    abandon: Boolean(args2["abandon"])
  });
  return emit({ db_path: dbPath2, ...result }, result.count > 0 ? 1 : 0, opts2);
}
function cmdVerify(db3, args2, dbPath2, opts2) {
  const allPending = Boolean(args2["all_pending"]);
  const taskIds = valuesFor(args2, "task_id");
  if (!allPending && taskIds.length === 0) {
    return emit({ error: "--task-id is required (or use --all-pending)" }, 1, opts2);
  }
  const statusArg = args2["status"] ? String(args2["status"]) : "SUCCESS";
  if (statusArg !== "SUCCESS" && statusArg !== "FAILED") {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts2);
  }
  if (!allPending && taskIds.length > 1) {
    const results = taskIds.map((taskId) => markVerified(db3, {
      taskId,
      agentId: String(args2["agent_id"] ?? "agent"),
      workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
      artifact: args2["artifact"] ? String(args2["artifact"]) : null,
      message: args2["message"] ? String(args2["message"]) : void 0,
      status: statusArg
    }));
    const failed = results.find((result2) => !result2.ok);
    if (failed && !failed.ok) {
      return emit({ db_path: dbPath2, ok: false, error: failed.error, task_id: null, task_ids: taskIds, results }, 1, opts2);
    }
    return emit({
      db_path: dbPath2,
      task_id: null,
      task_ids: taskIds,
      count: results.length,
      status: statusArg,
      results
    }, 0, opts2);
  }
  const result = markVerified(db3, {
    taskId: taskIds[0],
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
  const taskId = firstValue(args2, "task_id");
  if (!taskId && targetFiles.length === 0) {
    return emit({ error: "release-file-lock requires --task-id or --target-file" }, 1, opts2);
  }
  const result = releaseFileLock(db3, {
    agentId: String(args2["agent_id"] ?? "agent"),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null,
    taskId: taskId ?? null,
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
function cmdMemoryIndex(db3, args2, dbPath2, opts2) {
  const limit = args2["limit"] ? parseInt(String(args2["limit"]), 10) : 30;
  const minImportance = args2["min_importance"] ? parseInt(String(args2["min_importance"]), 10) : 1;
  const stdout = Boolean(args2["stdout"]);
  const wsPath = args2["workspace"] ? String(args2["workspace"]) : null;
  const conds = [];
  const binds = [minImportance];
  let sql = `SELECT memory_id, label, importance, task_context, observation, tags_json,
                    failure_signature, created_at
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
  const rows = db3.prepare(sql).all(...binds);
  if (rows.length > 0) {
    const refs = db3.prepare(
      `SELECT memory_id, reference
       FROM memory_refs
       WHERE memory_id IN (${rows.map(() => "?").join(",")})
       ORDER BY memory_id, ordinal`
    ).all(...rows.map((row) => row.memory_id));
    const refsByMemory = /* @__PURE__ */ new Map();
    for (const ref of refs) {
      const list = refsByMemory.get(ref.memory_id) ?? [];
      list.push(ref.reference);
      refsByMemory.set(ref.memory_id, list);
    }
    for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
  }
  const now = (/* @__PURE__ */ new Date()).toISOString().slice(0, 10);
  const lines = [
    `# Memory Index \u2014 ${now}`,
    `<!-- Auto-generated by octocode-awareness memory-index. Regenerate after recording or forgetting memories. -->`,
    "",
    `**${rows.length} active memories** (importance \u2265 ${minImportance}, sorted by salience)`,
    ""
  ];
  for (const m of rows) {
    const tags = parseJsonList(m.tags_json).join(", ");
    lines.push(`## [${m.label}:${m.importance}] ${m.task_context.slice(0, 80)}`);
    lines.push(`> ${m.observation.slice(0, 200)}`);
    if (tags) lines.push(`*Tags: ${tags}*`);
    if (m.references.length > 0) lines.push(`*References: ${m.references.join(", ")}*`);
    if (m.failure_signature) lines.push(`*Failure: ${m.failure_signature}*`);
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
    mkdirSync5(dirname6(targetPath), { recursive: true });
    writeFileSync4(targetPath, content, "utf8");
  } catch (err) {
    return emit({ db_path: dbPath2, error: `Could not write MEMORY.md: ${err.message}` }, 1, opts2);
  }
  return emit({ db_path: dbPath2, ok: true, path: targetPath, count: rows.length }, 0, opts2);
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
    const resolvedOutPath = isAbsolute6(outPath) ? resolve11(outPath) : resolve11(workspacePath, outPath);
    mkdirSync5(dirname6(resolvedOutPath), { recursive: true });
    writeFileSync4(resolvedOutPath, formatAwarenessQueryResult(result, format), "utf8");
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
function cmdView(db3, args2, dbPath2, opts2) {
  const view = String(args2["view"] ?? args2._[0] ?? "all");
  const result = writeAwarenessView(db3, {
    view,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : process.cwd(),
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
    includeBodies: flagBool(args2["include_bodies"]),
    out: args2["out"] ? String(args2["out"]) : void 0
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
    const result = listSkillDocs();
    return emit({
      ok: true,
      count: result.count,
      root: result.root,
      docs: result.docs.map((doc) => ({
        name: doc.name,
        title: doc.title,
        description: doc.description,
        kind: doc.kind,
        path: doc.path
      })),
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
    const sessionId = args2["session_id"] ? String(args2["session_id"]) : null;
    for (const entry2 of result.entries) {
      if (!entry2.stale) continue;
      const harnessId = proposeDocRefresh(db3, entry2, { agentId: agentId2, sessionId, workspacePath, artifact: artifact2 });
      proposed.push({ target_file: entry2.doc_file, harness_id: harnessId });
    }
  }
  return emit({ db_path: dbPath2, ...result, proposed }, 0, opts2);
}
function cmdNotify(db3, args2, dbPath2, opts2) {
  if (!args2["agent_id"]) return emit({ error: "--agent-id is required" }, 1, opts2);
  if (!args2["kind"]) return emit({ error: "--kind is required" }, 1, opts2);
  if (!args2["subject"]) return emit({ error: "--subject is required" }, 1, opts2);
  const rawFiles = args2["file"];
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [String(rawFiles)] : [];
  const rawRefIds = args2["ref_id"];
  const refIds = Array.isArray(rawRefIds) ? rawRefIds : rawRefIds ? [String(rawRefIds)] : [];
  const result = insertNotification(db3, {
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
function cmdNotifyGet(db3, args2, dbPath2, opts2) {
  if (!args2["agent_id"]) return emit({ error: "--agent-id is required" }, 1, opts2);
  const rawKinds = args2["kind"];
  const kinds = Array.isArray(rawKinds) ? rawKinds : rawKinds ? [String(rawKinds)] : [];
  const result = getNotifications(db3, {
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
function cmdNotifyResolve(db3, args2, dbPath2, opts2) {
  const rawIds = args2["signal_id"];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = resolveNotification(db3, {
    agentId: args2["agent_id"] ? String(args2["agent_id"]) : null,
    notificationIds,
    threadId: args2["thread_id"] ? String(args2["thread_id"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    artifact: args2["artifact"] ? String(args2["artifact"]) : null
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
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
  const publishKind = kinds[0];
  const rawSignalIds = args2["signal_id"];
  const signalIds = Array.isArray(rawSignalIds) ? rawSignalIds : rawSignalIds ? [String(rawSignalIds)] : [];
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
    kinds
  });
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
  evictExpiredLocks(db3);
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
  const taskScope = ["status='ACTIVE'"];
  const taskBinds = [];
  if (wsPath) {
    taskScope.push("workspace_path = ?");
    taskBinds.push(wsPath);
  }
  if (artifact2) {
    taskScope.push("(artifact = ? OR artifact IS NULL)");
    taskBinds.push(artifact2);
  }
  const activeTasks = db3.prepare(`SELECT COUNT(*) AS count FROM tasks WHERE ${taskScope.join(" AND ")}`).get(...taskBinds).count;
  const limit = Math.min(100, Math.max(1, parseInt(String(args2["limit"] ?? "20"), 10) || 20));
  const lockWhere = [];
  const lockBinds = [];
  if (wsPath) {
    lockWhere.push("ai.workspace_path = ?");
    lockBinds.push(wsPath);
  }
  if (artifact2) {
    lockWhere.push("(ai.artifact = ? OR ai.artifact IS NULL)");
    lockBinds.push(artifact2);
  }
  const locks = db3.prepare(
    `SELECT fl.file_path, fl.task_id, ai.agent_id, ai.workspace_path, ai.artifact, fl.lock_type, fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN tasks ai ON ai.task_id = fl.task_id
       ${lockWhere.length > 0 ? `WHERE ${lockWhere.join(" AND ")}` : ""}
       ORDER BY fl.acquired_at DESC LIMIT ?`
  ).all(...lockBinds, limit);
  const openRefinements = db3.prepare(
    `SELECT COUNT(*) AS count FROM refinements
      WHERE state IN ('open','ongoing')
      ${wsPath ? "AND (workspace_path = ? OR workspace_path IS NULL)" : ""}
      ${artifact2 ? "AND (artifact = ? OR artifact IS NULL)" : ""}`
  ).get(...[...wsPath ? [wsPath] : [], ...artifact2 ? [artifact2] : []]).count;
  return emit({
    db_path: dbPath2,
    fts_enabled: hasFts(db3),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    active_task_count: activeTasks,
    open_refinements: openRefinements,
    locks,
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
  Registry fallback only when no local CLI exists:
    npx @octocodeai/octocode-awareness maintenance init --compact
  Then install the bundled Agent Skill:
    npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common
  Registry fallback:
    npx octocode skill --name octocode-awareness

supported agents: Codex, Claude Code, Cursor, Pi, and custom library/CLI hosts
surfaces: CLI = control plane; Agent Skill = operating loop; hooks/Pi bridge = lifecycle automation

start: attend, workspace status, memory recall, refinement get, signal list, query <view>
edit: lock acquire, lock wait, lock release, lock prune, verify mark, verify audit
messages: signal publish, signal list, signal reply, signal ack, signal resolve, signal prune, agent register, agent list
learning: memory record, memory forget, refinement set, refinement get, refinement delete, reflect record, reflect mine-weakness, reflect export-harness, docs list, docs show, docs staleness
repo context: query <view> [--format json|table|csv|markdown|html], repo inject
hooks: hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end>, hooks install|check|remove --host claude|codex|cursor
utility: session capture, maintenance init, maintenance self-test, maintenance digest

examples:
  octocode-awareness workspace status --workspace "$PWD" --compact
  octocode-awareness attend --workspace "$PWD" --query "current task" --compact
  octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact
  octocode-awareness docs list --compact
  octocode-awareness docs show full-flow
  octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness schema commands --compact
  octocode-awareness query gotchas --workspace "$PWD" --format json --limit 20 --compact
  octocode-awareness repo inject --workspace "$PWD" --mode local --compact

Run "octocode-awareness <command> --help" for command flags. Exit 2 = lock conflict or wait timeout.`;
var HELP_COMPACT = `octocode-awareness: canonical noun/verb CLI. Use --compact for JSON.
local-first: octocode-awareness <command>; fallback: npx @octocodeai/octocode-awareness <command>; skill: npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common; agents: Codex, Claude, Cursor, Pi
start: attend; workspace status; memory recall; refinement get; signal list; docs list
edit: lock acquire|wait|release|prune; verify audit|mark
msg: signal publish|list|reply|ack|resolve|prune; agent register|list
learn: memory record|forget; reflect record|mine-weakness|export-harness; maintenance digest
repo: query <view> --format json|table|csv|markdown|html; repo inject
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
  "reflect": "reflect"
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
  "release-file-lock": "octocode-awareness lock release --agent-id agent --task-id task_123 --status SUCCESS --verified --compact",
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
  "docs list": "octocode-awareness docs list --compact",
  "docs show": "octocode-awareness docs show full-flow",
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
schema: octocode-awareness schema json-schema tell_memory --compact`,
  "get-memory": `usage: octocode-awareness memory recall [options]
filters: [--query <text>] [--limit <n>] [--min-importance <n>] [--label <l>]... [--tag <t>]... [--reference <r>]... [--file <p>]... [--regex <r>]... [--file-regex <r>]...
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--strict-scope] [--global-only]
rank: [--sort smart|score|importance|recent|accessed] [--state ACTIVE|SUPERSEDED]... [--as-of <iso>] [--semantic] [--explain]
example: octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
schema: octocode-awareness schema json-schema get_memory --compact`,
  "pre-flight-intent": `usage: octocode-awareness lock acquire --agent-id <id> --target-file <p>... [--workspace <p>] [--artifact <a>] [--rationale <t>] [--test-plan <t>] [--lock-type EXCLUSIVE|SHARED] [--ttl-minutes <n>] [--wait-seconds <n>]
example: octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact
schema: octocode-awareness schema json-schema pre_flight_intent --compact`,
  "agent-signal": `usage: octocode-awareness signal publish|list|reply|ack|resolve --agent-id <id> [--to-agent <id>]... [--signal-id <id>]... [--thread-id <id>] [--kind <k>] [--subject <t>] [--body <t>] [--file <p>]...
examples:
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --file src/file.ts --workspace "$PWD" --compact
  octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact
schema: octocode-awareness schema json-schema agent_signal --compact`,
  "verify": `usage: octocode-awareness verify mark (--task-id <id>... | --all-pending) --agent-id <id> [--status SUCCESS|FAILED] [--message <t>] [--workspace <p>] [--artifact <a>]
example: octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema verify --compact`,
  "reflect": `usage: octocode-awareness reflect record --agent-id <id> --task <text> --outcome worked|partial|failed [--lesson <t>] [--fix-repo <t>] [--fix-file <p>]... [--failure-signature <s>]
example: octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep CLI nouns canonical" --compact
schema: octocode-awareness schema json-schema reflect --compact`,
  "query": `usage: octocode-awareness query <all|repo-profile|memories|gotchas|lessons|tasks|locks|agents|signals|refinements|files|activity|workboard> [--workspace <repo>] [--format json|table|csv|markdown|html] [--out <path>]
examples:
  octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
schema: octocode-awareness schema json-schema query --compact`,
  "attend": `usage: octocode-awareness attend [--workspace <repo>] [--query <text>] [--file <p>]... [--limit <n>] [--include-bodies] [--explain-organ]
example: octocode-awareness attend --query "current task" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema attend --compact`,
  "repo-inject": `usage: octocode-awareness repo inject [--workspace <repo>] [--out .octocode] [--mode local|share] [--no-check] [--no-include-view]
example: octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
schema: octocode-awareness schema json-schema repo_inject --compact`,
  "docs-catalog": `usage: octocode-awareness docs list|show [name]
examples:
  octocode-awareness docs list --compact
  octocode-awareness docs show full-flow
  octocode-awareness docs show full-flow --compact
schema: octocode-awareness schema json-schema docs_catalog --compact`,
  "hook-run": `usage: octocode-awareness hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json`,
  "hooks-install": hooksInstallUsage(),
  "schema": `usage: octocode-awareness schema commands|list|json-schema <name>|example <name>|validate <name> <json-file|->
examples:
  octocode-awareness schema commands --compact
  octocode-awareness schema json-schema query --compact`,
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
  const result = spawnSync7(process.execPath, [script, ...rest], { stdio: "inherit" });
  process.exit(result.status ?? 1);
}
if (command === "hook-run") {
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
    case "notify-get": {
      const ngFormat = String(args["format"] ?? "json");
      const ngAgentId = args["agent_id"];
      if (ngAgentId && ngFormat !== "hook") {
        exitCode = cmdNotifyGet(db2, args, dbPath, opts);
      } else {
        const ngParams = {
          workspace: args["workspace"],
          artifact: args["artifact"],
          format: ngFormat,
          agent_id: ngAgentId
        };
        const ngResult = notifyGet(db2, ngParams);
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
    case "workspace-status": {
      const wsStatusResult = getWorkspaceStatus(db2, {
        workspace_path: args["workspace"],
        artifact: args["artifact"]
      });
      exitCode = emit({ db_path: dbPath, ...wsStatusResult }, 0, opts);
      break;
    }
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
          const { mkdirSync: mkdirSync6, writeFileSync: writeFileSync5 } = await import("node:fs");
          const { join: join9 } = await import("node:path");
          const docDir = join9(wsPath, ".octocode", "memory-reports");
          mkdirSync6(docDir, { recursive: true });
          const dateStr = (/* @__PURE__ */ new Date()).toISOString().slice(0, 16).replace("T", "-").replace(":", "");
          const docPath = typeof (args["export_doc"] ?? args["export-doc"]) === "string" ? args["export_doc"] ?? args["export-doc"] : join9(docDir, `memory-report-${dateStr}.md`);
          writeFileSync5(docPath, exportMemoryDoc(db2, { workspace_path: wsPath, artifact: artifact2 }), "utf8");
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
      const waitResult = waitForLock(db2, {
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
      exitCode = cmdMemoryIndex(db2, args, dbPath, opts);
      break;
    case "forget":
      exitCode = cmdForget(db2, args, dbPath, opts);
      break;
    case "refine-delete":
      exitCode = cmdRefineDelete(db2, args, dbPath, opts);
      break;
    case "export-harness":
      exitCode = cmdExportHarness(db2, args, dbPath, opts);
      break;
    case "query":
      exitCode = cmdQuery(db2, args, dbPath, opts);
      break;
    case "attend":
      exitCode = cmdAttend(db2, args, dbPath, opts);
      break;
    case "view":
      exitCode = cmdView(db2, args, dbPath, opts);
      break;
    case "repo-inject":
      exitCode = cmdRepoInject(db2, args, dbPath, opts);
      break;
    case "agent-registry":
      exitCode = cmdAgentRegistry(db2, args, dbPath, opts);
      break;
    case "notify":
      exitCode = cmdNotify(db2, args, dbPath, opts);
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
    case "notify-resolve":
      exitCode = cmdNotifyResolve(db2, args, dbPath, opts);
      break;
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
//# sourceMappingURL=awareness.js.map
