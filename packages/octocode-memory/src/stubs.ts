/**
 * stubs.ts — Implementations and stubs for commands not yet fully ported.
 *
 * pruneStale:       REAL — deletes expired file locks, sets affected intents to PENDING.
 * notifyGet:        STUB — returns empty inbox.
 * sessionCapture:   STUB — no-op.
 * waitForLock:      STUB — returns immediately.
 *
 * All stubs write "[stub] <command>: not yet implemented" to stderr.
 * TODO(RFC:octocode-memory): implement full versions.
 */

import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';

export interface PruneStaleResult {
  pruned_locks: number;
  updated_intents: number;
}

export interface NotifyGetResult {
  ok: true;
  count: 0;
  notifications: never[];
}

export interface SessionCaptureResult {
  ok: true;
  captured: false;
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: 0;
  lock_free: true;
}

/** REAL: Delete expired file locks and set parent intents to PENDING. */
export function pruneStale(db: DatabaseSync, _params: Record<string, unknown> = {}): PruneStaleResult {
  const now = utcNow();

  const expiredLocks = db.prepare(`
    SELECT fl.lock_id, fl.intent_id
    FROM file_locks fl
    WHERE fl.expires_at IS NOT NULL AND fl.expires_at < ?
  `).all(now) as Array<{ lock_id: string; intent_id: string }>;

  if (expiredLocks.length === 0) {
    return { pruned_locks: 0, updated_intents: 0 };
  }

  db.prepare(
    'DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?'
  ).run(now);

  const affectedIntentIds = [...new Set(expiredLocks.map(l => l.intent_id))];
  let updatedIntents = 0;
  for (const iid of affectedIntentIds) {
    const remaining = db.prepare('SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1').get(iid);
    if (!remaining) {
      const r = db.prepare(
        "UPDATE agent_intents SET status = 'PENDING', updated_at = ? WHERE intent_id = ? AND status = 'ACTIVE'"
      ).run(now, iid) as { changes: number };
      if (r.changes) updatedIntents++;
    }
  }

  return { pruned_locks: expiredLocks.length, updated_intents: updatedIntents };
}

/** STUB: Returns an empty notification inbox. */
export function notifyGet(
  _db: DatabaseSync,
  _params: Record<string, unknown> = {},
): NotifyGetResult {
  process.stderr.write('[stub] notify-get: not yet implemented; skipping\n');
  return { ok: true, count: 0, notifications: [] };
}

/** STUB: No-op session capture. */
export function sessionCapture(
  _db: DatabaseSync,
  _params: Record<string, unknown> = {},
): SessionCaptureResult {
  process.stderr.write('[stub] session-capture: not yet implemented; skipping\n');
  return { ok: true, captured: false };
}

/** STUB: Returns immediately (lock considered free). */
export function waitForLock(
  _db: DatabaseSync,
  _params: Record<string, unknown> = {},
): WaitForLockResult {
  process.stderr.write('[stub] wait-for-lock: not yet implemented; returning immediately\n');
  return { ok: true, waited_ms: 0, lock_free: true };
}
