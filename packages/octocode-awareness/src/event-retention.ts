import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';
import { repositoryWorkspacePaths } from './git.js';

export interface EventRetentionResult {
  matched: number;
  deleted: number;
  boundary: number;
  partial: boolean;
}

const ELIGIBLE = `workspace_path = ?
  AND sequence <= ?
  AND retention_class IN ('delivery', 'operational')
  AND aggregate_kind IS NOT 'experience'
  AND (
    (expires_at IS NOT NULL AND expires_at <= ?)
    OR (expires_at IS NULL AND retention_class = 'delivery' AND created_at < ?)
    OR (expires_at IS NULL AND retention_class = 'operational' AND created_at < ?)
  )`;

/** Infrastructure-owned, consumer-safe event retention. Audit and Experience records are durable. */
export function pruneRetainedEvents(db: DatabaseSync, params: {
  workspacePath: string;
  now: string;
  deliveryBefore: string;
  operationalBefore: string;
  limit: number;
  dryRun: boolean;
}): EventRetentionResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('event retention limit must be an integer in 1..500');
  }
  const inspect = () => {
    const relatedWorkspaces = repositoryWorkspacePaths(params.workspacePath);
    const slowest = db.prepare(`SELECT MIN(sequence) AS sequence FROM event_consumers
      WHERE workspace_path IN (SELECT value FROM json_each(?))`)
      .get(JSON.stringify(relatedWorkspaces)) as { sequence: number | null };
    const boundary = Number(slowest.sequence ?? 0);
    const bindings = [params.workspacePath, boundary, params.now, params.deliveryBefore, params.operationalBefore] as const;
    const matched = Number((db.prepare(`SELECT COUNT(*) AS count FROM event_outbox WHERE ${ELIGIBLE}`)
      .get(...bindings) as { count: number }).count);
    return { boundary, bindings, matched };
  };
  if (params.dryRun) {
    const { boundary, matched } = inspect();
    return { matched, deleted: 0, boundary, partial: false };
  }

  const transaction = beginWrite(db);
  try {
    const { boundary, bindings } = inspect();
    const rows = db.prepare(`SELECT event_id FROM event_outbox WHERE ${ELIGIBLE}
      ORDER BY sequence ASC, event_id ASC LIMIT ?`)
      .all(...bindings, params.limit + 1) as Array<{ event_id: string }>;
    const page = rows.slice(0, params.limit);
    let deleted = 0;
    if (page.length > 0) {
      const placeholders = page.map(() => '?').join(',');
      deleted = Number((db.prepare(`DELETE FROM event_outbox WHERE event_id IN (${placeholders})
        AND ${ELIGIBLE}`).run(...page.map(row => row.event_id), ...bindings) as { changes: number }).changes);
    }
    transaction.commit();
    return { matched: page.length, deleted, boundary, partial: rows.length > params.limit };
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
