import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';

export interface InteractionRetentionResult {
  expired: number;
  matched: number;
  deleted: number;
  partial: boolean;
}

const TERMINAL_ELIGIBLE = `workspace_path = ?
  AND NOT EXISTS (
    SELECT 1 FROM authorization_receipts receipt
    WHERE receipt.interaction_id = pending_interactions.interaction_id
  )
  AND (
    (status IN ('answered', 'cancelled', 'expired') AND resolved_at < ?)
    OR (status = 'pending' AND expires_at <= ? AND expires_at < ?)
  )`;

/** Host-owned interaction expiry and terminal-row retention. */
export function maintainInteractions(db: DatabaseSync, params: {
  workspacePath: string;
  terminalBefore: string;
  now: string;
  limit: number;
  dryRun: boolean;
}): InteractionRetentionResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('interaction retention limit must be an integer in 1..500');
  }
  const counts = () => ({
    expired: Number((db.prepare(`SELECT COUNT(*) AS count FROM pending_interactions
      WHERE workspace_path = ? AND status = 'pending' AND expires_at <= ?`)
      .get(params.workspacePath, params.now) as { count: number }).count),
    matched: Number((db.prepare(`SELECT COUNT(*) AS count FROM pending_interactions
      WHERE ${TERMINAL_ELIGIBLE}`)
      .get(params.workspacePath, params.terminalBefore, params.now, params.terminalBefore) as { count: number }).count),
  });
  if (params.dryRun) return { ...counts(), deleted: 0, partial: false };

  const transaction = beginWrite(db);
  try {
    const expireRows = db.prepare(`SELECT interaction_id FROM pending_interactions
      WHERE workspace_path = ? AND status = 'pending' AND expires_at <= ?
      ORDER BY expires_at ASC, interaction_id ASC LIMIT ?`)
      .all(params.workspacePath, params.now, params.limit + 1) as Array<{ interaction_id: string }>;
    const deleteRows = db.prepare(`SELECT interaction_id FROM pending_interactions
      WHERE ${TERMINAL_ELIGIBLE} ORDER BY COALESCE(resolved_at, expires_at) ASC, interaction_id ASC LIMIT ?`)
      .all(
        params.workspacePath, params.terminalBefore, params.now, params.terminalBefore, params.limit + 1,
      ) as Array<{ interaction_id: string }>;
    const expirePage = expireRows.slice(0, params.limit);
    const deletePage = deleteRows.slice(0, params.limit);
    let expired = 0;
    if (expirePage.length > 0) {
      const placeholders = expirePage.map(() => '?').join(',');
      expired = Number((db.prepare(`UPDATE pending_interactions
        SET status = 'expired', resolved_at = expires_at
        WHERE interaction_id IN (${placeholders}) AND workspace_path = ?
          AND status = 'pending' AND expires_at <= ?`).run(
        ...expirePage.map(row => row.interaction_id), params.workspacePath, params.now,
      ) as { changes: number }).changes);
    }
    let deleted = 0;
    if (deletePage.length > 0) {
      const placeholders = deletePage.map(() => '?').join(',');
      deleted = Number((db.prepare(`DELETE FROM pending_interactions
        WHERE interaction_id IN (${placeholders}) AND ${TERMINAL_ELIGIBLE}`).run(
        ...deletePage.map(row => row.interaction_id),
        params.workspacePath, params.terminalBefore, params.now, params.terminalBefore,
      ) as { changes: number }).changes);
    }
    transaction.commit();
    return {
      expired,
      matched: deletePage.length,
      deleted,
      partial: expireRows.length > params.limit || deleteRows.length > params.limit,
    };
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
