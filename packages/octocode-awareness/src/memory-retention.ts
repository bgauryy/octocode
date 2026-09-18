import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';
import { hasFts, rebuildFts } from './db-maintenance.js';

export interface MemoryRetentionResult {
  archived: number;
  matched: number;
  deleted: number;
  ftsRebuilt: boolean;
  partial: boolean;
}

/** Memory-owned expiry, terminal retention, and derived-index consistency. */
export function maintainMemories(db: DatabaseSync, params: {
  workspacePath: string;
  terminalBefore: string;
  now: string;
  limit: number;
  dryRun: boolean;
}): MemoryRetentionResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('memory retention limit must be an integer in 1..500');
  }
  const counts = () => ({
    archived: Number((db.prepare(`SELECT COUNT(*) AS count FROM awareness_memories
      WHERE workspace_path = ? AND state = 'ACTIVE' AND valid_to IS NOT NULL AND valid_to <= ?`)
      .get(params.workspacePath, params.now) as { count: number }).count),
    matched: Number((db.prepare(`SELECT COUNT(*) AS count FROM awareness_memories
      WHERE workspace_path = ? AND state = 'SUPERSEDED' AND updated_at < ?`)
      .get(params.workspacePath, params.terminalBefore) as { count: number }).count),
  });
  if (params.dryRun) return { ...counts(), deleted: 0, ftsRebuilt: false, partial: false };

  const transaction = beginWrite(db);
  try {
    const archiveRows = db.prepare(`SELECT memory_id FROM awareness_memories
      WHERE workspace_path = ? AND state = 'ACTIVE' AND valid_to IS NOT NULL AND valid_to <= ?
      ORDER BY valid_to ASC, memory_id ASC LIMIT ?`)
      .all(params.workspacePath, params.now, params.limit + 1) as Array<{ memory_id: string }>;
    const deleteRows = db.prepare(`SELECT memory_id FROM awareness_memories
      WHERE workspace_path = ? AND state = 'SUPERSEDED' AND updated_at < ?
      ORDER BY updated_at ASC, memory_id ASC LIMIT ?`)
      .all(params.workspacePath, params.terminalBefore, params.limit + 1) as Array<{ memory_id: string }>;
    const archivePage = archiveRows.slice(0, params.limit);
    const deletePage = deleteRows.slice(0, params.limit);
    let archived = 0;
    if (archivePage.length > 0) {
      const placeholders = archivePage.map(() => '?').join(',');
      archived = Number((db.prepare(`UPDATE awareness_memories
      SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
      WHERE memory_id IN (${placeholders}) AND workspace_path = ? AND state = 'ACTIVE'
        AND valid_to IS NOT NULL AND valid_to <= ?`).run(
        params.now, params.now, ...archivePage.map(row => row.memory_id), params.workspacePath, params.now,
      ) as { changes: number }).changes);
    }
    let deleted = 0;
    if (deletePage.length > 0) {
      const placeholders = deletePage.map(() => '?').join(',');
      deleted = Number((db.prepare(`DELETE FROM awareness_memories
        WHERE memory_id IN (${placeholders}) AND workspace_path = ?
          AND state = 'SUPERSEDED' AND updated_at < ?`).run(
        ...deletePage.map(row => row.memory_id), params.workspacePath, params.terminalBefore,
      ) as { changes: number }).changes);
    }
    const ftsRebuilt = (archived > 0 || deleted > 0) && hasFts(db);
    if (ftsRebuilt) rebuildFts(db);
    transaction.commit();
    return {
      archived,
      matched: deletePage.length,
      deleted,
      ftsRebuilt,
      partial: archiveRows.length > params.limit || deleteRows.length > params.limit,
    };
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
