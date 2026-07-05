#!/usr/bin/env node

// bin/awareness.ts
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
function tagsText(tags) {
  return tags.length === 0 ? "," : "," + tags.join(",") + ",";
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
function normalizeFilePath(filePath) {
  if (!filePath) return null;
  return resolve(String(filePath));
}
function rowToMemory(row) {
  return {
    memory_id: row.memory_id,
    agent_id: row.agent_id,
    task_context: row.task_context,
    observation: row.observation,
    importance_score: row.importance_score,
    state: row.state ?? "ACTIVE",
    label: row.label ?? "OTHER",
    superseded_by: row.superseded_by ?? null,
    tags: parseJsonList(row.tags_json),
    references: parseJsonList(row.references_json),
    workspace_path: row.workspace_path ?? null,
    repo: row.repo ?? null,
    ref: row.ref ?? null,
    file: row.file ?? null,
    novelty_score: row.novelty_score ?? null,
    similar_memory_ids: parseJsonList(row.similar_memory_ids_json),
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
var FTS_INDEX_VERSION = "2";
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
  return db2;
}
function initDb(db2) {
  db2.exec(`
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
      quality TEXT NOT NULL CHECK(quality IN ('good','bad')) DEFAULT 'good',
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
  ensureMemoryColumns(db2);
  ensureIntentColumns(db2);
  try {
    db2.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts
      USING fts5(memory_id UNINDEXED, task_context, observation, tags)
    `);
  } catch {
  }
  ensureFtsVersion(db2);
}
function tableColumns(db2, tableName) {
  const rows = db2.prepare(`PRAGMA table_info(${tableName})`).all();
  return new Set(rows.map((r) => r.name));
}
function ensureMemoryColumns(db2) {
  const cols = tableColumns(db2, "agent_memories");
  const alterations = [
    ["state", "TEXT NOT NULL DEFAULT 'ACTIVE'"],
    ["label", "TEXT NOT NULL DEFAULT 'OTHER'"],
    ["superseded_by", "TEXT"],
    ["updated_at", "TEXT"],
    ["file", "TEXT"],
    ["novelty_score", "REAL"],
    ["similar_memory_ids_json", "TEXT NOT NULL DEFAULT '[]'"],
    ["last_accessed_at", "TEXT"],
    ["access_count", "INTEGER NOT NULL DEFAULT 0"],
    ["decay_half_life_days", "REAL"],
    ["failure_signature", "TEXT"],
    ["valid_from", "TEXT"],
    ["valid_to", "TEXT"],
    ["expired_at", "TEXT"],
    ["references_json", "TEXT NOT NULL DEFAULT '[]'"],
    ["workspace_path", "TEXT"],
    ["repo", "TEXT"],
    ["ref", "TEXT"],
    ["embedding", "BLOB"],
    ["embedding_model", "TEXT"]
  ];
  for (const [col, def] of alterations) {
    if (!cols.has(col)) {
      db2.exec(`ALTER TABLE agent_memories ADD COLUMN ${col} ${def}`);
    }
  }
}
function ensureIntentColumns(db2) {
  const cols = tableColumns(db2, "agent_intents");
  if (!cols.has("workspace_path")) {
    db2.exec("ALTER TABLE agent_intents ADD COLUMN workspace_path TEXT");
  }
  if (!cols.has("files_json")) {
    db2.exec("ALTER TABLE agent_intents ADD COLUMN files_json TEXT NOT NULL DEFAULT '[]'");
  }
}
function hasFts(db2) {
  const row = db2.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_fts'"
  ).get();
  return Boolean(row);
}
function ftsTermsForRow(row) {
  const tags = parseJsonList(row.tags_json);
  const refs = parseJsonList(row.references_json);
  const label = (row.label ?? "OTHER").toLowerCase();
  return [
    ...tags,
    ...refs,
    label,
    row.file ?? "",
    row.failure_signature ?? "",
    row.workspace_path ?? "",
    row.repo ?? "",
    row.ref ?? ""
  ].filter(Boolean).join(" ");
}
function rebuildFts(db2) {
  db2.exec("DELETE FROM memory_fts");
  const rows = db2.prepare("SELECT * FROM agent_memories").all();
  const insert = db2.prepare(
    "INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
  );
  for (const row of rows) {
    insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
  }
}
function ensureFtsVersion(db2) {
  if (!hasFts(db2)) return;
  const row = db2.prepare(
    "SELECT value FROM awareness_meta WHERE key='memory_fts_version'"
  ).get();
  if (row?.value === FTS_INDEX_VERSION) return;
  rebuildFts(db2);
  db2.prepare(
    "INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('memory_fts_version', ?)"
  ).run(FTS_INDEX_VERSION);
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
    repo: partial.repo ?? null,
    ref: partial.ref ?? null
  };
  if (scope.workspace_path && scope.repo) return scope;
  const git = detectGit(cwd ?? process.cwd());
  if (!git.is_repo) return scope;
  if (!scope.workspace_path && git.root) scope.workspace_path = git.root;
  if (!scope.repo && git.repo) scope.repo = git.repo;
  if (!scope.ref && git.branch) scope.ref = git.branch;
  return scope;
}

// src/memory.ts
var DECAY_WEIGHTS = { importance: 0.25, recency: 0.3, access: 0.15, lexical: 0.3 };
var DEFAULT_HALF_LIFE_DAYS = 30;
var ACCESS_SATURATION = 50;
var SCORING_PREFETCH_FACTOR = 3;
var SIMILARITY_THRESHOLD = 0.45;
var SIMILARITY_PREFETCH = 12;
function textTokens(text) {
  const stopWords = /* @__PURE__ */ new Set(["the", "and", "for", "with", "this", "that", "about", "before", "after", "from", "into", "when", "what"]);
  return new Set((text.toLowerCase().match(/[a-z0-9_:-]{3,}/g) ?? []).filter((t) => !stopWords.has(t)));
}
function jaccard(a, b) {
  if (a.size === 0 || b.size === 0) return 0;
  let intersection = 0;
  for (const token of a) if (b.has(token)) intersection++;
  return intersection / (a.size + b.size - intersection);
}
function findSimilarMemories(db2, text, limit = 3, excludeMemoryId = null) {
  const queryTokens = textTokens(text);
  if (queryTokens.size === 0) return [];
  const candidates = lexicalSearch(
    db2,
    text,
    SIMILARITY_PREFETCH,
    1,
    [],
    [],
    ["ACTIVE"]
  ).filter((m) => m.memory_id !== excludeMemoryId);
  return candidates.map((m) => ({
    memory_id: m.memory_id,
    similarity: jaccard(queryTokens, textTokens(`${m.task_context} ${m.observation}`))
  })).filter((m) => m.similarity >= SIMILARITY_THRESHOLD).sort((a, b) => b.similarity - a.similarity).slice(0, limit);
}
function decayScore(memory, lexical, weights = DECAY_WEIGHTS) {
  const halfLife = memory.decay_half_life_days ?? DEFAULT_HALF_LIFE_DAYS;
  const lastUsedStr = memory.last_accessed_at ?? memory.created_at;
  let recency = 0;
  if (lastUsedStr) {
    const ageDays = Math.max(0, (Date.now() - new Date(lastUsedStr).getTime()) / 864e5);
    recency = Math.exp(-Math.LN2 * ageDays / Math.max(halfLife, 0.01));
  }
  const importance = (memory.importance_score ?? 0) / 10;
  const access = Math.log1p(memory.access_count ?? 0) / Math.log1p(ACCESS_SATURATION);
  const lexNorm = Math.max(0, Math.min(1, lexical));
  return weights.importance * importance + weights.recency * recency + weights.access * Math.min(access, 1) + weights.lexical * lexNorm;
}
function buildFtsQuery(query) {
  const stopWords = /* @__PURE__ */ new Set(["the", "and", "for", "with", "this", "that", "about", "before", "after"]);
  const tokens = [
    ...new Set(
      (query.toLowerCase().match(/[a-z0-9_]{2,}/g) ?? []).filter((t) => !stopWords.has(t))
    )
  ].slice(0, 16);
  return tokens.length > 0 ? tokens.join(" OR ") : null;
}
function fallbackSearch(db2, conditions, params, limit) {
  const sql = `
    SELECT m.*, 0 AS _bm25
    FROM agent_memories m
    WHERE ${conditions.join(" AND ")}
    ORDER BY m.importance_score DESC, m.created_at DESC
    LIMIT ?
  `;
  return db2.prepare(sql).all(...params, limit);
}
function lexicalSearch(db2, query, limit, minImportance, tags, labels, states) {
  const ftsQuery = query ? buildFtsQuery(query) : null;
  const params = [];
  const conditions = [
    "m.importance_score >= ?",
    `m.state IN (${states.map(() => "?").join(",")})`
  ];
  params.push(minImportance, ...states);
  if (labels.length > 0) {
    conditions.push(`m.label IN (${labels.map(() => "?").join(",")})`);
    params.push(...labels);
  }
  for (const tag of tags) {
    conditions.push("m.tags_text LIKE ?");
    params.push(`%,${tag},%`);
  }
  let rows;
  if (ftsQuery && hasFts(db2)) {
    try {
      const sql = `
        SELECT m.*, ABS(bm25(memory_fts)) AS _bm25
        FROM agent_memories m
        JOIN memory_fts ON memory_fts.memory_id = m.memory_id
        WHERE memory_fts MATCH ?
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
    const lexical = maxBm25 > 0 ? (row._bm25 ?? 0) / maxBm25 : 0.5;
    const mem = rowToMemory(row);
    mem.score = decayScore(mem, lexical);
    return mem;
  });
}
function bumpAccess(db2, memoryIds) {
  if (memoryIds.length === 0) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => "?").join(",");
  db2.prepare(`
    UPDATE agent_memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${placeholders})
  `).run(now, ...memoryIds);
}
function insertMemory(db2, params) {
  const {
    agentId = "agent",
    taskContext,
    observation,
    importanceScore,
    label,
    tags = [],
    tagsCsv = "",
    references = [],
    supersedes = [],
    failureSignature = null,
    validFrom: vf,
    validTo: vt,
    workspacePath,
    repo: repoArg,
    ref: refArg,
    file: fileArg,
    fileTreeFingerprint = null,
    cwd
  } = params;
  const imp = Number(importanceScore);
  if (!Number.isInteger(imp) || imp < 1 || imp > 10) {
    throw new Error(`importanceScore must be 1\u201310, got ${String(importanceScore)}`);
  }
  const memoryId = "mem_" + randomUUID().replace(/-/g, "");
  const tagList = normalizeTags(tags, tagsCsv);
  const refList = normalizeReferences(references);
  const normalizedLabel = normalizeLabel(Array.isArray(label) ? label[0] : label);
  const createdAt = utcNow();
  const validFromVal = vf ?? createdAt;
  const memFile = normalizeFilePath(fileArg);
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const similar = findSimilarMemories(db2, `${taskContext} ${observation}`);
  const noveltyScore = Math.max(0, Math.min(1, 1 - (similar[0]?.similarity ?? 0)));
  const similarMemoryIds = similar.map((m) => m.memory_id);
  db2.prepare(`
    INSERT INTO agent_memories (
      memory_id, agent_id, task_context, observation, importance_score,
      label, tags_json, tags_text, references_json, workspace_path, repo, ref,
      file_tree_fingerprint, file, novelty_score, similar_memory_ids_json, created_at, updated_at,
      last_accessed_at, access_count, failure_signature, valid_from, valid_to
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)
  `).run(
    memoryId,
    agentId,
    taskContext,
    observation,
    imp,
    normalizedLabel,
    JSON.stringify(tagList),
    tagsText(tagList),
    JSON.stringify(refList),
    scope.workspace_path,
    scope.repo,
    scope.ref,
    fileTreeFingerprint,
    memFile,
    noveltyScore,
    JSON.stringify(similarMemoryIds),
    createdAt,
    createdAt,
    createdAt,
    failureSignature ?? null,
    validFromVal,
    vt ?? null
  );
  if (hasFts(db2)) {
    db2.prepare(
      "INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
    ).run(
      memoryId,
      taskContext,
      observation,
      ftsTermsForRow({
        tags_json: JSON.stringify(tagList),
        references_json: JSON.stringify(refList),
        label: normalizedLabel,
        file: memFile,
        failure_signature: failureSignature ?? null,
        workspace_path: scope.workspace_path,
        repo: scope.repo,
        ref: scope.ref
      })
    );
  }
  const superseded = [];
  for (const oldId of supersedes) {
    const r = db2.prepare(`
      UPDATE agent_memories
      SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
          valid_to = COALESCE(valid_to, ?), expired_at = ?
      WHERE memory_id = ? AND memory_id <> ?
    `).run(memoryId, createdAt, validFromVal, createdAt, oldId, memoryId);
    if (r.changes) superseded.push(oldId);
  }
  return {
    memoryId,
    memory: {
      memory_id: memoryId,
      agent_id: agentId,
      task_context: taskContext,
      observation,
      importance_score: imp,
      label: normalizedLabel,
      tags: tagList,
      references: refList,
      workspace_path: scope.workspace_path,
      repo: scope.repo,
      ref: scope.ref,
      file: memFile,
      novelty_score: noveltyScore,
      similar_memory_ids: similarMemoryIds,
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
    states: statesRaw,
    sort = "smart",
    globalOnly = false,
    asOf
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
    states
  );
  let scope = workspacePath;
  if (!globalOnly && scope) {
    scope = fillScope({ workspace_path: null }, scope).workspace_path ?? scope;
    memories = memories.filter((m) => !m.workspace_path || m.workspace_path === scope);
  }
  if (asOf) {
    const asOfDate = new Date(asOf);
    memories = memories.filter((m) => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }
  memories.sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  memories = memories.slice(0, limit);
  bumpAccess(db2, memories.map((m) => m.memory_id));
  return {
    count: memories.length,
    memories,
    mode: hasFts(db2) ? "lexical" : "fallback",
    sort,
    as_of: asOf ?? null,
    global_only: Boolean(globalOnly),
    states
  };
}
function mineWeakness(db2, params = {}) {
  const { minCount = 2, limit = 20, cwd } = params;
  const wsPath = params.workspacePath ?? (cwd ? fillScope({ workspace_path: null }, cwd).workspace_path : null);
  const conditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
  const bindParams = [];
  if (wsPath) {
    conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
    bindParams.push(wsPath);
  }
  if (params.agentId) {
    conditions.push("agent_id = ?");
    bindParams.push(params.agentId);
  }
  const rows = db2.prepare(`
    SELECT failure_signature,
           count(*) AS freq,
           avg(importance_score) AS avg_imp,
           count(*) * avg(importance_score) AS score,
           group_concat(memory_id, ',') AS ids,
           group_concat(DISTINCT label) AS labels
    FROM agent_memories
    WHERE ${conditions.join(" AND ")}
    GROUP BY failure_signature
    HAVING freq >= ?
    ORDER BY score DESC
    LIMIT ?
  `).all(...bindParams, minCount, limit);
  const clusters = rows.map((row) => {
    const ids = row.ids.split(",");
    const rep = db2.prepare(
      `SELECT observation FROM agent_memories WHERE memory_id IN (${ids.map(() => "?").join(",")})
       ORDER BY importance_score DESC LIMIT 1`
    ).get(...ids);
    return {
      failure_signature: row.failure_signature,
      count: row.freq,
      avg_importance: Math.round(row.avg_imp * 10) / 10,
      score: Math.round(row.score * 10) / 10,
      memory_ids: ids,
      representative: rep?.observation?.slice(0, 200) ?? "",
      labels: row.labels.split(",").filter(Boolean)
    };
  });
  const totals = db2.prepare(
    `SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
     FROM agent_memories WHERE failure_signature IS NOT NULL AND state = 'ACTIVE'`
  ).get();
  return { ok: true, clusters, total_signatures: totals.sigs, total_memories: totals.mems };
}

// src/refinements.ts
import { randomUUID as randomUUID2 } from "node:crypto";
function insertRefinement(db2, params) {
  const {
    agentId = "agent",
    reasoning,
    remember,
    quality = "good",
    state = "open",
    workspacePath,
    repo: repoArg,
    ref: refArg,
    files = [],
    cwd
  } = params;
  const refinementId = "ref_" + randomUUID2().replace(/-/g, "");
  const now = utcNow();
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  db2.prepare(`
    INSERT INTO refinements (
      refinement_id, agent_id, workspace_path, repo, ref,
      files_json, reasoning, remember, quality, state, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    refinementId,
    agentId,
    scope.workspace_path ?? process.cwd(),
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
    repo: repoArg,
    states: statesRaw,
    limit: limitRaw = 10,
    cwd
  } = params;
  const limit = Math.min(50, Math.max(1, Number(limitRaw) || 10));
  const states = statesRaw ?? ["open", "ongoing"];
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, repo: repoArg ?? null },
    cwd ?? process.cwd()
  );
  const queryParams = [...states];
  const stateFilter = `state IN (${states.map(() => "?").join(",")})`;
  let sql = `SELECT * FROM refinements WHERE ${stateFilter}`;
  if (scope.repo) {
    sql += " AND (repo = ? OR repo IS NULL)";
    queryParams.push(scope.repo);
  } else if (scope.workspace_path) {
    sql += " AND (workspace_path = ? OR workspace_path IS NULL)";
    queryParams.push(scope.workspace_path);
  }
  sql += ` ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC LIMIT ?`;
  queryParams.push(limit);
  const rows = db2.prepare(sql).all(...queryParams);
  const refinements = rows.map((r) => ({
    refinement_id: r.refinement_id,
    agent_id: r.agent_id,
    workspace_path: r.workspace_path,
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

// src/intents.ts
import { randomUUID as randomUUID3 } from "node:crypto";
import { resolve as resolve3 } from "node:path";
function preFlightIntent(db2, params) {
  const {
    agentId = "agent",
    workspacePath,
    rationale = "agent write operation",
    testPlan = "post-edit verification",
    targetFiles = [],
    lockType = "EXCLUSIVE",
    ttlMs = null
  } = params;
  const intentId = "intent_" + randomUUID3().replace(/-/g, "");
  const now = utcNow();
  const wsPath = workspacePath ?? process.cwd();
  const absFiles = targetFiles.map((f) => resolve3(f));
  const conflicts = [];
  for (const absPath of absFiles) {
    const existing = db2.prepare(`
      SELECT fl.*, ai.agent_id AS intent_agent_id FROM file_locks fl
      JOIN agent_intents ai ON ai.intent_id = fl.intent_id
      WHERE fl.file_path = ?
        AND ai.agent_id <> ?
        AND ai.status = 'ACTIVE'
        AND fl.lock_type = 'EXCLUSIVE'
        AND (fl.expires_at IS NULL OR fl.expires_at > ?)
    `).all(absPath, agentId, now);
    conflicts.push(...existing);
  }
  if (conflicts.length > 0) {
    return {
      ok: false,
      conflict: true,
      conflicts: conflicts.map((c) => ({
        file_path: c.file_path,
        lock_type: c.lock_type,
        agent_id: c.intent_agent_id ?? c.agent_id,
        acquired_at: c.acquired_at,
        expires_at: c.expires_at
      }))
    };
  }
  db2.prepare(`
    INSERT INTO agent_intents
      (intent_id, agent_id, rationale, test_plan, status, workspace_path, files_json, created_at, updated_at)
    VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)
  `).run(intentId, agentId, rationale, testPlan, wsPath, JSON.stringify(absFiles), now, now);
  const expiresAt = ttlMs ? new Date(Date.now() + ttlMs).toISOString().replace(/\.\d{3}Z$/, "Z") : null;
  const acquiredLocks = [];
  for (const absPath of absFiles) {
    const lockId = "lock_" + randomUUID3().replace(/-/g, "");
    db2.prepare(`
      INSERT OR REPLACE INTO file_locks
        (lock_id, file_path, intent_id, agent_id, lock_type, acquired_at, expires_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run(lockId, absPath, intentId, agentId, lockType, now, expiresAt);
    acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
  }
  return {
    ok: true,
    intent: {
      intent_id: intentId,
      agent_id: agentId,
      lock_type: lockType,
      workspace_path: wsPath,
      target_files: absFiles,
      locks: acquiredLocks.map((l) => ({
        lock_id: l.lock_id,
        file_path: l.file_path,
        lock_type: l.lock_type,
        agent_id: agentId,
        acquired_at: now,
        expires_at: l.expires_at
      })),
      status: "ACTIVE",
      created_at: now
    }
  };
}
function releaseFileLock(db2, params) {
  const {
    agentId = "agent",
    intentId = null,
    targetFiles = [],
    status: statusArg = "SUCCESS"
  } = params;
  const now = utcNow();
  const whereClauses = ["fl.agent_id = ?"];
  const whereParams = [agentId];
  if (intentId) {
    whereClauses.push("fl.intent_id = ?");
    whereParams.push(intentId);
  }
  const absFiles = targetFiles.map((f) => resolve3(f));
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => "?").join(",");
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }
  const where = whereClauses.join(" AND ");
  const locks = db2.prepare(
    `SELECT fl.lock_id, fl.intent_id, fl.file_path FROM file_locks fl WHERE ${where}`
  ).all(...whereParams);
  const deleteWhere = where.replace(/\bfl\./g, "");
  db2.prepare(`DELETE FROM file_locks WHERE ${deleteWhere}`).run(...whereParams);
  const intentIds = [.../* @__PURE__ */ new Set([
    ...intentId ? [intentId] : [],
    ...locks.map((l) => l.intent_id)
  ])];
  for (const iid of intentIds) {
    const remaining = db2.prepare("SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1").get(iid);
    if (!remaining) {
      db2.prepare(
        "UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND agent_id = ?"
      ).run(statusArg, now, iid, agentId);
    }
  }
  return {
    agent_id: agentId,
    status: statusArg,
    released: locks.length > 0 || Boolean(intentId),
    locks_released: locks.length,
    intent_ids: intentIds,
    updated_at: now
  };
}

// src/reflect.ts
import { resolve as resolve4 } from "node:path";
var VALID_OUTCOMES = ["worked", "partial", "failed"];
var NEXT_MSG = "refine-get \u2192 repo fixes for the next agent \xB7 mine-weakness \u2192 recurring failures \xB7 export-harness \u2192 preview harness improvements. A human merges.";
function normalizeScopePaths(paths = [], prefix) {
  return [...new Set(paths.filter(Boolean).map((p) => `${prefix}:${resolve4(p)}`))];
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
    failureSignature: failSig,
    importance: impArg,
    references = [],
    file,
    files = [],
    folders = [],
    validFrom,
    validTo,
    workspacePath,
    repo: repoArg,
    ref: refArg,
    cwd
  } = params;
  const resolvedOutcome = VALID_OUTCOMES.includes(outcome ?? "") ? outcome : "partial";
  const bits = [`[reflection:${resolvedOutcome}] ${task}`];
  if (worked) bits.push(`worked: ${worked}`);
  if (didntWork) bits.push(`didn't work: ${didntWork}`);
  if (fixHarness) bits.push(`harness fix: ${fixHarness}`);
  const narrative = bits.join(" | ");
  const observation = lesson ? bits.length > 1 ? `${lesson}  (${narrative})` : lesson : narrative;
  const importance = impArg != null ? Number(impArg) : REFLECTION_IMPORTANCE[resolvedOutcome] ?? 5;
  const tags = ["reflection", resolvedOutcome, ...fixHarness ? ["harness"] : []];
  const sig = failSig ?? (resolvedOutcome === "failed" && fixHarness ? "harness:reflection|outcome:failed" : null);
  const scopeReferences = [
    ...references,
    ...normalizeScopePaths(file ? [file] : [], "file"),
    ...normalizeScopePaths(files, "file"),
    ...normalizeScopePaths(folders, "dir")
  ];
  const { memoryId, similarMemoryIds, noveltyScore } = insertMemory(db2, {
    agentId,
    taskContext: task,
    observation,
    importanceScore: importance,
    label: "EXPERIENCE",
    // distinct label so reflections are filterable and excluded from briefings
    tags,
    references: scopeReferences,
    failureSignature: sig,
    validFrom,
    validTo,
    workspacePath,
    repo: repoArg,
    ref: refArg,
    file: file ?? files[0] ?? folders[0] ?? null,
    cwd
  });
  let refinementId = null;
  if (fixRepo) {
    const { refinementId: rid } = insertRefinement(db2, {
      agentId,
      reasoning: `Fix in repo (from ${resolvedOutcome} reflection): ${fixRepo}`,
      remember: fixRepo,
      quality: "bad",
      state: "open",
      workspacePath,
      repo: repoArg,
      ref: refArg,
      files: [...files, ...folders],
      cwd
    });
    refinementId = rid;
  }
  return {
    outcome: resolvedOutcome,
    learning_memory_id: memoryId,
    repo_fix_refinement_id: refinementId,
    harness_fix: Boolean(fixHarness),
    eval_failure_count: 0,
    eval_failure_ids: [],
    next: NEXT_MSG,
    novelty_score: noveltyScore,
    similar_memory_ids: similarMemoryIds
  };
}

// src/stubs.ts
function pruneStale(db2, _params = {}) {
  const now = utcNow();
  const expiredLocks = db2.prepare(`
    SELECT fl.lock_id, fl.intent_id
    FROM file_locks fl
    WHERE fl.expires_at IS NOT NULL AND fl.expires_at < ?
  `).all(now);
  if (expiredLocks.length === 0) {
    return { pruned_locks: 0, updated_intents: 0 };
  }
  db2.prepare(
    "DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?"
  ).run(now);
  const affectedIntentIds = [...new Set(expiredLocks.map((l) => l.intent_id))];
  let updatedIntents = 0;
  for (const iid of affectedIntentIds) {
    const remaining = db2.prepare("SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1").get(iid);
    if (!remaining) {
      const r = db2.prepare(
        "UPDATE agent_intents SET status = 'PENDING', updated_at = ? WHERE intent_id = ? AND status = 'ACTIVE'"
      ).run(now, iid);
      if (r.changes) updatedIntents++;
    }
  }
  return { pruned_locks: expiredLocks.length, updated_intents: updatedIntents };
}
function notifyGet(db2, params = {}) {
  const wsPath = params.workspace ?? null;
  const format = params.format ?? "json";
  const items = [];
  try {
    const conditions = [
      "state = 'ACTIVE'",
      "importance_score >= 6",
      "label IN ('GOTCHA','BUG','DECISION','IMPROVEMENT','ARCHITECTURE','SECURITY')"
    ];
    const bindParams = [];
    if (wsPath) {
      conditions.push("(workspace_path = ? OR workspace_path IS NULL)");
      bindParams.push(wsPath);
    }
    const memRows = db2.prepare(
      `SELECT memory_id, observation, label, importance_score
       FROM agent_memories
       WHERE ${conditions.join(" AND ")}
       ORDER BY importance_score DESC, last_accessed_at DESC
       LIMIT 3`
    ).all(...bindParams);
    for (const m of memRows) {
      items.push({
        kind: "memory",
        text: `${m.label}(${m.importance_score}): ${m.observation.slice(0, 120)}`,
        importance: m.importance_score
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
    const topWk = db2.prepare(
      `SELECT failure_signature, count(*) AS freq, avg(importance_score) AS avg_imp
       FROM agent_memories
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
    const refConds = ["state = 'open'"];
    const refParams = [];
    if (wsPath) {
      refConds.push("(workspace_path = ? OR workspace_path IS NULL)");
      refParams.push(wsPath);
    }
    const refRow = db2.prepare(
      `SELECT count(*) AS cnt FROM refinements WHERE ${refConds.join(" AND ")}`
    ).get(...refParams);
    const refCount = refRow?.cnt ?? 0;
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
function sessionCapture(_db, _params = {}) {
  process.stderr.write("[stub] session-capture: not yet implemented; skipping\n");
  return { ok: true, captured: false };
}
function waitForLock(_db, _params = {}) {
  process.stderr.write("[stub] wait-for-lock: not yet implemented; returning immediately\n");
  return { ok: true, waited_ms: 0, lock_free: true };
}
function digest(db2, params = {}) {
  const retentionDays = Number(params.retention_days ?? 90);
  const now = (/* @__PURE__ */ new Date()).toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 864e5).toISOString();
  if (params.dry_run) {
    const wouldArchive = db2.prepare(
      `SELECT COUNT(*) AS c FROM agent_memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
    ).get(now).c;
    const wouldPruneOld = db2.prepare(
      `SELECT COUNT(*) AS c FROM agent_memories WHERE state = 'SUPERSEDED' AND updated_at < ?`
    ).get(cutoff).c;
    const wouldPruneLocks = db2.prepare(
      `SELECT COUNT(*) AS c FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?`
    ).get(now).c;
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      fts_rebuilt: false,
      schema_version: 1,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks
    };
  }
  const archiveRes = db2.prepare(
    `UPDATE agent_memories
     SET state = 'SUPERSEDED', expired_at = ?
     WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
  ).run(now, now);
  const deleteRes = db2.prepare(
    `DELETE FROM agent_memories
     WHERE state = 'SUPERSEDED' AND updated_at < ?`
  ).run(cutoff);
  const { pruned_locks } = pruneStale(db2, {});
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
    fts_rebuilt: ftsRebuilt,
    schema_version: 1
  };
}

// src/verify.ts
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
  const rows = db2.prepare(
    `SELECT intent_id, agent_id, status, test_plan, rationale, workspace_path, files_json, created_at
     FROM agent_intents
     WHERE ${where.join(" AND ")}
     ORDER BY created_at ASC`
  ).all(...binds);
  const unverified = rows.map((r) => ({
    intent_id: r.intent_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    rationale: r.rationale,
    target_files: parseJsonList(r.files_json),
    workspace_path: r.workspace_path,
    created_at: r.created_at
  }));
  return { ok: true, unverified, count: unverified.length };
}
function markVerified(db2, params) {
  const { intentId, agentId = "agent" } = params;
  const status = params.status ?? "SUCCESS";
  if (!intentId) {
    return { ok: false, error: "--intent-id is required", intent_id: intentId };
  }
  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" \u2014 must be SUCCESS or FAILED`,
      intent_id: intentId
    };
  }
  const now = utcNow();
  const result = db2.prepare(
    "UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND agent_id = ? AND status = 'PENDING'"
  ).run(status, now, intentId, agentId);
  if (result.changes === 0) {
    const row = db2.prepare(
      "SELECT agent_id, status FROM agent_intents WHERE intent_id = ?"
    ).get(intentId);
    if (!row) {
      return {
        ok: false,
        error: `no intent found with intent_id=${intentId}`,
        intent_id: intentId
      };
    }
    if (row.agent_id !== agentId) {
      return {
        ok: false,
        error: `intent ${intentId} belongs to agent "${row.agent_id}", not "${agentId}"`,
        intent_id: intentId
      };
    }
    return {
      ok: false,
      error: `intent ${intentId} has status "${row.status}" \u2014 only PENDING intents can be verified`,
      intent_id: intentId
    };
  }
  return {
    ok: true,
    intent_id: intentId,
    status,
    updated_at: now
  };
}

// bin/awareness.ts
if (parseInt(process.version.slice(1), 10) < 22) {
  process.stderr.write(`awareness requires Node >=22 (got ${process.version})
`);
  process.exit(1);
}
var ARRAY_FLAGS = /* @__PURE__ */ new Set([
  "tag",
  "reference",
  "file",
  "target_file",
  "supersedes",
  "label",
  "state"
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
  const importanceScore = args2["importance_score"];
  if (!taskContext) die("--task-context is required");
  if (!observation) die("--observation is required");
  const imp = parseInt(String(importanceScore), 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die("--importance-score must be 1\u201310");
  const rawTag = args2["tag"];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawRef = args2["reference"];
  const references = Array.isArray(rawRef) ? rawRef : rawRef ? [String(rawRef)] : [];
  const rawSup = args2["supersedes"];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args2["label"];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? "");
  const { memory, superseded } = insertMemory(db2, {
    agentId,
    taskContext,
    observation,
    importanceScore: imp,
    label: normalizeLabel(label),
    tags,
    references,
    supersedes,
    failureSignature: args2["failure_signature"] ? String(args2["failure_signature"]) : null,
    validFrom: args2["valid_from"] ? String(args2["valid_from"]) : null,
    validTo: args2["valid_to"] ? String(args2["valid_to"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    repo: args2["repo"] ? String(args2["repo"]) : null,
    ref: args2["ref"] ? String(args2["ref"]) : null,
    file: args2["file"] ? String(args2["file"]) : null
  });
  return emit({ db_path: dbPath2, memory, superseded }, 0, opts2);
}
function cmdGetMemory(db2, args2, dbPath2, opts2) {
  const rawLabel = args2["label"];
  const labelArr = Array.isArray(rawLabel) ? rawLabel : rawLabel ? [String(rawLabel)] : void 0;
  const rawTag = args2["tag"];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawState = args2["state"];
  const states = rawState ? Array.isArray(rawState) ? rawState : [String(rawState)] : void 0;
  const result = getMemory(db2, {
    query: String(args2["query"] ?? ""),
    limit: parseInt(String(args2["limit"] ?? "3"), 10),
    minImportance: parseInt(String(args2["min_importance"] ?? "1"), 10),
    label: labelArr,
    tags,
    smart: args2["smart"] === true || args2["smart"] === "true",
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    states,
    sort: String(args2["sort"] ?? "smart"),
    globalOnly: Boolean(args2["global_only"]),
    asOf: args2["as_of"] ? String(args2["as_of"]) : null
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdRefineSet(db2, args2, dbPath2, opts2) {
  const reasoning = String(args2["reasoning"] ?? "");
  const remember = String(args2["remember"] ?? "");
  if (!reasoning) die("--reasoning is required");
  if (!remember) die("--remember is required");
  const rawState = args2["state"];
  const stateVal = Array.isArray(rawState) ? rawState[0] : String(rawState ?? "open");
  const rawFile = args2["file"];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];
  const { refinement } = insertRefinement(db2, {
    agentId: String(args2["agent_id"] ?? "agent"),
    reasoning,
    remember,
    quality: String(args2["quality"] ?? "good"),
    state: stateVal ?? "open",
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
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
    repo: args2["repo"] ? String(args2["repo"]) : null,
    states,
    limit: parseInt(String(args2["limit"] ?? "10"), 10)
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdReflect(db2, args2, dbPath2, opts2) {
  if (!args2["task"]) die("--task is required");
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
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
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
  const ttlMs = ttlMinutes != null ? ttlMinutes * 6e4 : ttlSeconds != null ? ttlSeconds * 1e3 : null;
  const result = preFlightIntent(db2, {
    agentId: String(args2["agent_id"] ?? "agent"),
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null,
    rationale: String(args2["rationale"] ?? "agent write operation"),
    testPlan: String(args2["test_plan"] ?? "post-edit verification"),
    targetFiles,
    lockType: String(args2["lock_type"] ?? "EXCLUSIVE"),
    ttlMs
  });
  if (!result.ok) return emit({ db_path: dbPath2, ...result }, 2, opts2);
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdAuditUnverified(db2, args2, dbPath2, opts2) {
  const result = auditUnverified(db2, {
    agentId: args2["agent_id"] ? String(args2["agent_id"]) : null,
    workspacePath: args2["workspace"] ? String(args2["workspace"]) : null
  });
  return emit({ db_path: dbPath2, ...result }, result.count > 0 ? 1 : 0, opts2);
}
function cmdVerify(db2, args2, dbPath2, opts2) {
  if (!args2["intent_id"]) return emit({ error: "--intent-id is required" }, 1, opts2);
  const statusArg = args2["status"] ? String(args2["status"]) : "SUCCESS";
  if (statusArg !== "SUCCESS" && statusArg !== "FAILED") {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts2);
  }
  const result = markVerified(db2, {
    intentId: String(args2["intent_id"]),
    agentId: String(args2["agent_id"] ?? "agent"),
    status: statusArg
  });
  return emit({ db_path: dbPath2, ...result }, result.ok ? 0 : 1, opts2);
}
function cmdReleaseFileLock(db2, args2, dbPath2, opts2) {
  const rawTarget = args2["target_file"] ?? args2["file"];
  const targetFiles = rawTarget ? Array.isArray(rawTarget) ? rawTarget : [String(rawTarget)] : [];
  if (!args2["intent_id"] && targetFiles.length === 0) {
    return emit({ error: "release-file-lock requires --intent-id or --target-file" }, 1, opts2);
  }
  const result = releaseFileLock(db2, {
    agentId: String(args2["agent_id"] ?? "agent"),
    intentId: args2["intent_id"] ? String(args2["intent_id"]) : null,
    targetFiles,
    status: String(args2["status"] ?? "SUCCESS")
  });
  return emit({ db_path: dbPath2, ...result }, 0, opts2);
}
function cmdStatus(db2, dbPath2, args2, opts2) {
  const now = utcNow();
  db2.prepare("DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?").run(now);
  const memCount = db2.prepare("SELECT COUNT(*) AS count FROM agent_memories").get().count;
  const memStates = Object.fromEntries(
    db2.prepare("SELECT state, COUNT(*) AS count FROM agent_memories GROUP BY state").all().map((r) => [r.state, r.count])
  );
  const memLabels = Object.fromEntries(
    db2.prepare("SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM agent_memories GROUP BY label").all().map((r) => [r.label, r.count])
  );
  const activeIntents = db2.prepare("SELECT COUNT(*) AS count FROM agent_intents WHERE status='ACTIVE'").get().count;
  const limit = Math.min(100, Math.max(1, parseInt(String(args2["limit"] ?? "20"), 10) || 20));
  const locks = db2.prepare(
    "SELECT file_path, intent_id, agent_id, lock_type, acquired_at, expires_at FROM file_locks ORDER BY acquired_at DESC LIMIT ?"
  ).all(limit);
  const openRefinements = db2.prepare(
    "SELECT COUNT(*) AS count FROM refinements WHERE state IN ('open','ongoing')"
  ).get().count;
  return emit({
    db_path: dbPath2,
    fts_enabled: hasFts(db2),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    active_intent_count: activeIntents,
    open_refinements: openRefinements,
    locks,
    workspace_path: args2["workspace"] ? String(args2["workspace"]) : null
  }, 0, opts2);
}
function cmdInit(db2, dbPath2, opts2) {
  const memCount = db2.prepare("SELECT COUNT(*) AS count FROM agent_memories").get().count;
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
    importanceScore: 7,
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

commands: tell-memory  get-memory  reflect  refine-set  refine-get
          pre-flight-intent  release-file-lock  status  init  self-test
          prune-stale-locks  audit-unverified  verify
          notify-get  session-capture  wait-for-lock

common options:
  --db <path>     Override DB path (default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3)
  --compact       Compact JSON output (or OCTOCODE_AWARENESS_COMPACT=1)

tell-memory:
  --agent-id <id>  --task-context <text>  --observation <text>
  --importance-score <1-10>  --label <LABEL>  [--tag <t>]...  [--reference <r>]...

get-memory:
  --query <text>  [--limit <n>]  [--min-importance <n>]  [--label <L>]  [--smart]

reflect:
  --agent-id <id>  --task <text>  --outcome worked|partial|failed
  [--lesson <text>]  [--worked <text>]  [--didnt-work <text>]
  [--fix-repo <text>]  [--fix-harness <text>]

pre-flight-intent:
  --agent-id <id>  [--workspace <path>]  [--target-file <path>]...  [--ttl-minutes <n>]

release-file-lock:
  --agent-id <id>  (--intent-id <id> | --target-file <path>)  [--status SUCCESS|PENDING|FAILED]

audit-unverified:
  [--agent-id <id>]  [--workspace <path>]
  exits 1 when unverified (PENDING) intents exist; exits 0 when clear

verify:
  --intent-id <id>  --agent-id <id>  [--status SUCCESS|FAILED (default SUCCESS)]
  marks a PENDING intent as verified; clears it from audit-unverified
`;
var rawArgv = process.argv.slice(2);
if (rawArgv.length === 0 || rawArgv.includes("--help") || rawArgv.includes("-h")) {
  process.stdout.write(HELP + "\n");
  process.exit(0);
}
var { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
var [command, ...rest] = filteredArgv;
var args = parseArgs(rest ?? []);
if (globalDb) args["db"] = globalDb;
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
    case "release-intent":
      exitCode = cmdReleaseFileLock(db, args, dbPath, opts);
      break;
    case "status":
      exitCode = cmdStatus(db, dbPath, args, opts);
      break;
    case "init":
      exitCode = cmdInit(db, dbPath, opts);
      break;
    case "prune-stale-locks":
      exitCode = emit({ db_path: dbPath, ...pruneStale(db, {}) }, 0, opts);
      break;
    case "audit-unverified":
      exitCode = cmdAuditUnverified(db, args, dbPath, opts);
      break;
    case "verify":
      exitCode = cmdVerify(db, args, dbPath, opts);
      break;
    case "notify-get": {
      const ngParams = {
        workspace: args["workspace"],
        format: args["format"] ?? "json",
        agent_id: args["agent_id"]
      };
      const ngResult = notifyGet(db, ngParams);
      if (ngParams.format === "hook" && ngResult.additionalContext) {
        exitCode = emit({ additionalContext: ngResult.additionalContext }, 0, opts);
      } else {
        exitCode = emit({ db_path: dbPath, ...ngResult }, 0, opts);
      }
      break;
    }
    case "session-capture":
      exitCode = emit({ db_path: dbPath, ...sessionCapture(db, {}) }, 0, opts);
      break;
    case "mine-weakness": {
      const mwParams = {
        agentId: args["agent_id"],
        workspacePath: args["workspace"],
        minCount: args["min_count"] ? Number(args["min_count"]) : void 0,
        limit: args["limit"] ? Number(args["limit"]) : void 0,
        cwd: args["cwd"]
      };
      exitCode = emit({ db_path: dbPath, ...mineWeakness(db, mwParams) }, 0, opts);
      break;
    }
    case "digest": {
      const retDays = args["retention_days"] ? Number(args["retention_days"]) : void 0;
      exitCode = emit({ db_path: dbPath, ...digest(db, retDays !== void 0 ? { retention_days: retDays } : {}) }, 0, opts);
      break;
    }
    case "wait-for-lock":
      exitCode = emit({ db_path: dbPath, ...waitForLock(db, {}) }, 0, opts);
      break;
    default:
      exitCode = emit({ error: `unknown command: ${command}. Run --help for usage.` }, 1, opts);
  }
} catch (err) {
  exitCode = emit({
    error: err instanceof Error ? err.message : String(err),
    stack: err instanceof Error ? err.stack : void 0
  }, 1, opts);
}
process.exit(exitCode);
//# sourceMappingURL=awareness.js.map
