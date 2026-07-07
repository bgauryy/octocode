#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (w) => {
  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;
  console.error(w?.stack ?? String(w));
});

// bin/hook-runner.ts
import { spawnSync as spawnSync3 } from "node:child_process";
import { mkdirSync as mkdirSync2, readFileSync, writeFileSync } from "node:fs";
import { join as join2 } from "node:path";

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
function normalizeArtifact(value) {
  if (value == null) return null;
  const cleaned = String(value).trim().slice(0, 256);
  return cleaned.length > 0 ? cleaned : null;
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
function connectDb(dbPath) {
  mkdirSync(dirname(dbPath), { recursive: true });
  const db2 = new DatabaseSync(dbPath);
  db2.exec("PRAGMA foreign_keys = ON");
  db2.exec("PRAGMA busy_timeout = 5000");
  db2.exec("PRAGMA journal_mode = WAL");
  initDb(db2);
  _db = db2;
  return db2;
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
function initDb(db2) {
  db2.exec(SCHEMA_DDL);
  migrateExistingTables(db2);
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
function evictExpiredLocks(db2) {
  db2.prepare("DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?").run(utcNow());
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
    artifact: artifact2,
    rationale = "agent write operation",
    testPlan = "post-edit verification",
    targetFiles = [],
    lockType = "EXCLUSIVE",
    ttlMs = MAX_LOCK_TTL_MS
  } = params;
  const taskId = "task_" + randomUUID().replace(/-/g, "");
  const now = utcNow();
  const wsPath = workspaceRoot(workspacePath);
  const artifactScope = normalizeArtifact(artifact2);
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
      `).all(absPath, agentId2, now);
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
      ).run(sessionId, agentId2, wsPath, artifactScope, now);
    }
    db2.prepare(`
      INSERT INTO tasks
        (task_id, agent_id, session_id, rationale, test_plan, status, workspace_path, artifact, files_json, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?)
    `).run(taskId, agentId2, sessionId, rationale, testPlan, wsPath, artifactScope, JSON.stringify(absFiles), now, now);
    const expiresAt = expiresAtFromNow(ttlMs);
    const acquiredLocks = [];
    for (const absPath of absFiles) {
      const lockId = "lock_" + randomUUID().replace(/-/g, "");
      db2.prepare(`
        INSERT OR REPLACE INTO locks
          (lock_id, file_path, task_id, agent_id, session_id, lock_type, acquired_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(lockId, absPath, taskId, agentId2, sessionId, lockType, now, expiresAt);
      acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
    }
    db2.exec("COMMIT");
    return {
      ok: true,
      task: {
        task_id: taskId,
        agent_id: agentId2,
        session_id: sessionId,
        lock_type: lockType,
        workspace_path: wsPath,
        artifact: artifactScope,
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
      db2.exec("ROLLBACK");
    } catch {
    }
    throw e;
  }
}
function releaseFileLock(db2, params) {
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
  const requestedSuccessWithoutVerification = statusArg === "SUCCESS" && !verified;
  const effectiveStatus = verified ? "SUCCESS" : requestedSuccessWithoutVerification ? "PENDING" : statusArg;
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
        ).run(effectiveStatus, now, tid, agentId2);
        if (verified && verifiedNote) {
          try {
            db2.prepare(
              `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
               VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
            ).run("evt_" + randomUUID().replace(/-/g, ""), tid, agentId2, verifiedNote, now);
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
    agent_id: agentId2,
    status: effectiveStatus,
    released: locks.length > 0 || Boolean(taskId),
    locks_released: locks.length,
    task_ids: taskIds,
    updated_at: now,
    ...requestedSuccessWithoutVerification ? { unverifiedConclusion: "SUCCESS requested without --verified; stored as PENDING until verify records the test result." } : {}
  };
}

// src/verify.ts
import { randomUUID as randomUUID2 } from "node:crypto";

// src/sql/tasks.ts
var TASKS_UPDATE_PENDING_TO_FAILED = `UPDATE tasks SET status = 'FAILED', updated_at = ? WHERE task_id = ? AND status = 'PENDING'`;
var TASKS_UPDATE_ACTIVE_TO_FAILED = `UPDATE tasks SET status = 'FAILED', updated_at = ? WHERE task_id = ? AND status = 'ACTIVE'`;
var TASK_LOG_INSERT_ABANDONED = `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`;
var TASK_LOG_INSERT_STALE_ABANDONED = `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'stale active (no live locks) abandoned by audit-unverified --abandon', ?)`;

// src/verify.ts
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
  const artifact2 = normalizeArtifact(params.artifact);
  if (artifact2) {
    where.push("(artifact = ? OR artifact IS NULL)");
    binds.push(artifact2);
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
          "evt_" + randomUUID2().replace(/-/g, ""),
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
    if (artifact2) {
      staleWhere.push("(ai.artifact = ? OR ai.artifact IS NULL)");
      staleBinds.push(artifact2);
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
          "evt_" + randomUUID2().replace(/-/g, ""),
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

// src/maintenance.ts
import { spawnSync as spawnSync2 } from "node:child_process";
import { randomUUID as randomUUID4 } from "node:crypto";
import { isAbsolute as isAbsolute2, resolve as resolve4 } from "node:path";

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

// src/notifications.ts
import { randomUUID as randomUUID3 } from "node:crypto";

// src/sql/signals.ts
var SIGNALS_SELECT_BASE = "SELECT n.* FROM signals n";
var SIGNALS_SELECT_LEFT_JOIN_READS = "LEFT JOIN signal_reads nr ON nr.signal_id = n.signal_id AND nr.agent_id = ?";
var SIGNALS_SELECT_ORDER_LIMIT = "ORDER BY n.created_at DESC LIMIT ?";
var SIGNAL_READS_INSERT_IGNORE = "INSERT OR IGNORE INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)";

// src/sql/refinements.ts
var COLS = "refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json, reasoning, remember, quality, state, created_at, updated_at";
var REFINEMENTS_SELECT_OPEN = `SELECT ${COLS} FROM refinements
   WHERE state IN ('open','ongoing') AND quality <> 'handoff'
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
  const rows = db2.prepare(sql).all(...allBinds, limit);
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
function pruneStale(db2, params = {}) {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) : params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId2 = typeof params.agent_id === "string" ? params.agent_id : typeof params.agentId === "string" ? params.agentId : null;
  const workspacePath = typeof params.workspace === "string" ? params.workspace : typeof params.workspace_path === "string" ? params.workspace_path : typeof params.workspacePath === "string" ? params.workspacePath : null;
  const artifact2 = normalizeArtifact(params.artifact);
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
    binds.push(resolve4(workspacePath));
  }
  if (artifact2) {
    conditions.push("(t.artifact = ? OR t.artifact IS NULL)");
    binds.push(artifact2);
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
  const workspaceInput = params.workspace ?? params.workspace_path ?? params.workspacePath;
  const rawWorkspacePath = typeof workspaceInput === "string" && workspaceInput.trim() ? resolve4(workspaceInput.trim()) : null;
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
  const taskRows = db2.prepare(
    `SELECT task_id, rationale, test_plan, status, files_json, created_at, updated_at
     FROM tasks
     WHERE agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path IN (${workspacePlaceholders}) OR workspace_path IS NULL)
       AND (? IS NULL OR artifact = ? OR artifact IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId2, ...taskWorkspaceCandidates, artifact2, artifact2);
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
    if (artifact2) {
      cConds.push("(artifact = ? OR artifact IS NULL)");
      cBinds.push(artifact2);
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
  const refinementId = "ref_" + randomUUID4().replace(/-/g, "");
  const capturedFiles = [.../* @__PURE__ */ new Set([...files, ...dirtyFiles])];
  const statusSummary = taskRows.map((row) => {
    const rowFiles = parseJsonList(row.files_json);
    const fileSuffix = rowFiles.length > 0 ? ` files=${rowFiles.join(", ")}` : "";
    return `${row.status} ${row.task_id}: ${row.rationale}; verify=${row.test_plan}${fileSuffix}`;
  });
  const reasoning = [
    `Session capture for ${agentId2}${reason ? ` (${reason})` : ""}.`,
    `Unresolved tasks: ${taskRows.length} (${activeTasks} active, ${pendingTasks} pending).`,
    dirtyFiles.length > 0 ? `Dirty files: ${dirtyFiles.join(", ")}.` : null,
    statusSummary.length > 0 ? `Task details: ${statusSummary.join(" | ")}` : null
  ].filter(Boolean).join(" ");
  const remember = [
    `Review session handoff for ${agentId2}: ${activeTasks} active and ${pendingTasks} pending tasks remain.`,
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
    dirty_files: dirtyFiles,
    reason,
    consolidation_opportunities: consolidationOpportunities
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
    fts_rebuilt: ftsRebuilt
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
function extractPiWriteTargetPaths(toolName, input = {}) {
  const normalizedToolName = String(toolName ?? "").toLowerCase();
  const isWriteTool = [
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
  return new Promise((resolve5) => {
    let raw = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      raw += chunk;
    });
    process.stdin.on("end", () => resolve5(raw));
    process.stdin.on("error", () => resolve5(raw));
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
function agentId(payload) {
  return process.env.OCTOCODE_AGENT_ID || String(payload.session_id ?? payload.sessionId ?? payload.agent_id ?? payload.agentId ?? "claude-agent");
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
  const input = payloadInput(payload);
  const inputObj = objectOrEmpty2(input);
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? "";
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
    const result = preFlightIntent(db(), {
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
      workspacePath: workspace(payload) ?? void 0,
      artifact: artifact(payload),
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
    const r = spawnSync3("git", ["-C", dir, "rev-parse", "--abbrev-ref", "HEAD"], {
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
    const report = auditUnverified(db(), { agentId: agentId(payload), ...scopeArgs(payload) });
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
  } catch {
  }
  return 0;
}
function maybeRunDigest(payload) {
  if (process.env.OCTOCODE_NO_DIGEST === "1") return;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 36e5 : 4 * 36e5;
  const memoryHome2 = process.env.OCTOCODE_MEMORY_HOME || `${process.env.HOME ?? ""}/.octocode/memory`;
  const markerPath = join2(memoryHome2, ".last-digest-epoch-ms");
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
      mkdirSync2(memoryHome2, { recursive: true });
      writeFileSync(markerPath, String(now), "utf8");
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
      artifact: artifact(payload) ?? void 0,
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
      artifact: artifact(payload) ?? void 0,
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
