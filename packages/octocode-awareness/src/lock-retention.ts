import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';

export interface LockRetentionResult {
  matched: number;
  deleted: number;
  partial: boolean;
}

/** Lock-owned cleanup. Reports the whole backlog and applies deterministic bounded batches. */
export function pruneExpiredLocks(db: DatabaseSync, params: {
  workspacePath: string;
  now: string;
  limit: number;
  dryRun: boolean;
}): LockRetentionResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('lock retention limit must be an integer in 1..500');
  }

  const where = `FROM awareness_locks AS lock
    JOIN task_runs AS run ON run.run_id = lock.run_id
    WHERE run.workspace_path = ? AND lock.expires_at <= ?`;
  if (params.dryRun) {
    const row = db.prepare(`SELECT COUNT(*) AS count ${where}`)
      .get(params.workspacePath, params.now) as { count: number };
    return { matched: Number(row.count), deleted: 0, partial: false };
  }

  const transaction = beginWrite(db);
  try {
    const rows = db.prepare(`SELECT lock.lock_id ${where}
      ORDER BY lock.expires_at ASC, lock.lock_id ASC LIMIT ?`)
      .all(params.workspacePath, params.now, params.limit + 1) as Array<{ lock_id: string }>;
    const page = rows.slice(0, params.limit);
    let deleted = 0;
    if (page.length > 0) {
      const placeholders = page.map(() => '?').join(',');
      deleted = Number((db.prepare(`DELETE FROM awareness_locks
        WHERE lock_id IN (${placeholders}) AND expires_at <= ?`)
        .run(...page.map(row => row.lock_id), params.now) as { changes: number }).changes);
    }
    transaction.commit();
    return { matched: page.length, deleted, partial: rows.length > params.limit };
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
