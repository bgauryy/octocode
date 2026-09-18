import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';
import { appendRunVerificationEvent } from './event-outbox.js';
import { RUNS_UPDATE_ACTIVE_TO_FAILED } from './sql/runs.js';
import { closeRunFiles, failStaleLinkedTask } from './verify-shared.js';

export interface RunRetentionResult {
  matched: number;
  deleted: number;
  partial: boolean;
}

export interface StaleRunRecoveryResult {
  matched: number;
  failed: number;
  partial: boolean;
}

const ELIGIBLE = `workspace_path = ?
  AND task_id IS NULL
  AND origin IN ('WORK', 'HOOK')
  AND status IN ('SUCCESS', 'FAILED')
  AND updated_at < ?`;

/** Work-owned retention for terminal standalone and hook execution rows. */
export function pruneTerminalRuns(db: DatabaseSync, params: {
  workspacePath: string;
  before: string;
  limit: number;
  dryRun: boolean;
}): RunRetentionResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('run retention limit must be an integer in 1..500');
  }
  const count = () => Number((db.prepare(`SELECT COUNT(*) AS count FROM task_runs WHERE ${ELIGIBLE}`)
    .get(params.workspacePath, params.before) as { count: number }).count);
  if (params.dryRun) return { matched: count(), deleted: 0, partial: false };

  const transaction = beginWrite(db);
  try {
    const rows = db.prepare(`SELECT run_id FROM task_runs WHERE ${ELIGIBLE}
      ORDER BY updated_at ASC, run_id ASC LIMIT ?`)
      .all(params.workspacePath, params.before, params.limit + 1) as Array<{ run_id: string }>;
    const page = rows.slice(0, params.limit);
    let deleted = 0;
    if (page.length > 0) {
      const placeholders = page.map(() => '?').join(',');
      deleted = Number((db.prepare(`DELETE FROM task_runs WHERE run_id IN (${placeholders})
        AND ${ELIGIBLE}`).run(
        ...page.map(row => row.run_id), params.workspacePath, params.before,
      ) as { changes: number }).changes);
    }
    transaction.commit();
    return { matched: page.length, deleted, partial: rows.length > params.limit };
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}

/** Work-owned, opt-in recovery for ACTIVE runs whose claims and file presence have expired. */
export function recoverStaleActiveRuns(db: DatabaseSync, params: {
  workspacePath: string;
  before: string;
  now: string;
  limit: number;
  apply: boolean;
  dryRun: boolean;
}): StaleRunRecoveryResult {
  if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 500) {
    throw new Error('stale run recovery limit must be an integer in 1..500');
  }
  const eligible = `run.status = 'ACTIVE' AND run.workspace_path = ? AND run.updated_at < ?
    AND EXISTS (SELECT 1 FROM run_files any_file WHERE any_file.run_id = run.run_id)
    AND NOT EXISTS (SELECT 1 FROM run_files live_file
      WHERE live_file.run_id = run.run_id AND live_file.ended_at IS NULL AND live_file.expires_at > ?)
    AND NOT EXISTS (SELECT 1 FROM task_claims claim
      WHERE claim.run_id = run.run_id AND claim.expires_at > ?)`;
  const bindings = [params.workspacePath, params.before, params.now, params.now] as const;
  if (params.dryRun || !params.apply) {
    const count = Number((db.prepare(`SELECT COUNT(*) AS count FROM task_runs run WHERE ${eligible}`)
      .get(...bindings) as { count: number }).count);
    return { matched: count, failed: 0, partial: false };
  }

  const transaction = beginWrite(db);
  try {
    const rows = db.prepare(`SELECT run.run_id, run.agent_id FROM task_runs run WHERE ${eligible}
      ORDER BY run.updated_at ASC, run.run_id ASC LIMIT ?`)
      .all(...bindings, params.limit + 1) as Array<{ run_id: string; agent_id: string }>;
    const page = rows.slice(0, params.limit);
    let failed = 0;
    for (const run of page) {
      const changed = Number(db.prepare(RUNS_UPDATE_ACTIVE_TO_FAILED).run(params.now, run.run_id).changes);
      if (changed !== 1) continue;
      closeRunFiles(db, run.run_id, params.now);
      const message = 'maintenance retention: ACTIVE run had no live claim or file presence';
      failStaleLinkedTask(db, run.run_id, run.agent_id, params.now, message);
      appendRunVerificationEvent(db, {
        runId: run.run_id,
        agentId: run.agent_id,
        message,
        createdAt: params.now,
      });
      failed++;
    }
    transaction.commit();
    return { matched: page.length, failed, partial: rows.length > params.limit };
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
