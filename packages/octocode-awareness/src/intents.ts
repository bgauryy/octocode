/**
 * intents.ts — execution-run and file-lock operations.
 */

import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { evictExpiredLocks } from './db.js';
import { canonicalizePath, normalizeWorkspacePath } from './git.js';
import { renewWorkLease, startWork } from './work.js';
import type {
  PreFlightRunParams, PreFlightRunResult,
  ReleaseFileLockParams, ReleaseFileLockResult,
  FileLockParams,
  FileLockResult,
  FileLockStatusEntry,
} from './types.js';

const MAX_LOCK_TTL_MS = 10 * 60_000;
const VALID_RELEASE_STATUSES = new Set(['PENDING', 'ACTIVE', 'SUCCESS', 'FAILED']);
type ReleaseStatus = 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';

function effectiveTtlMs(ttlMs: number | null | undefined): number {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}

function workspaceScopeRoot(workspacePath?: string | null): string {
  const candidate = workspacePath ?? process.cwd();
  return normalizeWorkspacePath(candidate, candidate) ?? resolve(candidate);
}

function workspaceFileBase(workspacePath?: string | null): string {
  return workspacePath ? resolve(workspacePath) : process.cwd();
}

function resolveTargetFiles(targetFiles: string[] = [], workspacePath?: string | null): string[] {
  const root = workspaceFileBase(workspacePath);
  return targetFiles.map((file) => canonicalizePath(isAbsolute(file) ? resolve(file) : resolve(root, file)));
}

function activeLockRows(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; agentId?: string | null; sessionId?: string | null; runId?: string | null } = {},
): FileLockStatusEntry[] {
  // ARCH-3: Delegate eviction to the shared evictExpiredLocks instead of
  // duplicating the DELETE. Note: eviction here is intentional — stale locks
  // must be cleared before the caller decides whether a file is locked.
  evictExpiredLocks(db);
  const now = utcNow(); // re-read after eviction so the SELECT filter is consistent

  const clauses = ["ai.status = 'ACTIVE'", "(fl.expires_at IS NULL OR fl.expires_at > ?)"];
  const binds: (string | number)[] = [now];
  if (params.workspacePath) {
    clauses.push('ai.workspace_path = ?');
    binds.push(workspaceScopeRoot(params.workspacePath));
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    clauses.push('(ai.artifact = ? OR ai.artifact IS NULL)');
    binds.push(artifact);
  }
  if (params.agentId) {
    clauses.push('ai.agent_id = ?');
    binds.push(params.agentId);
  }
  if (params.sessionId) {
    clauses.push('ai.session_id = ?');
    binds.push(params.sessionId);
  }
  if (params.runId) {
    clauses.push('fl.run_id = ?');
    binds.push(params.runId);
  }

  return db.prepare(
    `SELECT fl.lock_id, fl.run_id, fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact,
            ai.rationale AS reasoning, ai.test_plan AS test_plan, 'EXCLUSIVE' AS lock_type,
            fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN task_runs ai ON ai.run_id = fl.run_id
      WHERE ${clauses.join(' AND ')}
      ORDER BY fl.acquired_at DESC`
  ).all(...binds) as unknown as FileLockStatusEntry[];
}

/**
 * Claim file locks for an agent write operation.
 * Returns { ok: true, run } on success or { ok: false, conflict, conflicts } on conflict.
 */
export function preFlightIntent(
  db: DatabaseSync,
  params: PreFlightRunParams,
): PreFlightRunResult {
  const agentId = params.agentId ?? 'agent';
  const result = startWork(db, {
    agentId,
    sessionId: params.sessionId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    runId: params.runId,
    rationale: params.rationale ?? 'agent write operation',
    testPlan: params.testPlan ?? 'post-edit verification',
    contextRef: params.contextRef,
    targetFiles: params.targetFiles ?? [],
    origin: 'WORK',
    source: 'EXPLICIT',
    ttlMs: effectiveTtlMs(params.ttlMs),
    exclusive: true,
  });
  if (!result.ok) {
    return {
      ok: false,
      conflict: true,
      conflicts: result.conflicts.map((conflict) => {
        const holder = db.prepare(`SELECT tr.session_id, s.ended_at, l.acquired_at
          FROM task_runs tr
          LEFT JOIN sessions s ON s.session_id = tr.session_id
          LEFT JOIN locks l ON l.run_id = tr.run_id AND l.file_path = ?
          WHERE tr.run_id = ?`).get(conflict.file_path, conflict.run_id) as {
            session_id: string | null;
            ended_at: string | null;
            acquired_at: string | null;
          } | undefined;
        return {
          file_path: conflict.file_path,
          lock_type: 'EXCLUSIVE' as const,
          agent_id: conflict.agent_id,
          acquired_at: holder?.acquired_at ?? conflict.heartbeat_at,
          expires_at: conflict.expires_at,
          run_id: conflict.run_id,
          reasoning: conflict.rationale,
          test_plan: db.prepare('SELECT test_plan FROM task_runs WHERE run_id = ?')
            .get(conflict.run_id)?.['test_plan'] as string ?? 'post-edit verification',
          session_id: holder?.session_id ?? null,
          holder_session_active: !holder?.ended_at,
        };
      }),
    };
  }
  const locks = activeLockRows(db, { runId: result.run.run_id });
  return {
    ok: true,
    run: {
      run_id: result.run.run_id,
      task_id: result.run.task_id,
      origin: result.run.origin,
      agent_id: result.run.agent_id,
      session_id: result.run.session_id,
      workspace_path: result.run.workspace_path ?? workspaceScopeRoot(params.workspacePath),
      artifact: result.run.artifact,
      context_ref: result.run.context_ref,
      target_files: result.files.filter((file) => file.ended_at == null).map((file) => file.file_path),
      locks: locks.map((lock) => ({
        lock_id: lock.lock_id,
        file_path: lock.file_path,
        lock_type: 'EXCLUSIVE',
        agent_id: lock.agent_id,
        session_id: lock.session_id,
        acquired_at: lock.acquired_at,
        expires_at: lock.expires_at,
      })),
      status: result.run.status,
      created_at: result.run.created_at,
    },
  };
}

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
    status: statusArg = 'SUCCESS',
  } = params;

  if (!VALID_RELEASE_STATUSES.has(String(statusArg))) {
    throw new Error(`releaseFileLock status must be ACTIVE, PENDING, SUCCESS, or FAILED; got "${statusArg}"`);
  }
  const requestedStatus = String(statusArg) as ReleaseStatus;
  // Lock release ends editing; it never certifies the work. All SUCCESS
  // transitions go through markVerified so evidence and linked-task closure
  // share one policy and one audit receipt.
  const requestedDirectSuccess = requestedStatus === 'SUCCESS';
  const effectiveStatus: ReleaseStatus = requestedDirectSuccess ? 'PENDING' : requestedStatus;

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
       FROM locks fl JOIN task_runs ai ON ai.run_id = fl.run_id
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

  // FIX #3 (P0): Wrap DELETE from locks AND UPDATE task_runs status in a single atomic transaction
  // so a crash between the two statements never leaves orphaned lock rows with no task update.
  db.exec('BEGIN IMMEDIATE');
  let updatedRuns = 0;
  try {
    const lockIds = locks.map((lock) => lock.lock_id);
    if (lockIds.length > 0) {
      db.prepare(`DELETE FROM locks WHERE lock_id IN (${lockIds.map(() => '?').join(',')})`).run(...lockIds);
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
      const remaining = db.prepare('SELECT 1 FROM locks WHERE run_id = ? LIMIT 1').get(tid);
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

    db.exec('COMMIT');
  } catch (e) {
    try { db.exec('ROLLBACK'); } catch { /* not in transaction */ }
    throw e;
  }

  return {
    agent_id: agentId,
    status: effectiveStatus,
    released: locks.length > 0 || updatedRuns > 0,
    locks_released: locks.length,
    run_ids: runIds,
    updated_at: now,
    ...(requestedDirectSuccess
      ? { unverifiedConclusion: 'Direct SUCCESS on lock release is not allowed; stored as PENDING until verify mark records an evidence receipt.' }
      : {}),
  };
}

export function fileLock(db: DatabaseSync, params: FileLockParams): FileLockResult {
  switch (params.type) {
    case 'lock': {
      const result = preFlightIntent(db, {
        agentId: params.agentId,
        sessionId: params.sessionId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        runId: params.runId,
        targetFiles: params.targetFiles ?? [],
        ttlMs: params.ttlMs,
        rationale: params.reasoning?.trim() || 'manual: fileLock lock',
        testPlan: 'release or verify file-lock run',
      });
      if (!result.ok) return { ok: false, type: 'lock', conflict: true, conflicts: result.conflicts };
      const locks = activeLockRows(db, { runId: result.run.run_id });
      return {
        ok: true,
        type: 'lock',
        runId: result.run.run_id,
        files: result.run.target_files,
        reasoning: params.reasoning?.trim() || 'manual: fileLock lock',
        acquiredAt: result.run.locks[0]?.acquired_at ?? null,
        expiresAt: result.run.locks[0]?.expires_at ?? null,
        locks,
      };
    }
    case 'release': {
      if (!params.runId && (!params.targetFiles || params.targetFiles.length === 0)) {
        throw new Error('fileLock release requires runId or targetFiles');
      }
      const rel = releaseFileLock(db, {
        agentId: params.agentId,
        sessionId: params.sessionId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        runId: params.runId,
        targetFiles: params.targetFiles,
        status: params.status,
        verified: params.verified,
        verifiedNote: params.verifiedNote,
      });
      return {
        ok: !('unverifiedConclusion' in rel),
        type: 'release',
        ...rel,
      };
    }
    case 'status':
      return {
        ok: true,
        type: 'status',
        locks: activeLockRows(db, {
          workspacePath: params.workspacePath,
          artifact: params.artifact,
          agentId: params.agentId,
          sessionId: params.sessionId,
          runId: params.runId,
        }),
      };
    case 'renew': {
      if (!params.runId) throw new Error('fileLock renew requires runId');
      const agentId = params.agentId ?? 'agent';
      const renewed = renewWorkLease(db, {
        agentId, runId: params.runId, ttlMs: params.ttlMs,
      }, { exclusiveOnly: true });
      return {
        ok: true,
        type: 'renew',
        runId: params.runId,
        renewed: renewed.locksRenewed > 0,
        locks_renewed: renewed.locksRenewed,
        expiresAt: renewed.expiresAt,
      };
    }
  }
}
