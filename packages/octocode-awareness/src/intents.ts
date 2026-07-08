/**
 * intents.ts — edit task + file-lock operations.
 */

import { randomUUID } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { evictExpiredLocks } from './db.js';
import { normalizeWorkspacePath } from './git.js';
import type {
  PreFlightTaskParams, PreFlightTaskResult,
  ReleaseFileLockParams, ReleaseFileLockResult,
  FileLockRow,
  FileLockParams,
  FileLockResult,
  FileLockStatusEntry,
} from './types.js';

const MAX_LOCK_TTL_MS = 10 * 60_000;
const VALID_RELEASE_STATUSES = new Set(['PENDING', 'SUCCESS', 'FAILED']);
type ReleaseStatus = 'PENDING' | 'SUCCESS' | 'FAILED';

function effectiveTtlMs(ttlMs: number | null | undefined): number {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}

function expiresAtFromNow(ttlMs: number | null | undefined): string {
  return new Date(Date.now() + effectiveTtlMs(ttlMs)).toISOString().replace(/\.\d{3}Z$/, 'Z');
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
  return targetFiles.map((file) => isAbsolute(file) ? resolve(file) : resolve(root, file));
}

function activeLockRows(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; agentId?: string | null; sessionId?: string | null; taskId?: string | null } = {},
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
  if (params.taskId) {
    clauses.push('fl.task_id = ?');
    binds.push(params.taskId);
  }

  return db.prepare(
    `SELECT fl.lock_id, fl.task_id, fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact,
            ai.rationale AS reasoning, fl.lock_type, fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN tasks ai ON ai.task_id = fl.task_id
      WHERE ${clauses.join(' AND ')}
      ORDER BY fl.acquired_at DESC`
  ).all(...binds) as unknown as FileLockStatusEntry[];
}

/**
 * Claim file locks for an agent write operation.
 * Returns { ok: true, task } on success or { ok: false, conflict, conflicts } on conflict.
 */
export function preFlightIntent(
  db: DatabaseSync,
  params: PreFlightTaskParams,
): PreFlightTaskResult {
  const {
    agentId = 'agent',
    sessionId = null,
    workspacePath,
    artifact,
    rationale = 'agent write operation',
    testPlan = 'post-edit verification',
    planDocRef = null,
    targetFiles = [],
    lockType = 'EXCLUSIVE',
    ttlMs = MAX_LOCK_TTL_MS,
  } = params;
  const taskId = 'task_' + randomUUID().replace(/-/g, '');
  const now = utcNow();
  const wsPath = workspaceScopeRoot(workspacePath);
  const artifactScope = normalizeArtifact(artifact);
  const absFiles = resolveTargetFiles(targetFiles, workspacePath);

  // ARCH-3: Drop expired locks before checking conflicts so dangling locks never block new work.
  evictExpiredLocks(db);

  // BEGIN IMMEDIATE acquires a write lock upfront, serializing the check-then-insert sequence
  // and eliminating the TOCTOU race where two agents both pass the conflict check before either
  // inserts, then both hold EXCLUSIVE locks on the same file.
  db.exec('BEGIN IMMEDIATE');
  try {
    // Check for conflicts with OTHER agents.
    const conflicts: FileLockRow[] = [];
    for (const absPath of absFiles) {
      const conflictMode = lockType === 'SHARED' ? "fl.lock_type = 'EXCLUSIVE'" : '1 = 1';
      const existing = db.prepare(`
        SELECT fl.*, ai.agent_id AS task_agent_id FROM locks fl
        JOIN tasks ai ON ai.task_id = fl.task_id
        WHERE fl.file_path = ?
          AND ai.agent_id <> ?
          AND ai.status = 'ACTIVE'
          AND ${conflictMode}
          AND (fl.expires_at IS NULL OR fl.expires_at > ?)
      `).all(absPath, agentId, now) as unknown as FileLockRow[];
      conflicts.push(...existing);
    }

    if (conflicts.length > 0) {
      db.exec('ROLLBACK');
      return {
        ok: false,
        conflict: true,
        conflicts: conflicts.map(c => ({
          file_path: c.file_path,
          lock_type: c.lock_type as 'EXCLUSIVE' | 'SHARED',
          agent_id: c.task_agent_id ?? c.agent_id,
          acquired_at: c.acquired_at,
          expires_at: c.expires_at,
        })),
      };
    }

    // Auto-register session when provided so the FK on tasks.session_id is satisfied.
    if (sessionId) {
      db.prepare(
        `INSERT OR IGNORE INTO sessions (session_id, agent_id, workspace_path, artifact, started_at)
         VALUES (?, ?, ?, ?, ?)`
      ).run(sessionId, agentId, wsPath, artifactScope, now);
    }

    // Insert task + all file locks atomically within the same transaction.
    db.prepare(`
      INSERT INTO tasks
        (task_id, agent_id, session_id, rationale, test_plan, plan_doc_ref, status, workspace_path, artifact, files_json, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?)
    `).run(taskId, agentId, sessionId, rationale, testPlan, planDocRef, wsPath, artifactScope, JSON.stringify(absFiles), now, now);

    const expiresAt = expiresAtFromNow(ttlMs);

    const acquiredLocks: Array<{ lock_id: string; file_path: string; lock_type: 'EXCLUSIVE' | 'SHARED'; expires_at: string | null }> = [];
    for (const absPath of absFiles) {
      const lockId = 'lock_' + randomUUID().replace(/-/g, '');
      db.prepare(`
        INSERT OR REPLACE INTO locks
          (lock_id, file_path, task_id, agent_id, session_id, lock_type, acquired_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(lockId, absPath, taskId, agentId, sessionId, lockType, now, expiresAt);
      acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
    }

    db.exec('COMMIT');

    return {
      ok: true,
      task: {
        task_id: taskId,
        agent_id: agentId,
        session_id: sessionId,
        lock_type: lockType,
        workspace_path: wsPath,
        artifact: artifactScope,
        plan_doc_ref: planDocRef,
        target_files: absFiles,
        locks: acquiredLocks.map(l => ({
          lock_id: l.lock_id,
          file_path: l.file_path,
          lock_type: l.lock_type,
          agent_id: agentId,
          session_id: sessionId,
          acquired_at: now,
          expires_at: l.expires_at,
        })),
        status: 'ACTIVE',
        created_at: now,
      },
    };
  } catch (e) {
    try { db.exec('ROLLBACK'); } catch { /* not in transaction */ }
    throw e;
  }
}

/**
 * Release file locks for a task or specific files.
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
    taskId = null,
    targetFiles = [],
    status: statusArg = 'SUCCESS',
    verified = false,
    verifiedNote,
  } = params;

  if (!VALID_RELEASE_STATUSES.has(String(statusArg))) {
    throw new Error(`releaseFileLock status must be PENDING, SUCCESS, or FAILED; got "${statusArg}"`);
  }
  const requestedStatus = String(statusArg) as ReleaseStatus;
  const requestedSuccessWithoutVerification = requestedStatus === 'SUCCESS' && !verified;
  const effectiveStatus: ReleaseStatus = requestedSuccessWithoutVerification ? 'PENDING' : requestedStatus;

  const now = utcNow();
  const whereClauses: string[] = ['fl.agent_id = ?'];
  const whereParams: (string | number)[] = [agentId];

  if (sessionId) {
    whereClauses.push('fl.session_id = ?');
    whereParams.push(sessionId);
  }
  const artifactScope = normalizeArtifact(artifact);
  if (workspacePath || artifactScope) {
    whereClauses.push('ai.task_id = fl.task_id');
  }
  if (workspacePath) {
    whereClauses.push('ai.workspace_path = ?');
    whereParams.push(workspaceScopeRoot(workspacePath));
  }
  if (artifactScope) {
    whereClauses.push('(ai.artifact = ? OR ai.artifact IS NULL)');
    whereParams.push(artifactScope);
  }

  if (taskId) {
    whereClauses.push('fl.task_id = ?');
    whereParams.push(taskId);
  }

  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => '?').join(',');
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }

  const where = whereClauses.join(' AND ');
  const locks = db.prepare(
    `SELECT fl.lock_id, fl.task_id, fl.file_path
       FROM locks fl${workspacePath || artifactScope ? ', tasks ai' : ''}
      WHERE ${where}`
  ).all(...whereParams) as unknown as Array<{ lock_id: string; task_id: string; file_path: string }>;

  // INT-2: Build the DELETE WHERE clause independently instead of string-replacing
  // the SELECT WHERE clause to strip the 'fl.' table alias. String-replace is
  // fragile: a bind value containing 'fl.' would silently corrupt the query.
  const deleteClauses: string[] = ['agent_id = ?'];
  const deleteParams: (string | number)[] = [agentId];
  if (sessionId) { deleteClauses.push('session_id = ?'); deleteParams.push(sessionId); }
  if (taskId) { deleteClauses.push('task_id = ?'); deleteParams.push(taskId); }
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => '?').join(',');
    deleteClauses.push(`file_path IN (${ph})`);
    deleteParams.push(...absFiles);
  }
  const taskIds = [...new Set([
    ...(taskId ? [taskId] : []),
    ...locks.map(l => l.task_id),
  ])];

  // FIX #3 (P0): Wrap DELETE from locks AND UPDATE tasks status in a single atomic transaction
  // so a crash between the two statements never leaves orphaned lock rows with no task update.
  db.exec('BEGIN IMMEDIATE');
  try {
    const lockIds = locks.map((lock) => lock.lock_id);
    if (lockIds.length > 0) {
      db.prepare(`DELETE FROM locks WHERE lock_id IN (${lockIds.map(() => '?').join(',')})`).run(...lockIds);
    } else if (taskId && !workspacePath && !artifactScope) {
      db.prepare(`DELETE FROM locks WHERE ${deleteClauses.join(' AND ')}`).run(...deleteParams);
    }

    for (const tid of taskIds) {
      const remaining = db.prepare('SELECT 1 FROM locks WHERE task_id = ? LIMIT 1').get(tid);
      if (!remaining) {
        db.prepare(
          'UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ? AND agent_id = ?'
        ).run(effectiveStatus, now, tid, agentId);
        if (verified && verifiedNote) {
          try {
            db.prepare(
              `INSERT INTO task_log(event_id, task_id, agent_id, event_type, message, created_at)
               VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
            ).run('evt_' + randomUUID().replace(/-/g, ''), tid, agentId, verifiedNote, now);
          } catch { /* non-critical audit log */ }
        }
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
    released: locks.length > 0 || Boolean(taskId),
    locks_released: locks.length,
    task_ids: taskIds,
    updated_at: now,
    ...(requestedSuccessWithoutVerification
      ? { unverifiedConclusion: 'SUCCESS requested without --verified; stored as PENDING until verify records the test result.' }
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
        targetFiles: params.targetFiles ?? [],
        lockType: params.lockType,
        ttlMs: params.ttlMs,
        rationale: params.reasoning?.trim() || 'manual: fileLock lock',
        testPlan: 'release or verify fileLock task',
      });
      if (!result.ok) return { ok: false, type: 'lock', conflict: true, conflicts: result.conflicts };
      const locks = activeLockRows(db, { taskId: result.task.task_id });
      return {
        ok: true,
        type: 'lock',
        taskId: result.task.task_id,
        files: result.task.target_files,
        reasoning: params.reasoning?.trim() || 'manual: fileLock lock',
        acquiredAt: result.task.locks[0]?.acquired_at ?? null,
        expiresAt: result.task.locks[0]?.expires_at ?? null,
        locks,
      };
    }
    case 'release': {
      if (!params.taskId && (!params.targetFiles || params.targetFiles.length === 0)) {
        throw new Error('fileLock release requires taskId or targetFiles');
      }
      const rel = releaseFileLock(db, {
        agentId: params.agentId,
        sessionId: params.sessionId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        taskId: params.taskId,
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
          taskId: params.taskId,
        }),
      };
    case 'renew': {
      if (!params.taskId) throw new Error('fileLock renew requires taskId');
      const agentId = params.agentId ?? 'agent';
      const expiresAt = expiresAtFromNow(params.ttlMs);
      const res = db.prepare(
        `UPDATE locks SET expires_at = ? WHERE task_id = ? AND agent_id = ?`
      ).run(expiresAt, params.taskId, agentId) as { changes: number };
      db.prepare('UPDATE tasks SET updated_at = ? WHERE task_id = ? AND agent_id = ?')
        .run(utcNow(), params.taskId, agentId);
      return {
        ok: true,
        type: 'renew',
        taskId: params.taskId,
        renewed: res.changes > 0,
        locks_renewed: res.changes,
        expiresAt: res.changes > 0 ? expiresAt : null,
      };
    }
  }
}
