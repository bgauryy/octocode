/**
 * agents.ts — Agent identity registry (agentId ↔ agentName mapping).
 *
 * ARCH-5: Raw agentIds like "pi:12345-abc8f3d2" are opaque in lock/notification
 * displays. A lightweight SQLite table lets callers register a human-readable
 * name once and resolve it on any read.
 *
 * Schema: `agents` table.
 */

import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import {
  AGENTS_UPSERT,
  AGENTS_UPDATE_LAST_SEEN,
  AGENTS_SELECT_NAME_BY_ID,
  AGENTS_SELECT_NAMES_BY_IDS_PREFIX,
  AGENTS_SELECT_NAMES_NONEMPTY_SUFFIX,
  AGENTS_LIST_SELECT,
  AGENTS_LIST_CLAUSE_WORKSPACE_PATH,
  AGENTS_LIST_CLAUSE_ARTIFACT,
  AGENTS_LIST_ORDER,
} from './sql/agents.js';
import type { AgentIdentity, RegisterAgentParams, ListAgentsResult } from './types.js';

// ─── Register / touch ────────────────────────────────────────────────────────

/**
 * Upsert an agent identity record.
 *
 * Safe to call repeatedly — uses INSERT OR REPLACE with conditional name update:
 * an empty name never overwrites a stored name, but a non-empty name always wins.
 *
 * Call this at session start or whenever the agent name becomes known
 * (e.g. in pi-hooks `handleBeforeAgentStart`, or when the first memory is recorded).
 */
export function registerAgent(
  db: DatabaseSync,
  params: RegisterAgentParams,
): AgentIdentity {
  const agentId = params.agentId;
  const agentName = params.agentName ?? '';  // null/undefined both become ''
  // Normalize to the same git-root + symlink-canonicalized scope key used by
  // memory/lock/signal so `workspace status`/`agent list` see the same rows.
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const context = params.context ?? null;
  const now = utcNow();

  db.prepare(AGENTS_UPSERT).run(agentId, agentName, workspacePath, artifact, context, now, now);

  return { agent_id: agentId, agent_name: agentName, workspace_path: workspacePath, artifact, context, registered_at: now, last_seen_at: now };
}

/**
 * Bump last_seen_at for an existing identity without changing the name.
 * Lightweight — call on every tool invocation to keep the registry fresh.
 */
export function touchAgent(db: DatabaseSync, agentId: string, workspacePath: string | null = null, artifact: string | null = null): void {
  try {
    const normalized = workspacePath ? normalizeWorkspacePath(workspacePath, workspacePath) : null;
    db.prepare(AGENTS_UPDATE_LAST_SEEN).run(utcNow(), normalized, normalizeArtifact(artifact), agentId);
  } catch { /* non-critical registry touch */ }
}

// ─── Resolve ──────────────────────────────────────────────────────────────────

/**
 * Resolve an agentId to its human-readable display name.
 * Returns null when the agent is not registered or has no name.
 *
 * Never throws — safe to call inside briefing/display paths.
 */
export function resolveAgentName(db: DatabaseSync, agentId: string): string | null {
  try {
    const row = db.prepare(AGENTS_SELECT_NAME_BY_ID).get(agentId) as { agent_name: string } | undefined;
    const name = row?.agent_name ?? '';
    return name !== '' ? name : null;
  } catch {
    return null;
  }
}

/**
 * Resolve multiple agentIds to display names in a single query.
 * Returns a Map<agentId, agentName> — missing entries have no key.
 */
export function resolveAgentNames(
  db: DatabaseSync,
  agentIds: string[],
): Map<string, string> {
  const result = new Map<string, string>();
  if (agentIds.length === 0) return result;
  try {
    const ph = agentIds.map(() => '?').join(',');
    const rows = db.prepare(
      `${AGENTS_SELECT_NAMES_BY_IDS_PREFIX}(${ph}) ${AGENTS_SELECT_NAMES_NONEMPTY_SUFFIX}`
    ).all(...agentIds) as unknown as Array<{ agent_id: string; agent_name: string }>;
    for (const row of rows) result.set(row.agent_id, row.agent_name);
  } catch { /* ignore */ }
  return result;
}

// ─── List ─────────────────────────────────────────────────────────────────────

/**
 * List all known agent identities, ordered by last_seen_at DESC.
 * Optionally filter to a workspace (or-NULL so global agents always appear).
 *
 * Never throws — returns { count: 0, agents: [] } on error.
 */
export function listAgents(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null } = {},
): ListAgentsResult {
  try {
    const binds: string[] = [];
    let sql = AGENTS_LIST_SELECT;
    const clauses: string[] = [];
    if (params.workspacePath) {
      clauses.push(AGENTS_LIST_CLAUSE_WORKSPACE_PATH);
      binds.push(normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? params.workspacePath);
    }
    const artifact = normalizeArtifact(params.artifact);
    if (artifact) {
      clauses.push(AGENTS_LIST_CLAUSE_ARTIFACT);
      binds.push(artifact);
    }
    if (clauses.length > 0) sql += ` WHERE ${clauses.join(' AND ')}`;
    sql += ` ${AGENTS_LIST_ORDER}`;
    const rows = db.prepare(sql).all(...binds) as unknown as AgentIdentity[];
    return { count: rows.length, agents: rows };
  } catch {
    return { count: 0, agents: [] };
  }
}
