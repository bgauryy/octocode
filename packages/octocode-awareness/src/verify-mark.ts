/**
 * verify.ts — Verify-gate operations for the awareness Stop hook.
 *
 * auditUnverified: returns runs with status='PENDING' (edited but not verified)
 *                  for an agent/workspace. The Stop hook (stop-verify.sh) blocks
 *                  conclude when count > 0.
 *
 * markVerified:    transitions a run PENDING → SUCCESS | FAILED so the gate
 *                  clears after the agent verifies its edits. Restricted to PENDING
 *                  transitions to prevent orphaning ACTIVE locks as SUCCESS.
 *                  A linked plan task moves VERIFY → DONE | FAILED with it.
 */
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import type { RunStatus } from './types.js';
import { RUN_LOG_INSERT_VERIFIED, RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT, RUNS_SELECT_STATUS, RUNS_SELECT_PENDING_IDS } from './sql/runs.js';
import { AgentStatusRow, closeRunFiles, finishLinkedTask, MarkVerifiedParams, MarkVerifiedResult, VALID_VERIFY_STATUSES } from './verify-shared.js';

/**
 * Transition a PENDING task to SUCCESS or FAILED.
 *
 * Only operates on PENDING tasks — attempting to verify an ACTIVE, SUCCESS,
 * or FAILED task returns ok=false with a descriptive error so the agent knows
 * exactly what went wrong.
 */
export function markVerified(
  db: DatabaseSync,
  params: MarkVerifiedParams,
): MarkVerifiedResult {
  const { agentId = 'agent', allPending = false, message } = params;
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const runId = params.runId ?? '';
  const status = params.status ?? 'SUCCESS';

  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" — must be SUCCESS or FAILED`,
      run_id: runId || null,
    };
  }

  const receipt = message?.trim() ?? '';
  if (status === 'SUCCESS' && !receipt) {
    return {
      ok: false,
      error: 'SUCCESS verification requires a non-empty evidence receipt in message',
      run_id: runId || null,
    };
  }
  if (allPending && !workspacePath && !artifact) {
    return {
      ok: false,
      error: '--all-pending requires --workspace or --artifact; use explicit run ids for cross-workspace verification',
      run_id: null,
    };
  }

  // --all-pending: verify every PENDING run for this agent/workspace at once
  if (allPending) {
    const dynWhere = [
      workspacePath ? ' AND workspace_path = ?' : '',
      artifact ? ' AND (artifact = ? OR artifact IS NULL)' : '',
    ].join('');
    const selectSql = RUNS_SELECT_PENDING_IDS.replace('{DYNAMIC_WHERE}', dynWhere);
    const selectBinds: (string | number)[] = [agentId];
    if (workspacePath) selectBinds.push(workspacePath);
    if (artifact) selectBinds.push(artifact);

    db.exec('BEGIN IMMEDIATE');
    try {
      const rows = db.prepare(selectSql).all(...selectBinds) as unknown as Array<{ run_id: string }>;
      const now = utcNow();
      const ids: string[] = [];
      for (const row of rows) {
        const upd = db.prepare(RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
          status, now, row.run_id, agentId,
        ) as { changes: number };
        if (upd.changes === 0) continue;
        closeRunFiles(db, row.run_id, now);
        finishLinkedTask(db, row.run_id, status, agentId, now, receipt || undefined);
        ids.push(row.run_id);
        if (receipt) {
          try {
            db.prepare(RUN_LOG_INSERT_VERIFIED).run(
              'evt_' + randomUUID().replace(/-/g, ''), row.run_id, agentId, receipt, now,
            );
          } catch { /* non-critical audit log */ }
        }
      }
      db.exec('COMMIT');
      // VER-1: Return null for run_id — no single task applies in allPending batch mode.
      return { ok: true, run_id: null, run_ids: ids, count: ids.length, status: status as RunStatus, updated_at: now };
    } catch (e) {
      try { db.exec('ROLLBACK'); } catch { /* not in transaction */ }
      throw e;
    }
  }

  if (!runId) {
    return { ok: false, error: '--run-id is required (or use --all-pending)', run_id: null };
  }

  const now = utcNow();
  db.exec('BEGIN IMMEDIATE');
  try {
    const result = db.prepare(RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
      status, now, runId, agentId,
    ) as { changes: number };

    if (result.changes === 0) {
      db.exec('ROLLBACK');
      // Distinguish: no such run / wrong agent / not PENDING
      const row = db.prepare(RUNS_SELECT_STATUS).get(runId) as unknown as AgentStatusRow | undefined;

      if (!row) {
        return { ok: false, error: `no run found with run_id=${runId}`, run_id: runId };
      }
      if (row.agent_id !== agentId) {
        return {
          ok: false,
          error: `run ${runId} belongs to agent "${row.agent_id}", not "${agentId}"`,
          run_id: runId,
        };
      }
      return {
        ok: false,
        error: `run ${runId} has status "${row.status}" — only PENDING runs can be verified`,
        run_id: runId,
      };
    }

    if (receipt) {
      try {
        db.prepare(RUN_LOG_INSERT_VERIFIED).run(
          'evt_' + randomUUID().replace(/-/g, ''), runId, agentId, receipt, now,
        );
      } catch { /* non-critical audit log */ }
    }

    closeRunFiles(db, runId, now);
    finishLinkedTask(db, runId, status, agentId, now, receipt || undefined);
    db.exec('COMMIT');
    return { ok: true, run_id: runId, status: status as RunStatus, updated_at: now };
  } catch (e) {
    try { db.exec('ROLLBACK'); } catch { /* not in transaction */ }
    throw e;
  }
}
