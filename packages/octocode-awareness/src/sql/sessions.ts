// src/sql/sessions.ts — SQL constants for sessions table

export const SESSIONS_INSERT =
  `INSERT INTO sessions (session_id, agent_id, workspace_path, artifact, repo, ref, started_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)`;

export const SESSIONS_UPDATE_END =
  `UPDATE sessions SET ended_at = ?, summary = ? WHERE session_id = ? RETURNING *`;

export const SESSIONS_SELECT_BY_ID =
  `SELECT session_id, agent_id, workspace_path, artifact, repo, ref, started_at, ended_at, summary
   FROM sessions WHERE session_id = ?`;

export const SESSIONS_SELECT_ACTIVE =
  `SELECT session_id FROM sessions
   WHERE agent_id = ? AND workspace_path = ? AND (artifact = ? OR (artifact IS NULL AND ? IS NULL)) AND ended_at IS NULL
   LIMIT 1`;

// ─── List query fragments (composed dynamically in listSessions) ──────────────

export const SESSIONS_LIST_SELECT =
  `SELECT session_id, agent_id, workspace_path, artifact, repo, ref, started_at, ended_at, summary
   FROM sessions`;

export const SESSIONS_LIST_ORDER = `ORDER BY started_at DESC`;

export const SESSIONS_LIST_CLAUSE_AGENT_ID = `agent_id = ?`;

export const SESSIONS_LIST_CLAUSE_WORKSPACE_PATH = `workspace_path = ?`;

export const SESSIONS_LIST_CLAUSE_ARTIFACT = `artifact = ?`;

export const SESSIONS_LIST_CLAUSE_ACTIVE = `ended_at IS NULL`;
