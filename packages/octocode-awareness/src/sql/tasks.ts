// src/sql/tasks.ts — SQL constants for tasks, locks, task_log tables

// ─── Lock eviction ────────────────────────────────────────────────────────────

/** Delete all expired locks; bind: (nowIso) — used by evictExpiredLocks in db.ts */
export const LOCKS_EVICT_EXPIRED =
  `DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?`;

// ─── Lock inserts ─────────────────────────────────────────────────────────────

/** Upsert a single file lock row; bind: (lock_id, file_path, task_id, agent_id, session_id, lock_type, acquired_at, expires_at) */
export const LOCKS_INSERT =
  `INSERT OR REPLACE INTO locks
     (lock_id, file_path, task_id, agent_id, session_id, lock_type, acquired_at, expires_at)
   VALUES (?, ?, ?, ?, ?, ?, ?, ?)`;

// ─── Lock selects ─────────────────────────────────────────────────────────────

/**
 * Select active non-expired locks joined to their parent task.
 * {DYNAMIC_WHERE}: caller appends clauses for workspace_path, agent_id, session_id,
 * task_id after the mandatory "ai.status='ACTIVE'" and expires_at filter.
 * bind: (nowIso, ...optional_scope_values)
 */
export const LOCKS_SELECT_ACTIVE =
  `SELECT fl.lock_id, fl.task_id, fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path,
          ai.rationale AS reasoning, fl.lock_type, fl.acquired_at, fl.expires_at
   FROM locks fl
   JOIN tasks ai ON ai.task_id = fl.task_id
   WHERE ai.status = 'ACTIVE'
     AND (fl.expires_at IS NULL OR fl.expires_at > ?)
   {DYNAMIC_WHERE}
   ORDER BY fl.acquired_at DESC`;

/**
 * Conflict check: locks on a file held by a different agent.
 * {DYNAMIC_WHERE}: replaced with lock_type filter ("fl.lock_type = 'EXCLUSIVE'" for SHARED request, or "1 = 1").
 * bind: (file_path, agentId, nowIso)
 */
export const LOCKS_SELECT_CONFLICTS =
  `SELECT fl.*, ai.agent_id AS task_agent_id
   FROM locks fl
   JOIN tasks ai ON ai.task_id = fl.task_id
   WHERE fl.file_path = ?
     AND ai.agent_id <> ?
     AND ai.status = 'ACTIVE'
     AND {DYNAMIC_WHERE}
     AND (fl.expires_at IS NULL OR fl.expires_at > ?)`;

/**
 * Select lock rows to be released; {DYNAMIC_WHERE} is built from agent_id, session_id,
 * task_id, file_path IN list — always includes "fl.agent_id = ?".
 * bind: (agentId, ...optional_scope_values)
 */
export const LOCKS_SELECT_FOR_RELEASE =
  `SELECT fl.lock_id, fl.task_id, fl.file_path FROM locks fl WHERE {DYNAMIC_WHERE}`;

/**
 * Check whether a task still has at least one live lock (used before updating task status).
 * bind: (task_id)
 */
export const LOCKS_SELECT_REMAINING =
  `SELECT 1 FROM locks WHERE task_id = ? LIMIT 1`;

/**
 * Select stale/expired lock rows for pruneStale.
 * {DYNAMIC_WHERE}: includes expired/age conditions plus optional agent_id and file_path IN list.
 * bind varies; see pruneStale for exact construction.
 */
export const LOCKS_SELECT_STALE =
  `SELECT lock_id, task_id FROM locks WHERE {DYNAMIC_WHERE}`;

/**
 * Workspace lock listing joined to tasks; {DYNAMIC_WHERE} is optional "WHERE ai.workspace_path = ?".
 * bind: (...optional_workspace_value)
 */
export const LOCKS_SELECT_WORKSPACE =
  `SELECT fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, fl.task_id,
          fl.lock_type, fl.acquired_at, fl.expires_at
   FROM locks fl
   JOIN tasks ai ON ai.task_id = fl.task_id
   {DYNAMIC_WHERE}
   ORDER BY fl.acquired_at DESC
   LIMIT 50`;

/**
 * Conflict check for waitForLock polling.
 * {DYNAMIC_WHERE}: optional "AND fl.lock_type = 'EXCLUSIVE'" injected for SHARED requests.
 * bind: (...file_paths, agentId, nowIso)
 */
export const LOCKS_SELECT_CONFLICTS_FOR_WAIT =
  `SELECT fl.file_path, ai.agent_id, fl.expires_at
   FROM locks fl
   JOIN tasks ai ON ai.task_id = fl.task_id
   WHERE fl.file_path IN ({PLACEHOLDERS})
     AND ai.agent_id <> ?
     AND ai.status = 'ACTIVE'
     {DYNAMIC_WHERE}
     AND (fl.expires_at IS NULL OR fl.expires_at > ?)`;

// ─── Lock deletes ─────────────────────────────────────────────────────────────

/**
 * Delete locks for releaseFileLock; {DYNAMIC_WHERE} mirrors LOCKS_SELECT_FOR_RELEASE
 * but without the "fl." alias (direct table query, not joined).
 * bind: (agentId, ...optional_scope_values)
 */
export const LOCKS_DELETE_BY_AGENT =
  `DELETE FROM locks WHERE {DYNAMIC_WHERE}`;

/**
 * Bulk-delete stale locks by primary key list; {PLACEHOLDERS} = comma-separated "?".
 * bind: (...lock_ids)
 */
export const LOCKS_DELETE_BY_IDS =
  `DELETE FROM locks WHERE lock_id IN ({PLACEHOLDERS})`;

// ─── Lock updates ─────────────────────────────────────────────────────────────

/** Extend all lock TTLs for a task; bind: (expires_at, task_id, agent_id) */
export const LOCKS_RENEW =
  `UPDATE locks SET expires_at = ? WHERE task_id = ? AND agent_id = ?`;

// ─── Task inserts ─────────────────────────────────────────────────────────────

/** Insert a new ACTIVE task row; bind: (task_id, agent_id, session_id, rationale, test_plan, workspace_path, artifact, files_json, created_at, updated_at) */
export const TASKS_INSERT =
  `INSERT INTO tasks
     (task_id, agent_id, session_id, rationale, test_plan, status, workspace_path, artifact, files_json, created_at, updated_at)
   VALUES (?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?)`;

// ─── Task selects ─────────────────────────────────────────────────────────────

/**
 * Select PENDING tasks for auditUnverified.
 * {DYNAMIC_WHERE}: optional agent_id and workspace_path filters appended after "status = 'PENDING'".
 * bind: (...optional_scope_values)
 */
export const TASKS_SELECT_PENDING =
  `SELECT task_id, agent_id, status, test_plan, rationale, workspace_path, artifact, files_json, created_at
   FROM tasks
   WHERE status = 'PENDING'
   {DYNAMIC_WHERE}
   ORDER BY created_at ASC`;

/**
 * Select task_id list of PENDING tasks for markVerified allPending.
 * {DYNAMIC_WHERE}: mandatory "agent_id = ?" plus optional workspace_path filter.
 * bind: (agentId, ...optional_scope_values)
 */
export const TASKS_SELECT_PENDING_IDS =
  `SELECT task_id FROM tasks WHERE status = 'PENDING' AND agent_id = ? {DYNAMIC_WHERE}`;

/**
 * Select ACTIVE tasks with no live locks (stale/orphaned sessions) for auditUnverified.
 * {DYNAMIC_WHERE}: optional agent_id and workspace_path appended after the NOT EXISTS subquery.
 * bind: (nowIso, ...optional_scope_values)
 */
export const TASKS_SELECT_STALE_ACTIVE =
  `SELECT ai.task_id, ai.agent_id, ai.rationale, ai.workspace_path, ai.artifact, ai.files_json, ai.created_at
   FROM tasks ai
   WHERE ai.status = 'ACTIVE'
     AND NOT EXISTS (
       SELECT 1 FROM locks fl
       WHERE fl.task_id = ai.task_id
         AND (fl.expires_at IS NULL OR fl.expires_at > ?)
     )
   {DYNAMIC_WHERE}
   ORDER BY ai.created_at ASC`;

/**
 * Fetch agent_id and status for a single task (used to distinguish error cases in markVerified).
 * bind: (task_id)
 */
export const TASKS_SELECT_STATUS =
  `SELECT agent_id, status FROM tasks WHERE task_id = ?`;

/**
 * Select ACTIVE/PENDING tasks for an agent+workspace (sessionCapture snapshot).
 * bind: (agentId, workspacePath)
 */
export const TASKS_SELECT_ACTIVE_PENDING_BY_AGENT =
  `SELECT task_id, rationale, test_plan, status, files_json, created_at, updated_at
   FROM tasks
   WHERE agent_id = ?
     AND status IN ('ACTIVE', 'PENDING')
     AND (workspace_path = ? OR workspace_path IS NULL)
   ORDER BY updated_at DESC, created_at DESC
   LIMIT 20`;

/** Count PENDING tasks; {DYNAMIC_WHERE}: optional "AND workspace_path = ?". bind: (...) */
export const TASKS_COUNT_PENDING =
  `SELECT COUNT(*) AS c FROM tasks WHERE status = 'PENDING' {DYNAMIC_WHERE}`;

/** Count ACTIVE tasks; {DYNAMIC_WHERE}: optional "AND workspace_path = ?". bind: (...) */
export const TASKS_COUNT_ACTIVE =
  `SELECT COUNT(*) AS c FROM tasks WHERE status = 'ACTIVE' {DYNAMIC_WHERE}`;

// ─── Task updates ─────────────────────────────────────────────────────────────

/** Set task status to any value; bind: (status, updated_at, task_id, agent_id) */
export const TASKS_UPDATE_STATUS =
  `UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND agent_id = ?`;

/** Bump updated_at without changing status (used after lock renew); bind: (updated_at, task_id, agent_id) */
export const TASKS_UPDATE_TOUCHED =
  `UPDATE tasks SET updated_at = ? WHERE task_id = ? AND agent_id = ?`;

/** Transition only a PENDING task to a new status; bind: (status, updated_at, task_id, status='PENDING') */
export const TASKS_UPDATE_PENDING_VERIFIED =
  `UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND status = 'PENDING'`;

/** Transition only a PENDING task to a new status, scoped to agent; bind: (status, updated_at, task_id, agent_id) */
export const TASKS_UPDATE_PENDING_VERIFIED_BY_AGENT =
  `UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND agent_id = ? AND status = 'PENDING'`;

/** Abandon a PENDING task as FAILED (audit --abandon); bind: (updated_at, task_id) */
export const TASKS_UPDATE_PENDING_TO_FAILED =
  `UPDATE tasks SET status = 'FAILED', updated_at = ? WHERE task_id = ? AND status = 'PENDING'`;

/** Reset an orphaned ACTIVE task to PENDING when its locks are gone; bind: (updated_at, task_id) */
export const TASKS_UPDATE_ACTIVE_TO_PENDING =
  `UPDATE tasks SET status = 'PENDING', updated_at = ? WHERE task_id = ? AND status = 'ACTIVE'`;

/** Mark a stale ACTIVE task as FAILED (abandon stale-active); bind: (updated_at, task_id) */
export const TASKS_UPDATE_ACTIVE_TO_FAILED =
  `UPDATE tasks SET status = 'FAILED', updated_at = ? WHERE task_id = ? AND status = 'ACTIVE'`;

// ─── task_log inserts ─────────────────────────────────────────────────────────

/** Record a VERIFIED event after an agent confirms its edits; bind: (event_id, task_id, agent_id, message, created_at) */
export const TASK_LOG_INSERT_VERIFIED =
  `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'VERIFIED', ?, ?)`;

/** Record an ABANDONED event when audit --abandon dismisses an orphaned PENDING task; bind: (event_id, task_id, agent_id, created_at) */
export const TASK_LOG_INSERT_ABANDONED =
  `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`;

/** Record an ABANDONED event when audit --abandon clears a stale ACTIVE (no-live-locks) task; bind: (event_id, task_id, agent_id, created_at) */
export const TASK_LOG_INSERT_STALE_ABANDONED =
  `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'stale active (no live locks) abandoned by audit-unverified --abandon', ?)`;

/** Generic task_log insert with a caller-supplied event_type and message; bind: (event_id, task_id, agent_id, event_type, message, created_at) */
export const TASK_LOG_INSERT =
  `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, ?, ?, ?)`;
