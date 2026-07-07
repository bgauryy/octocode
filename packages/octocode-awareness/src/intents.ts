/**
 * intents.ts — File-lock intent operations.
 */

import { randomUUID } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
import { evictExpiredLocks } from './db.js';
import type {
  PreFlightIntentParams, PreFlightIntentResult,
  ReleaseFileLockParams, ReleaseFileLockResult,
  FileLockRow,
  FileLockParams,
  FileLockResult,
  FileLockStatusEntry,
} from './types.js';

const MAX_LOCK_TTL_MS = 10 * 60_000;

function effectiveTtlMs(ttlMs: number | null | undefined): number {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}

function expiresAtFromNow(ttlMs: number | null | undefined): string {
  return new Date(Date.now() + effectiveTtlMs(ttlMs)).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function workspaceRoot(workspacePath?: string | null): string {
  return workspacePath ? resolve(workspacePath) : process.cwd();
}

function resolveTargetFiles(targetFiles: string[] = [], workspacePath?: string | null): string[] {
  const root = workspaceRoot(workspacePath);
  return targetFiles.map((file) => isAbsolute(file) ? resolve(file) : resolve(root, file));
}

function activeLockRows(
  db: DatabaseSync,
  params: { workspacePath?: string | null; agentId?: string | null; sessionId?: string | null; intentId?: string | null } = {},
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
    binds.push(workspaceRoot(params.workspacePath));
  }
  if (params.agentId) {
    clauses.push('ai.agent_id = ?');
    binds.push(params.agentId);
  }
  if (params.sessionId) {
    clauses.push('ai.session_id = ?');
    binds.push(params.sessionId);
  }
  if (params.intentId) {
    clauses.push('fl.intent_id = ?');
    binds.push(params.intentId);
  }

  return db.prepare(
    `SELECT fl.lock_id, fl.intent_id, fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path,
            ai.rationale AS reasoning, fl.lock_type, fl.acquired_at, fl.expires_at
       FROM file_locks fl
       JOIN agent_intents ai ON ai.intent_id = fl.intent_id
      WHERE ${clauses.join(' AND ')}
      ORDER BY fl.acquired_at DESC`
  ).all(...binds) as unknown as FileLockStatusEntry[];
}

/**
 * Claim file locks for an agent write operation.
 * Returns { ok: true, intent } on success or { ok: false, conflict, conflicts } on conflict.
 */
export function preFlightIntent(
  db: DatabaseSync,
  params: PreFlightIntentParams,
): PreFlightIntentResult {
  const {
    agentId = 'agent',
    sessionId = null,
    workspacePath,
    rationale = 'agent write operation',
    testPlan = 'post-edit verification',
    planDocRef = null,
    targetFiles = [],
    lockType = 'EXCLUSIVE',
    ttlMs = MAX_LOCK_TTL_MS,
  } = params;

  const intentId = 'intent_' + randomUUID().replace(/-/g, '');
  const now = utcNow();
  const wsPath = workspaceRoot(workspacePath);
  const absFiles = resolveTargetFiles(targetFiles, wsPath);

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
        SELECT fl.*, ai.agent_id AS intent_agent_id FROM file_locks fl
        JOIN agent_intents ai ON ai.intent_id = fl.intent_id
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
          agent_id: c.intent_agent_id ?? c.agent_id,
          acquired_at: c.acquired_at,
          expires_at: c.expires_at,
        })),
      };
    }

    // Insert intent + all file locks atomically within the same transaction.
    db.prepare(`
      INSERT INTO agent_intents
        (intent_id, agent_id, session_id, rationale, test_plan, plan_doc_ref, status, workspace_path, files_json, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)
    `).run(intentId, agentId, sessionId, rationale, testPlan, planDocRef, wsPath, JSON.stringify(absFiles), now, now);

    const expiresAt = expiresAtFromNow(ttlMs);

    const acquiredLocks: Array<{ lock_id: string; file_path: string; lock_type: 'EXCLUSIVE' | 'SHARED'; expires_at: string | null }> = [];
    for (const absPath of absFiles) {
      const lockId = 'lock_' + randomUUID().replace(/-/g, '');
      db.prepare(`
        INSERT OR REPLACE INTO file_locks
          (lock_id, file_path, intent_id, agent_id, session_id, lock_type, acquired_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(lockId, absPath, intentId, agentId, sessionId, lockType, now, expiresAt);
      acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
    }

    db.exec('COMMIT');

    return {
      ok: true,
      intent: {
        intent_id: intentId,
        agent_id: agentId,
        session_id: sessionId,
        lock_type: lockType,
        workspace_path: wsPath,
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
 * Release file locks for an intent or specific files.
 */
export function releaseFileLock(
  db: DatabaseSync,
  params: ReleaseFileLockParams,
): ReleaseFileLockResult {
  const {
    agentId = 'agent',
    sessionId = null,
    workspacePath = null,
    intentId = null,
    targetFiles = [],
    status: statusArg = 'SUCCESS',
    verified = false,
    verifiedNote,
  } = params;

  const requestedSuccessWithoutVerification = statusArg === 'SUCCESS' && !verified;
  const effectiveStatus: ReleaseFileLockParams['status'] = verified
    ? 'SUCCESS'
    : requestedSuccessWithoutVerification
      ? 'PENDING'
      : statusArg;

  const now = utcNow();
  const whereClauses: string[] = ['fl.agent_id = ?'];
  const whereParams: (string | number)[] = [agentId];

  if (sessionId) {
    whereClauses.push('fl.session_id = ?');
    whereParams.push(sessionId);
  }

  if (intentId) {
    whereClauses.push('fl.intent_id = ?');
    whereParams.push(intentId);
  }

  const absFiles = resolveTargetFiles(targetFiles, workspacePath);
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => '?').join(',');
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }

  const where = whereClauses.join(' AND ');
  const locks = db.prepare(
    `SELECT fl.lock_id, fl.intent_id, fl.file_path FROM file_locks fl WHERE ${where}`
  ).all(...whereParams) as unknown as Array<{ lock_id: string; intent_id: string; file_path: string }>;

  // INT-2: Build the DELETE WHERE clause independently instead of string-replacing
  // the SELECT WHERE clause to strip the 'fl.' table alias. String-replace is
  // fragile: a bind value containing 'fl.' would silently corrupt the query.
  const deleteClauses: string[] = ['agent_id = ?'];
  const deleteParams: (string | number)[] = [agentId];
  if (sessionId) { deleteClauses.push('session_id = ?'); deleteParams.push(sessionId); }
  if (intentId) { deleteClauses.push('intent_id = ?'); deleteParams.push(intentId); }
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => '?').join(',');
    deleteClauses.push(`file_path IN (${ph})`);
    deleteParams.push(...absFiles);
  }
  db.prepare(`DELETE FROM file_locks WHERE ${deleteClauses.join(' AND ')}`).run(...deleteParams);

  const intentIds = [...new Set([
    ...(intentId ? [intentId] : []),
    ...locks.map(l => l.intent_id),
  ])];

  for (const iid of intentIds) {
    const remaining = db.prepare('SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1').get(iid);
    if (!remaining) {
      db.prepare(
        'UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND agent_id = ?'
      ).run(effectiveStatus, now, iid, agentId);
      if (verified && verifiedNote) {
        try {
          db.prepare(
            `INSERT INTO intent_events(event_id, intent_id, agent_id, event_type, message, created_at)
             VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
          ).run('evt_' + randomUUID().replace(/-/g, ''), iid, agentId, verifiedNote, now);
        } catch { /* intent_events may not exist on older DBs */ }
      }
    }
  }

  return {
    agent_id: agentId,
    status: effectiveStatus as 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED',
    released: locks.length > 0 || Boolean(intentId),
    locks_released: locks.length,
    intent_ids: intentIds,
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
        targetFiles: params.targetFiles ?? [],
        lockType: params.lockType,
        ttlMs: params.ttlMs,
        rationale: params.reasoning?.trim() || 'manual: fileLock lock',
        testPlan: 'release or verify fileLock intent',
      });
      if (!result.ok) return { ok: false, type: 'lock', conflict: true, conflicts: result.conflicts };
      const locks = activeLockRows(db, { intentId: result.intent.intent_id });
      return {
        ok: true,
        type: 'lock',
        intentId: result.intent.intent_id,
        files: result.intent.target_files,
        reasoning: params.reasoning?.trim() || 'manual: fileLock lock',
        acquiredAt: result.intent.locks[0]?.acquired_at ?? null,
        expiresAt: result.intent.locks[0]?.expires_at ?? null,
        locks,
      };
    }
    case 'release': {
      if (!params.intentId && (!params.targetFiles || params.targetFiles.length === 0)) {
        throw new Error('fileLock release requires intentId or targetFiles');
      }
      const rel = releaseFileLock(db, {
        agentId: params.agentId,
        sessionId: params.sessionId,
        workspacePath: params.workspacePath,
        intentId: params.intentId,
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
          agentId: params.agentId,
          sessionId: params.sessionId,
          intentId: params.intentId,
        }),
      };
    case 'renew': {
      if (!params.intentId) throw new Error('fileLock renew requires intentId');
      const agentId = params.agentId ?? 'agent';
      const expiresAt = expiresAtFromNow(params.ttlMs);
      const res = db.prepare(
        `UPDATE file_locks SET expires_at = ? WHERE intent_id = ? AND agent_id = ?`
      ).run(expiresAt, params.intentId, agentId) as { changes: number };
      db.prepare('UPDATE agent_intents SET updated_at = ? WHERE intent_id = ? AND agent_id = ?')
        .run(utcNow(), params.intentId, agentId);
      return {
        ok: true,
        type: 'renew',
        intentId: params.intentId,
        renewed: res.changes > 0,
        locks_renewed: res.changes,
        expiresAt: res.changes > 0 ? expiresAt : null,
      };
    }
  }
}
