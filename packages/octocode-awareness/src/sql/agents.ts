// src/sql/agents.ts — SQL constants for agents table

export const AGENTS_UPSERT =
  `INSERT INTO agents (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)
   ON CONFLICT(agent_id) DO UPDATE SET
     agent_name     = CASE WHEN excluded.agent_name <> '' THEN excluded.agent_name ELSE agent_name END,
     workspace_path = COALESCE(excluded.workspace_path, workspace_path),
     artifact       = COALESCE(excluded.artifact, artifact),
     context        = COALESCE(excluded.context, context),
     last_seen_at   = excluded.last_seen_at`;

export const AGENTS_SELECT_BY_ID =
  `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
   FROM agents WHERE agent_id = ?`;

export const AGENTS_SELECT_ALL =
  `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
   FROM agents ORDER BY last_seen_at DESC`;

export const AGENTS_UPDATE_LAST_SEEN =
  `UPDATE agents SET last_seen_at = ?, workspace_path = COALESCE(?, workspace_path), artifact = COALESCE(?, artifact) WHERE agent_id = ?`;

// ─── Resolve query fragments ──────────────────────────────────────────────────

export const AGENTS_SELECT_NAME_BY_ID =
  `SELECT agent_name FROM agents WHERE agent_id = ?`;

export const AGENTS_SELECT_NAMES_BY_IDS_PREFIX =
  `SELECT agent_id, agent_name FROM agents WHERE agent_id IN `;

export const AGENTS_SELECT_NAMES_NONEMPTY_SUFFIX =
  `AND agent_name <> ''`;

// ─── List query fragments (composed dynamically in listAgents) ────────────────

export const AGENTS_LIST_SELECT =
  `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
   FROM agents`;

export const AGENTS_LIST_CLAUSE_WORKSPACE_PATH = `(workspace_path = ? OR workspace_path IS NULL)`;

export const AGENTS_LIST_CLAUSE_ARTIFACT = `(artifact = ? OR artifact IS NULL)`;

export const AGENTS_LIST_ORDER = `ORDER BY last_seen_at DESC`;
