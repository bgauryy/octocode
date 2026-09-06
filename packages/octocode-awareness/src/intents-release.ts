import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';
import { normalizeArtifact, utcNow } from './helpers.js';
import type { FileLockReleaseStatus, ReleaseFileLockParams, ReleaseFileLockResult } from './types/locks-reflection.js';
import { resolveTargetFiles, VALID_RELEASE_STATUSES, workspaceScopeRoot } from './intents-preflight.js';

/**
 * Release file locks for a run or specific files.
 */
export function releaseFileLock(
  db: DatabaseSync,
  params: ReleaseFileLockParams,
): ReleaseFileLockResult {
  const {
    agentId = 'agent',
    sessionId = null,
    workspacePath = null,
    artifact = null,
    runId = null,
    targetFiles = [],
    status: statusArg = 'PENDING',
  } = params;

  const requestedStatus = String(statusArg);
  if (requestedStatus === 'SUCCESS') {
    throw new Error('file lock release cannot set SUCCESS; release as PENDING, then use verify mark with observed evidence');
  }
  if (!VALID_RELEASE_STATUSES.has(requestedStatus as FileLockReleaseStatus)) {
    throw new Error(`releaseFileLock status must be ACTIVE, PENDING, or FAILED; got "${statusArg}"`);
  }
  const effectiveStatus = requestedStatus as FileLockReleaseStatus;

  const now = utcNow();
  const whereClauses: string[] = ['ai.run_id = fl.run_id', 'ai.agent_id = ?'];
  const whereParams: (string | number)[] = [agentId];

  if (sessionId) {
    whereClauses.push('ai.session_id = ?');
    whereParams.push(sessionId);
  }
  const artifactScope = normalizeArtifact(artifact);
  if (workspacePath) {
    whereClauses.push('ai.workspace_path = ?');
    whereParams.push(workspaceScopeRoot(workspacePath));
  }
  if (artifactScope) {
    whereClauses.push('(ai.artifact = ? OR ai.artifact IS NULL)');
    whereParams.push(artifactScope);
  }

  if (runId) {
    whereClauses.push('fl.run_id = ?');
    whereParams.push(runId);
  }

  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => '?').join(',');
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }

  const where = whereClauses.join(' AND ');
  const locks = db.prepare(
    `SELECT fl.lock_id, fl.run_id, fl.file_path
       FROM awareness_locks fl JOIN task_runs ai ON ai.run_id = fl.run_id
      WHERE ${where}`
  ).all(...whereParams) as unknown as Array<{ lock_id: string; run_id: string; file_path: string }>;

  const runIds = [...new Set(locks.map(l => l.run_id))];
  if (runId && !runIds.includes(runId)) {
    const directWhere = ['run_id = ?', 'agent_id = ?'];
    const directParams: (string | number)[] = [runId, agentId];
    if (sessionId) { directWhere.push('session_id = ?'); directParams.push(sessionId); }
    if (workspacePath) { directWhere.push('workspace_path = ?'); directParams.push(workspaceScopeRoot(workspacePath)); }
    if (artifactScope) { directWhere.push('(artifact = ? OR artifact IS NULL)'); directParams.push(artifactScope); }
    const directRun = db.prepare(`SELECT run_id FROM task_runs WHERE ${directWhere.join(' AND ')}`)
      .get(...directParams) as { run_id: string } | undefined;
    if (directRun) runIds.push(directRun.run_id);
  }
  const ambiguousRelease = !runId && absFiles.length > 0 && runIds.length > 1;
  if (ambiguousRelease) {
    return {
      agent_id: agentId,
      status: effectiveStatus,
      released: false,
      locks_released: 0,
      run_ids: runIds,
      updated_at: now,
      ambiguousRelease: 'target-file release matched multiple active runs; pass --run-id to release exactly one run',
    };
  }

  if (runIds.length === 0) {
    return {
      agent_id: agentId,
      status: effectiveStatus,
      released: false,
      locks_released: 0,
      run_ids: [],
      updated_at: now,
    };
  }

  const runMetadata = db.prepare(`SELECT run_id, task_id, origin FROM task_runs
    WHERE run_id IN (${runIds.map(() => '?').join(',')})`)
    .all(...runIds) as unknown as Array<{ run_id: string; task_id: string | null; origin: 'TASK' | 'WORK' | 'HOOK' }>;
  if (effectiveStatus !== 'ACTIVE' && runMetadata.some((run) => run.task_id != null)) {
    throw new Error('task-linked runs must use task submit or task release; lock release may only keep them ACTIVE');
  }

  // Delete locks and settle their run in one transaction. beginWrite uses a
  // savepoint when a host already owns the outer write transaction.
  const transaction = beginWrite(db);
  let updatedRuns = 0;
  try {
    const lockIds = locks.map((lock) => lock.lock_id);
    if (lockIds.length > 0) {
      db.prepare(`DELETE FROM awareness_locks WHERE lock_id IN (${lockIds.map(() => '?').join(',')})`).run(...lockIds);
    }

    for (const tid of runIds) {
      const metadata = runMetadata.find((run) => run.run_id === tid);
      if (effectiveStatus !== 'ACTIVE' && metadata?.origin !== 'TASK') {
        const releasedFiles = locks.filter((lock) => lock.run_id === tid).map((lock) => lock.file_path);
        const fileClause = releasedFiles.length > 0
          ? ` AND file_path IN (${releasedFiles.map(() => '?').join(',')})`
          : '';
        db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
          WHERE run_id = ? AND ended_at IS NULL${fileClause}`)
          .run(now, now, now, tid, ...releasedFiles);
      }
      const remaining = db.prepare('SELECT 1 FROM awareness_locks WHERE run_id = ? LIMIT 1').get(tid);
      if (!remaining) {
        if (effectiveStatus !== 'ACTIVE' && metadata?.origin !== 'TASK') {
          db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
            WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, tid);
        }
        const updated = db.prepare(
          `UPDATE task_runs SET status = ?, updated_at = ?
           WHERE run_id = ? AND agent_id = ? AND status IN ('ACTIVE','PENDING')`
        ).run(effectiveStatus, now, tid, agentId) as { changes: number };
        updatedRuns += updated.changes;
      }
    }

    transaction.commit();
  } catch (e) {
    try { transaction.rollback(); } catch { /* transaction did not open */ }
    throw e;
  }

  return {
    agent_id: agentId,
    status: effectiveStatus,
    released: locks.length > 0 || updatedRuns > 0,
    locks_released: locks.length,
    run_ids: runIds,
    updated_at: now,
  };
}
