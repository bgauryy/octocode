/**
 * intents.ts — File-lock intent operations.
 */

import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
import type {
  PreFlightIntentParams, PreFlightIntentResult,
  ReleaseFileLockParams, ReleaseFileLockResult,
  FileLockRow,
} from './types.js';

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
    workspacePath,
    rationale = 'agent write operation',
    testPlan = 'post-edit verification',
    targetFiles = [],
    lockType = 'EXCLUSIVE',
    ttlMs = 10 * 60_000,
  } = params;

  const maxTtlMs = 10 * 60_000;
  const effectiveTtlMs = Math.min(Math.max(1, ttlMs ?? maxTtlMs), maxTtlMs);
  const intentId = 'intent_' + randomUUID().replace(/-/g, '');
  const now = utcNow();
  const wsPath = workspacePath ?? process.cwd();
  const absFiles = targetFiles.map(f => resolve(f));

  // Drop expired locks before checking conflicts so dangling locks never block new work.
  db.prepare('DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at <= ?').run(now);

  // Check for conflicts (EXCLUSIVE locks on these files by other agents)
  const conflicts: FileLockRow[] = [];
  for (const absPath of absFiles) {
    const existing = db.prepare(`
      SELECT fl.*, ai.agent_id AS intent_agent_id FROM file_locks fl
      JOIN agent_intents ai ON ai.intent_id = fl.intent_id
      WHERE fl.file_path = ?
        AND ai.agent_id <> ?
        AND ai.status = 'ACTIVE'
        AND fl.lock_type = 'EXCLUSIVE'
        AND (fl.expires_at IS NULL OR fl.expires_at > ?)
    `).all(absPath, agentId, now) as unknown as FileLockRow[];
    conflicts.push(...existing);
  }

  if (conflicts.length > 0) {
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

  db.prepare(`
    INSERT INTO agent_intents
      (intent_id, agent_id, rationale, test_plan, status, workspace_path, files_json, created_at, updated_at)
    VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)
  `).run(intentId, agentId, rationale, testPlan, wsPath, JSON.stringify(absFiles), now, now);

  const expiresAt = new Date(Date.now() + effectiveTtlMs).toISOString().replace(/\.\d{3}Z$/, 'Z');

  const acquiredLocks: Array<{ lock_id: string; file_path: string; lock_type: 'EXCLUSIVE' | 'SHARED'; expires_at: string | null }> = [];
  for (const absPath of absFiles) {
    const lockId = 'lock_' + randomUUID().replace(/-/g, '');
    db.prepare(`
      INSERT OR REPLACE INTO file_locks
        (lock_id, file_path, intent_id, agent_id, lock_type, acquired_at, expires_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run(lockId, absPath, intentId, agentId, lockType, now, expiresAt);
    acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type: lockType, expires_at: expiresAt });
  }

  return {
    ok: true,
    intent: {
      intent_id: intentId,
      agent_id: agentId,
      lock_type: lockType,
      workspace_path: wsPath,
      target_files: absFiles,
      locks: acquiredLocks.map(l => ({
        lock_id: l.lock_id,
        file_path: l.file_path,
        lock_type: l.lock_type,
        agent_id: agentId,
        acquired_at: now,
        expires_at: l.expires_at,
      })),
      status: 'ACTIVE',
      created_at: now,
    },
  };
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

  if (intentId) {
    whereClauses.push('fl.intent_id = ?');
    whereParams.push(intentId);
  }

  const absFiles = targetFiles.map(f => resolve(f));
  if (absFiles.length > 0) {
    const ph = absFiles.map(() => '?').join(',');
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...absFiles);
  }

  const where = whereClauses.join(' AND ');
  const locks = db.prepare(
    `SELECT fl.lock_id, fl.intent_id, fl.file_path FROM file_locks fl WHERE ${where}`
  ).all(...whereParams) as unknown as Array<{ lock_id: string; intent_id: string; file_path: string }>;

  const deleteWhere = where.replace(/\bfl\./g, '');
  db.prepare(`DELETE FROM file_locks WHERE ${deleteWhere}`).run(...whereParams);

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
