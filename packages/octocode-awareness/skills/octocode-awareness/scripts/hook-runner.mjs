#!/usr/bin/env node

// src/db.ts
import { DatabaseSync } from "node:sqlite";
import { mkdirSync } from "node:fs";
import { join, resolve as resolve2, dirname } from "node:path";
import { homedir, platform } from "node:os";

// src/helpers.ts
import { resolve } from "node:path";
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

// src/db.ts
var REFERENCES_INDEX_VERSION = "1";
var REFINEMENT_QUALITY_SCHEMA_VERSION = "2";
var FTS_INDEX_VERSION = "3";
var DEFAULT_DB_NAME = "awareness.sqlite3";
var MEMORY_HOME_ENV = "OCTOCODE_MEMORY_HOME";
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
function connectDb(dbPath) {
  mkdirSync(dirname(dbPath), { recursive: true });
  const db2 = new DatabaseSync(dbPath);
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

    -- ARCH-5: Agent identity registry \u2014 maps opaque agentIds to human-readable names.
    -- Separate from agent_memories so the mapping persists even when memories are pruned.
    -- ON CONFLICT logic in agents.ts ensures a non-empty name is never overwritten by ''.
    CREATE TABLE IF NOT EXISTS agent_identities (
      agent_id       TEXT PRIMARY KEY,
      agent_name     TEXT NOT NULL DEFAULT '',
      workspace_path TEXT,
      context        TEXT,   -- 'pi' | 'cursor' | 'claude-code' | etc
      registered_at  TEXT NOT NULL,
      last_seen_at   TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_agent_identities_workspace ON agent_identities(workspace_path);
    CREATE INDEX IF NOT EXISTS idx_agent_identities_last_seen ON agent_identities(last_seen_at DESC);

    CREATE INDEX IF NOT EXISTS idx_agent_memories_importance ON agent_memories(importance_score);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_created_at ON agent_memories(created_at);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_state ON agent_memories(state);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_label ON agent_memories(label);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_file ON agent_memories(file);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_failure_sig ON agent_memories(failure_signature);
    -- DB-1: workspace_path and tags_text used in nearly every scope filter \u2014 previously unindexed
    CREATE INDEX IF NOT EXISTS idx_agent_memories_workspace_path ON agent_memories(workspace_path);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_tags_text ON agent_memories(tags_text);
    CREATE INDEX IF NOT EXISTS idx_file_locks_file_path ON file_locks(file_path);
    CREATE INDEX IF NOT EXISTS idx_file_locks_agent_id ON file_locks(agent_id);
    CREATE INDEX IF NOT EXISTS idx_file_locks_acquired_at ON file_locks(acquired_at);
    CREATE INDEX IF NOT EXISTS idx_file_locks_expires_at ON file_locks(expires_at);
    CREATE INDEX IF NOT EXISTS idx_refinements_state ON refinements(state);
    CREATE INDEX IF NOT EXISTS idx_refinements_repo ON refinements(repo);
    -- DB-2: notifications table had zero indexes; all inbox queries were full table scans
    CREATE INDEX IF NOT EXISTS idx_notifications_status ON notifications(status);
    CREATE INDEX IF NOT EXISTS idx_notifications_to_agent ON notifications(to_agent);
    CREATE INDEX IF NOT EXISTS idx_notifications_workspace_path ON notifications(workspace_path);
    CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at);
    -- Composite for getRefinements ORDER BY CASE state ... , updated_at DESC
    CREATE INDEX IF NOT EXISTS idx_refinements_state_updated ON refinements(state, updated_at DESC);

    -- Critical missing indexes (verified from production DB) -------------------
    -- agent_intents: status-only scan is a full table scan with 1679 rows in prod
    CREATE INDEX IF NOT EXISTS idx_agent_intents_status ON agent_intents(status);
    CREATE INDEX IF NOT EXISTS idx_agent_intents_agent_status ON agent_intents(agent_id, status);
    CREATE INDEX IF NOT EXISTS idx_agent_intents_workspace ON agent_intents(workspace_path);
    -- agent_memories: composite scope index covers (workspace_path, repo, ref) at once
    CREATE INDEX IF NOT EXISTS idx_agent_memories_scope ON agent_memories(workspace_path, repo, ref);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_repo_ref ON agent_memories(repo, ref);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_valid ON agent_memories(valid_from, valid_to);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_embedding_model ON agent_memories(embedding_model);
    -- file_locks: session-based release queries
    CREATE INDEX IF NOT EXISTS idx_file_locks_session_id ON file_locks(session_id);
    -- notifications: thread and to_agent inbox
    CREATE INDEX IF NOT EXISTS idx_notifications_thread ON notifications(thread_id);
    -- Deduplicate idx_notifications_to_agent; keep the shorter alias too
    CREATE INDEX IF NOT EXISTS idx_notifications_to ON notifications(to_agent);
    -- memory_references: cover both column name spellings from different migration versions
    CREATE INDEX IF NOT EXISTS idx_memory_references_reference ON memory_references(reference);
  `);
  ensureMemoryColumns(db2);
  ensureIntentColumns(db2);
  ensureRefinementQualitySchema(db2);
  ensureMemoryReferencesVersion(db2);
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
  if (!cols.has("session_id")) {
    db2.exec("ALTER TABLE agent_intents ADD COLUMN session_id TEXT");
  }
  const lockCols = tableColumns(db2, "file_locks");
  if (!lockCols.has("session_id")) {
    db2.exec("ALTER TABLE file_locks ADD COLUMN session_id TEXT");
  }
  db2.exec("CREATE INDEX IF NOT EXISTS idx_file_locks_session_id ON file_locks(session_id)");
}
function rewriteLegacyHandoffRefinements(db2) {
  db2.prepare(
    "UPDATE refinements SET quality = 'handoff', updated_at = COALESCE(updated_at, created_at) WHERE quality <> 'handoff' AND remember LIKE 'Review session handoff%'"
  ).run();
}
function ensureRefinementQualitySchema(db2) {
  const row = db2.prepare(
    "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'refinements'"
  ).get();
  if (!row?.sql) return;
  if (!row.sql.includes("'handoff'")) {
    db2.exec("ALTER TABLE refinements RENAME TO refinements_old_quality_migration");
    db2.exec(`
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
  rewriteLegacyHandoffRefinements(db2);
  db2.prepare("INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('refinement_quality_schema_version', ?)").run(REFINEMENT_QUALITY_SCHEMA_VERSION);
  db2.exec(`
    CREATE INDEX IF NOT EXISTS idx_refinements_state ON refinements(state);
    CREATE INDEX IF NOT EXISTS idx_refinements_repo ON refinements(repo);
  `);
}
function hasFts(db2) {
  const row = db2.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_fts'"
  ).get();
  return Boolean(row);
}
function ftsTermsForRow(row) {
  const tags = parseJsonList(row.tags_json);
  const label = (row.label ?? "OTHER").toLowerCase();
  return [...tags, label].filter(Boolean).join(" ");
}
function referenceKind(reference) {
  if (/^https?:\/\//.test(reference)) return "url";
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.\-]*):/);
  return m ? m[1].toLowerCase() : "other";
}
function replaceMemoryReferences(db2, memoryId, references) {
  db2.prepare("DELETE FROM memory_references WHERE memory_id = ?").run(memoryId);
  const insert = db2.prepare(
    "INSERT OR REPLACE INTO memory_references(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)"
  );
  references.forEach((ref, i) => insert.run(memoryId, ref, referenceKind(ref), i));
}
function backfillMemoryReferences(db2) {
  const rows = db2.prepare("SELECT memory_id, references_json FROM agent_memories").all();
  for (const row of rows) {
    const refs = parseJsonList(row.references_json);
    if (refs.length > 0) replaceMemoryReferences(db2, row.memory_id, refs);
  }
}
function ensureMemoryReferencesVersion(db2) {
  try {
    const row = db2.prepare("SELECT value FROM awareness_meta WHERE key='memory_references_version'").get();
    if (row?.value === REFERENCES_INDEX_VERSION) return;
    backfillMemoryReferences(db2);
    db2.prepare("INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('memory_references_version', ?)").run(REFERENCES_INDEX_VERSION);
  } catch {
  }
}
function rebuildFts(db2) {
  db2.exec("DELETE FROM memory_fts");
  const rows = db2.prepare(
    "SELECT memory_id, task_context, observation, tags_json, label FROM agent_memories"
  ).all();
  const insert = db2.prepare(
    "INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)"
  );
  for (const row of rows) {
    insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
  }
}
function evictExpiredLocks(db2) {
  db2.prepare("DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at <= ?").run(utcNow());
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

// src/intents.ts
import { randomUUID } from "node:crypto";
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
    agentId: agentId2 = "agent",
    sessionId = null,
    workspacePath,
    rationale = "agent write operation",
    testPlan = "post-edit verification",
    targetFiles = [],
    lockType = "EXCLUSIVE",
    ttlMs = MAX_LOCK_TTL_MS
  } = params;
  const intentId = "intent_" + randomUUID().replace(/-/g, "");
  const now = utcNow();
  const wsPath = workspaceRoot(workspacePath);
  const absFiles = resolveTargetFiles(targetFiles, wsPath);
  evictExpiredLocks(db2);
  const conflicts = [];
  for (const absPath of absFiles) {
    const conflictMode = lockType === "SHARED" ? "fl.lock_type = 'EXCLUSIVE'" : "1 = 1";
    const existing = db2.prepare(`
      SELECT fl.*, ai.agent_id AS intent_agent_id FROM file_locks fl
      JOIN agent_intents ai ON ai.intent_id = fl.intent_id
      WHERE fl.file_path = ?
        AND ai.agent_id <> ?
        AND ai.status = 'ACTIVE'
        AND ${conflictMode}
        AND (fl.expires_at IS NULL OR fl.expires_at > ?)
    `).all(absPath, agentId2, now);
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
      (intent_id, agent_id, session_id, rationale, test_plan, status, workspace_path, files_json, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)
  `).run(intentId, agentId2, sessionId, rationale, testPlan, wsPath, JSON.stringify(absFiles), now, now);
  const expiresAt = expiresAtFromNow(ttlMs);
  const acquiredLocks = [];
  for (const absPath of absFiles) {
    const lockId = "lock_" + randomUUID().replace(/-/g, "");
    db2.prepare(`
      INSERT OR REPLACE INTO file_locks
        (lock_id, file_path, intent_id, agent_id, session_id, lock_type, acquired_at, expires_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `).run(lockId, absPath, intentId, agentId2, sessionId, lockType, now, expiresAt);
    acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
  }
  return {
    ok: true,
    intent: {
      intent_id: intentId,
      agent_id: agentId2,
      session_id: sessionId,
      lock_type: lockType,
      workspace_path: wsPath,
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
}
function releaseFileLock(db2, params) {
  const {
    agentId: agentId2 = "agent",
    sessionId = null,
    workspacePath = null,
    intentId = null,
    targetFiles = [],
    status: statusArg = "SUCCESS",
    verified = false,
    verifiedNote
  } = params;
  const requestedSuccessWithoutVerification = statusArg === "SUCCESS" && !verified;
  const effectiveStatus = verified ? "SUCCESS" : requestedSuccessWithoutVerification ? "PENDING" : statusArg;
  const now = utcNow();
  const whereClauses = ["fl.agent_id = ?"];
  const whereParams = [agentId2];
  if (sessionId) {
    whereClauses.push("fl.session_id = ?");
    whereParams.push(sessionId);
  }
  if (intentId) {
    whereClauses.push("fl.intent_id = ?");
    whereParams.push(intentId);
  }
  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => "?").join(",");
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }
  const where = whereClauses.join(" AND ");
  const locks = db2.prepare(
    `SELECT fl.lock_id, fl.intent_id, fl.file_path FROM file_locks fl WHERE ${where}`
  ).all(...whereParams);
  const deleteClauses = ["agent_id = ?"];
  const deleteParams = [agentId2];
  if (sessionId) {
    deleteClauses.push("session_id = ?");
    deleteParams.push(sessionId);
  }
  if (intentId) {
    deleteClauses.push("intent_id = ?");
    deleteParams.push(intentId);
  }
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => "?").join(",");
    deleteClauses.push(`file_path IN (${ph})`);
    deleteParams.push(...absFiles);
  }
  db2.prepare(`DELETE FROM file_locks WHERE ${deleteClauses.join(" AND ")}`).run(...deleteParams);
  const intentIds = [.../* @__PURE__ */ new Set([
    ...intentId ? [intentId] : [],
    ...locks.map((l) => l.intent_id)
  ])];
  for (const iid of intentIds) {
    const remaining = db2.prepare("SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1").get(iid);
    if (!remaining) {
      db2.prepare(
        "UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND agent_id = ?"
      ).run(effectiveStatus, now, iid, agentId2);
      if (verified && verifiedNote) {
        try {
          db2.prepare(
            `INSERT INTO intent_events(event_id, intent_id, agent_id, event_type, message, created_at)
             VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
          ).run("evt_" + randomUUID().replace(/-/g, ""), iid, agentId2, verifiedNote, now);
        } catch {
        }
      }
    }
  }
  return {
    agent_id: agentId2,
    status: effectiveStatus,
    released: locks.length > 0 || Boolean(intentId),
    locks_released: locks.length,
    intent_ids: intentIds,
    updated_at: now,
    ...requestedSuccessWithoutVerification ? { unverifiedConclusion: "SUCCESS requested without --verified; stored as PENDING until verify records the test result." } : {}
  };
}

// src/verify.ts
import { randomUUID as randomUUID2 } from "node:crypto";
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
  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db2.prepare(
        "UPDATE agent_intents SET status = 'FAILED', updated_at = ? WHERE intent_id = ? AND status = 'PENDING'"
      ).run(now, intent.intent_id);
      try {
        db2.prepare(
          `INSERT INTO intent_events(event_id, intent_id, agent_id, event_type, message, created_at)
           VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`
        ).run("evt_" + randomUUID2().replace(/-/g, ""), intent.intent_id, intent.agent_id, now);
      } catch {
      }
    }
  }
  const staleActive = [];
  try {
    const nowIso = utcNow();
    const staleWhere = [
      "ai.status = 'ACTIVE'",
      // No live lock remains: all locks either deleted or expired
      `NOT EXISTS (
        SELECT 1 FROM file_locks fl
        WHERE fl.intent_id = ai.intent_id
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
    const staleRows = db2.prepare(
      `SELECT ai.intent_id, ai.agent_id, ai.rationale, ai.workspace_path, ai.files_json, ai.created_at
       FROM agent_intents ai
       WHERE ${staleWhere.join(" AND ")}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds);
    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        intent_id: r.intent_id,
        agent_id: r.agent_id,
        status: "ACTIVE",
        rationale: r.rationale,
        target_files: parseJsonList(r.files_json),
        workspace_path: r.workspace_path,
        created_at: r.created_at,
        age_hours: Math.round(ageMs / 36e5 * 10) / 10
      });
    }
  } catch {
  }
  const total = unverified.length + staleActive.length;
  return { ok: true, unverified, stale_active: staleActive, count: total };
}

// src/maintenance.ts
import { spawnSync as spawnSync2 } from "node:child_process";
import { randomUUID as randomUUID4 } from "node:crypto";

// src/git.ts
import { spawnSync } from "node:child_process";
import { basename } from "node:path";
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

// src/notifications.ts
import { randomUUID as randomUUID3 } from "node:crypto";
function rowToNotification(r) {
  return {
    notification_id: r.notification_id,
    workspace_path: r.workspace_path,
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
    in_reply_to: r.in_reply_to,
    importance: r.importance,
    status: r.status,
    created_at: r.created_at
  };
}
function getNotifications(db2, params) {
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
    { workspace_path: params.workspacePath ?? null, repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd()
  );
  const where = [];
  const binds = [];
  if (scope.workspace_path) {
    where.push("(n.workspace_path = ? OR n.workspace_path IS NULL)");
    binds.push(scope.workspace_path);
  }
  if (threadId) {
    where.push("n.thread_id = ?");
    binds.push(threadId);
  } else {
    where.push("(n.to_agent IS NULL OR n.to_agent = ?)");
    binds.push(agentId2);
    if (unreadOnly) {
      where.push("n.status = 'open'");
      where.push("nr.notification_id IS NULL");
    }
  }
  if (kinds.length > 0) {
    where.push(`n.kind IN (${kinds.map(() => "?").join(",")})`);
    binds.push(...kinds);
  }
  const whereClause = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
  const joinClause = unreadOnly && !threadId ? `LEFT JOIN notification_reads nr ON nr.notification_id = n.notification_id AND nr.agent_id = ?` : "";
  const allBinds = unreadOnly && !threadId ? [agentId2, ...binds] : binds;
  const sql = `
    SELECT n.* FROM notifications n
    ${joinClause}
    ${whereClause}
    ORDER BY n.created_at DESC
    LIMIT ?
  `;
  const rows = db2.prepare(sql).all(...allBinds, limit);
  const notifications = rows.map(rowToNotification);
  if (markRead && notifications.length > 0) {
    const now = utcNow();
    const insertRead = db2.prepare(
      "INSERT OR IGNORE INTO notification_reads(notification_id, agent_id, read_at) VALUES (?, ?, ?)"
    );
    for (const n of notifications) {
      insertRead.run(n.notification_id, agentId2, now);
    }
  }
  return { count: notifications.length, notifications, unread_only: unreadOnly };
}

// src/maintenance.ts
function pruneStale(db2, params = {}) {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) : params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const now = utcNow();
  const ageCutoff = olderThanMinutes != null ? new Date(Date.now() - olderThanMinutes * 6e4).toISOString() : null;
  if (dryRun) {
    let count = 0;
    try {
      const row = db2.prepare(
        `SELECT COUNT(*) AS c FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?`
      ).get(now);
      count += row.c;
      if (ageCutoff) {
        const row2 = db2.prepare(
          `SELECT COUNT(*) AS c FROM file_locks WHERE acquired_at < ? AND (expires_at IS NULL OR expires_at >= ?)`
        ).get(ageCutoff, now);
        count += row2.c;
      }
    } catch {
    }
    return { pruned_locks: 0, updated_intents: 0, dry_run: true, would_prune: count };
  }
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
function openRefinementCount(db2, params = {}) {
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, repo: params.repo ?? null },
    params.cwd ?? process.cwd()
  );
  const queryParams = [];
  let sql = "SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";
  if (!params.includeHandoffs) sql += " AND quality <> 'handoff'";
  if (scope.repo) {
    sql += " AND (repo = ? OR repo IS NULL)";
    queryParams.push(scope.repo);
  } else if (scope.workspace_path) {
    sql += " AND (workspace_path = ? OR workspace_path IS NULL)";
    queryParams.push(scope.workspace_path);
  }
  return db2.prepare(sql).get(...queryParams).c;
}
var BRIEFING_LABELS = ["GOTCHA", "BUG", "DECISION", "IMPROVEMENT", "ARCHITECTURE", "SECURITY"];
function notifyGet(db2, params = {}) {
  const wsPath = params.workspace ?? null;
  const format = params.format ?? "json";
  const agentId2 = String(params.agent_id ?? params.agentId ?? "agent");
  const notifyCwd = wsPath ?? params.cwd ?? process.cwd();
  const items = [];
  try {
    const inbox = getNotifications(db2, {
      agentId: agentId2,
      workspacePath: wsPath,
      unreadOnly: true,
      markRead: false,
      limit: 5,
      cwd: notifyCwd
    });
    for (const n of inbox.notifications) {
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
    const conditions = [
      "state = 'ACTIVE'",
      "importance_score >= 6",
      `label IN (${BRIEFING_LABELS.map(() => "?").join(",")})`
    ];
    const bindParams = [...BRIEFING_LABELS];
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
    const refCount = openRefinementCount(db2, { workspacePath: wsPath, cwd: process.cwd() });
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
  const agentId2 = String(params.agent_id ?? params.agentId ?? "agent");
  const reason = params.reason ? String(params.reason) : null;
  const scope = fillScope(
    {
      workspace_path: params.workspace ?? params.workspace_path ?? params.workspacePath,
      repo: params.repo ?? null,
      ref: params.ref ?? null
    },
    params.cwd ?? process.cwd()
  );
  const workspacePath = scope.workspace_path ?? process.cwd();
  const intentRows = db2.prepare(
    `SELECT intent_id, rationale, test_plan, status, files_json, created_at, updated_at
     FROM agent_intents
     WHERE agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path = ? OR workspace_path IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId2, workspacePath);
  const files = [...new Set(intentRows.flatMap((row) => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeIntents = intentRows.filter((row) => row.status === "ACTIVE").length;
  const pendingIntents = intentRows.filter((row) => row.status === "PENDING").length;
  if (intentRows.length === 0 && dirtyFiles.length === 0) {
    return {
      ok: true,
      captured: false,
      refinement_id: null,
      pending_intents: 0,
      active_intents: 0,
      files: [],
      dirty_files: [],
      reason
    };
  }
  const now = utcNow();
  const refinementId = "ref_" + randomUUID4().replace(/-/g, "");
  const capturedFiles = [.../* @__PURE__ */ new Set([...files, ...dirtyFiles])];
  const statusSummary = intentRows.map((row) => {
    const rowFiles = parseJsonList(row.files_json);
    const fileSuffix = rowFiles.length > 0 ? ` files=${rowFiles.join(", ")}` : "";
    return `${row.status} ${row.intent_id}: ${row.rationale}; verify=${row.test_plan}${fileSuffix}`;
  });
  const reasoning = [
    `Session capture for ${agentId2}${reason ? ` (${reason})` : ""}.`,
    `Unresolved intents: ${intentRows.length} (${activeIntents} active, ${pendingIntents} pending).`,
    dirtyFiles.length > 0 ? `Dirty files: ${dirtyFiles.join(", ")}.` : null,
    statusSummary.length > 0 ? `Intent details: ${statusSummary.join(" | ")}` : null
  ].filter(Boolean).join(" ");
  const remember = [
    `Review session handoff for ${agentId2}: ${activeIntents} active and ${pendingIntents} pending intents remain.`,
    capturedFiles.length > 0 ? `Touched files: ${capturedFiles.join(", ")}.` : null,
    dirtyFiles.length > 0 ? "Check dirty git state before continuing." : null,
    pendingIntents > 0 ? "Run the recorded verification before claiming completion." : null
  ].filter(Boolean).join(" ");
  db2.prepare(
    `INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, repo, ref,
       files_json, reasoning, remember, quality, state, created_at, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'handoff', 'open', ?, ?)`
  ).run(
    refinementId,
    agentId2,
    workspacePath,
    scope.repo,
    scope.ref,
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
    pending_intents: pendingIntents,
    active_intents: activeIntents,
    files: capturedFiles,
    dirty_files: dirtyFiles,
    reason
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
      `SELECT COUNT(*) AS c FROM agent_memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
    ).get(now).c;
    const wouldPruneOld = db2.prepare(
      `SELECT COUNT(*) AS c FROM agent_memories WHERE state = 'SUPERSEDED' AND updated_at < ?`
    ).get(cutoff).c;
    const wouldPruneLocks = db2.prepare(
      `SELECT COUNT(*) AS c FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?`
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
    `UPDATE agent_memories
     SET state = 'SUPERSEDED', expired_at = ?
     WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
  ).run(now, now);
  const deleteRes = db2.prepare(
    `DELETE FROM agent_memories
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

// src/pi-hooks.ts
import path from "node:path";
import { randomUUID as randomUUID5 } from "node:crypto";
var _sessionStartupToken = randomUUID5().slice(0, 8);
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
function extractPiWriteTargetPaths(toolName, input = {}) {
  const normalizedToolName = String(toolName ?? "").toLowerCase();
  const isWriteTool = ["write", "edit", "multi_edit", "multiedit", "notebookedit", "notebook_edit"].includes(normalizedToolName);
  const payload = objectOrEmpty(input);
  const command2 = payload.command;
  if (!isWriteTool && typeof command2 !== "string") return [];
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
var command = process.argv[2] ?? "help";
function readStdin() {
  return new Promise((resolve4) => {
    let raw = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      raw += chunk;
    });
    process.stdin.on("end", () => resolve4(raw));
    process.stdin.on("error", () => resolve4(raw));
  });
}
function parsePayload(raw) {
  try {
    const parsed = JSON.parse(raw || "{}");
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}
function objectOrEmpty2(value) {
  return value && typeof value === "object" ? value : {};
}
function payloadInput(payload) {
  return objectOrEmpty2(payload.tool_input ?? payload.input ?? payload.args ?? payload);
}
function agentId(payload) {
  return process.env.OCTOCODE_AGENT_ID || String(payload.session_id ?? payload.sessionId ?? payload.agent_id ?? payload.agentId ?? "claude-agent");
}
function workspace(payload) {
  const value = payload.cwd ?? payload.workspace ?? payload.workspacePath;
  return typeof value === "string" && value.trim() ? value.trim() : null;
}
function hookReason(payload) {
  return typeof payload.reason === "string" ? payload.reason : "";
}
function isStopHookActive(payload) {
  return Boolean(payload.stop_hook_active);
}
function extractFiles(payload) {
  const input = payloadInput(payload);
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? input.tool_name ?? input.toolName ?? "";
  return extractPiWriteTargetPaths(toolName, input);
}
function resolveHookPath(file, cwd = process.cwd()) {
  return file.startsWith("/") ? file : `${cwd}/${file}`;
}
function isInsidePath(candidate, root) {
  const normalizedRoot = root.endsWith("/") ? root : `${root}/`;
  return candidate === root || candidate.startsWith(normalizedRoot);
}
function db() {
  return connectDb(resolveDbPath(null));
}
function workspaceArgs(payload) {
  const ws = workspace(payload);
  return ws ? { workspacePath: ws } : {};
}
async function runPreEdit(payload) {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  try {
    const result = preFlightIntent(db(), {
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? process.cwd(),
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
    releaseFileLock(db(), {
      agentId: agentId(payload),
      targetFiles: files,
      status: "PENDING"
    });
  } catch {
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
  if (process.env.OCTOCODE_HARNESS_BRANCH_OK !== "1") {
    console.error("octocode-awareness: harness self-fix is branch-only. Create a dedicated branch first, or set OCTOCODE_HARNESS_BRANCH_OK=1. Edit blocked.");
    return 2;
  }
  return 0;
}
async function runStopVerify(payload) {
  if (process.env.OCTOCODE_NO_VERIFY_GATE === "1" || isStopHookActive(payload)) return 0;
  try {
    const report = auditUnverified(db(), { agentId: agentId(payload), ...workspaceArgs(payload) });
    if (report.count > 0) {
      const plans = report.unverified.map((u) => `${u.status}:${u.intent_id}: ${u.test_plan}`).join("; ");
      console.error(`octocode-awareness: concluding with unverified work. Pending: ${plans}`);
      return 2;
    }
  } catch {
  }
  return 0;
}
function maybeRunDigest(payload) {
  if (process.env.OCTOCODE_NO_DIGEST === "1") return;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 36e5 : 4 * 36e5;
  const memoryHome2 = process.env.OCTOCODE_MEMORY_HOME || `${process.env.HOME ?? ""}/.octocode/memory`;
  const markerKey = "__octocode_last_digest_epoch_ms";
  try {
    const database = db();
    database.exec("CREATE TABLE IF NOT EXISTS awareness_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
    const row = database.prepare("SELECT value FROM awareness_meta WHERE key = ?").get(markerKey);
    const last = Number(row?.value ?? 0);
    const now = Date.now();
    if (!last || now - last >= intervalMs) {
      database.prepare("INSERT OR REPLACE INTO awareness_meta (key, value) VALUES (?, ?)").run(markerKey, String(now));
      digest(database, { workspace: workspace(payload), memoryHome: memoryHome2 });
    }
  } catch {
  }
}
async function runNotifyDeliver(payload) {
  if (process.env.OCTOCODE_NO_NOTIFY === "1") return 0;
  maybeRunDigest(payload);
  try {
    const result = notifyGet(db(), {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? void 0,
      format: "hook"
    });
    if (result.additionalContext) {
      process.stdout.write(JSON.stringify({ additionalContext: result.additionalContext }) + "\n");
    }
  } catch {
  }
  return 0;
}
async function runSessionEnd(payload) {
  if (process.env.OCTOCODE_NO_SESSION_CAPTURE === "1" || hookReason(payload) === "clear") return 0;
  try {
    sessionCapture(db(), {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? void 0,
      reason: hookReason(payload) || void 0
    });
  } catch {
  }
  return 0;
}
async function main() {
  if (command === "help" || command === "--help" || command === "-h") {
    process.stdout.write("usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n");
    return 0;
  }
  const payload = parsePayload(await readStdin());
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
process.exitCode = await main();
//# sourceMappingURL=hook-runner.js.map
