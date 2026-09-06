import type { DatabaseSync } from 'node:sqlite';
import { cutoffIso } from './coordination-shared.js';

export function countStaleAgentPresence(db: DatabaseSync, workspace: string, staleAfterMs: number): number {
  const row = db.prepare("SELECT COUNT(*) AS count FROM awareness_agents WHERE workspace_path = ? AND status != 'LEFT' AND last_seen_at < ?")
    .get(workspace, cutoffIso(staleAfterMs)) as { count: number };
  return row.count;
}

/** A zero freshness window explicitly disables age filtering. */
export function countPresentAgentPresence(db: DatabaseSync, workspace: string, staleAfterMs: number): number {
  const sql = staleAfterMs === 0
    ? "SELECT COUNT(*) AS count FROM awareness_agents WHERE workspace_path = ? AND status != 'LEFT'"
    : "SELECT COUNT(*) AS count FROM awareness_agents WHERE workspace_path = ? AND status != 'LEFT' AND last_seen_at >= ?";
  const row = staleAfterMs === 0
    ? db.prepare(sql).get(workspace)
    : db.prepare(sql).get(workspace, cutoffIso(staleAfterMs));
  return (row as { count: number }).count;
}
