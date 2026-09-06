// src/sql/agents.ts — SQL constants for agents table

export const AGENTS_UPSERT = `INSERT INTO awareness_agents (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)
   ON CONFLICT(workspace_path, agent_id) DO UPDATE SET
     agent_name     = CASE WHEN excluded.agent_name <> '' THEN excluded.agent_name ELSE agent_name END,
     artifact       = COALESCE(excluded.artifact, artifact),
     context        = COALESCE(excluded.context, context),
     last_seen_at   = excluded.last_seen_at`;

export const AGENTS_UPDATE_LAST_SEEN = `UPDATE awareness_agents
  SET last_seen_at = ?, artifact = COALESCE(?, artifact)
  WHERE workspace_path = ? AND agent_id = ?`;

// ─── Resolve query fragments ──────────────────────────────────────────────────

export const AGENTS_SELECT_NAME_BY_ID = `SELECT agent_name FROM awareness_agents WHERE agent_id = ? ORDER BY last_seen_at DESC LIMIT 1`;

export const AGENTS_SELECT_NAMES_BY_IDS_PREFIX = `SELECT agent_id, agent_name FROM awareness_agents WHERE agent_id IN `;

export const AGENTS_SELECT_NAMES_NONEMPTY_SUFFIX = `AND agent_name <> '' ORDER BY last_seen_at DESC`;

// ─── List query fragments (composed dynamically in listAgents) ────────────────

export const AGENTS_LIST_SELECT = `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
   FROM awareness_agents`;

export const AGENTS_LIST_CLAUSE_WORKSPACE_PATH = `(workspace_path = ? OR workspace_path = '')`;

export const AGENTS_LIST_CLAUSE_ARTIFACT = `(artifact = ? OR artifact IS NULL)`;

export const AGENTS_LIST_ORDER = `ORDER BY last_seen_at DESC`;
