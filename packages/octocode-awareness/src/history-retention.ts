import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';

export interface HistoryPreviewRetentionResult {
  matched: number;
  deleted: number;
  preview_ids: string[];
  partial: boolean;
}

/** History-owned bounded cleanup for expired, unapplied restore previews. */
export function maintainExpiredHistoryPreviews(db: DatabaseSync, params: {
  workspacePath: string;
  now: string;
  limit: number;
  dryRun: boolean;
}): HistoryPreviewRetentionResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('history retention limit must be an integer in 1..500');
  }
  const select = () => db.prepare(`SELECT preview_id FROM local_history_restores
    WHERE workspace_path = ? AND status = 'ready' AND expires_at <= ?
    ORDER BY rowid ASC LIMIT ?`).all(params.workspacePath, params.now, params.limit + 1) as Array<{ preview_id: string }>;
  const result = (rows: Array<{ preview_id: string }>, deleted: number): HistoryPreviewRetentionResult => {
    const page = rows.slice(0, params.limit);
    return { matched: page.length, deleted, preview_ids: page.map(row => row.preview_id), partial: rows.length > params.limit };
  };
  if (params.dryRun) return result(select(), 0);

  const transaction = beginWrite(db);
  try {
    const rows = select();
    const page = rows.slice(0, params.limit);
    let deleted = 0;
    if (page.length > 0) {
      const placeholders = page.map(() => '?').join(',');
      deleted = Number((db.prepare(`DELETE FROM local_history_restores
        WHERE preview_id IN (${placeholders}) AND status = 'ready' AND expires_at <= ?`)
        .run(...page.map(row => row.preview_id), params.now) as { changes: number }).changes);
    }
    transaction.commit();
    return result(rows, deleted);
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
