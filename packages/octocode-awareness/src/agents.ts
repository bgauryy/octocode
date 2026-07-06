/**
 * agents.ts — Agent identity registry (agentId ↔ agentName mapping).
 *
 * ARCH-5: Raw agentIds like "pi:12345-abc8f3d2" are opaque in lock/notification
 * displays. A lightweight SQLite table lets callers register a human-readable
 * name once and resolve it on any read.
 *
 * Research finding (Claude Code reverse-engineering): Claude Code uses NO
 * persistent agent registry — identity is prompt-injected at assembly time.
 * Worker IDs are UUIDs tracked in coordinator context only. For octocode's
 * cross-session SQLite persistence model, a simple mapping table is the
 * right call since agent IDs are stored alongside memories and locks.
 *
 * Schema: `agent_identities` table added via ensureAgentIdentitiesSchema()
 * in db.ts (called by initDb on every connect — IF NOT EXISTS is safe).
 */

import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
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
  const workspacePath = params.workspacePath ?? null;
  const context = params.context ?? null;
  const now = utcNow();

  try {
    db.prepare(`
      INSERT INTO agent_identities(agent_id, agent_name, workspace_path, context, registered_at, last_seen_at)
      VALUES (?, ?, ?, ?, ?, ?)
      ON CONFLICT(agent_id) DO UPDATE SET
        agent_name     = CASE WHEN excluded.agent_name <> '' THEN excluded.agent_name ELSE agent_name END,
        workspace_path = COALESCE(excluded.workspace_path, workspace_path),
        context        = COALESCE(excluded.context, context),
        last_seen_at   = excluded.last_seen_at
    `).run(agentId, agentName, workspacePath, context, now, now);
  } catch { /* agent_identities table may not exist on very old DBs */ }

  return { agent_id: agentId, agent_name: agentName, workspace_path: workspacePath, context, registered_at: now, last_seen_at: now };
}

/**
 * Bump last_seen_at for an existing identity without changing the name.
 * Lightweight — call on every tool invocation to keep the registry fresh.
 */
export function touchAgent(db: DatabaseSync, agentId: string): void {
  try {
    db.prepare(
      `UPDATE agent_identities SET last_seen_at = ? WHERE agent_id = ?`
    ).run(utcNow(), agentId);
  } catch { /* ignore — table may not exist yet */ }
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
    const row = db.prepare(
      `SELECT agent_name FROM agent_identities WHERE agent_id = ?`
    ).get(agentId) as { agent_name: string } | undefined;
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
      `SELECT agent_id, agent_name FROM agent_identities WHERE agent_id IN (${ph}) AND agent_name <> ''`
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
  params: { workspacePath?: string | null } = {},
): ListAgentsResult {
  try {
    let sql =
      `SELECT agent_id, agent_name, workspace_path, context, registered_at, last_seen_at
       FROM agent_identities`;
    const binds: string[] = [];
    if (params.workspacePath) {
      sql += ` WHERE (workspace_path = ? OR workspace_path IS NULL)`;
      binds.push(params.workspacePath);
    }
    sql += ` ORDER BY last_seen_at DESC`;
    const rows = db.prepare(sql).all(...binds) as unknown as AgentIdentity[];
    return { count: rows.length, agents: rows };
  } catch {
    return { count: 0, agents: [] };
  }
}
