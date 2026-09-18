import type { DatabaseSync } from 'node:sqlite';
import { SIGNALS_DELETE_BY_IDS } from './sql/signals.js';
import type { NotificationKind } from './types/notifications-agents.js';
import { utcNow } from './helpers.js';

/** Canonical message retention policy. All signal expiry derives from this map. */
export const MESSAGE_RETENTION_DAYS: Readonly<Record<NotificationKind, number>> = Object.freeze({
  claim: 14,
  handoff: 30,
  question: 14,
  reply: 30,
  blocker: 30,
  request: 14,
  decision: 90,
  approval: 30,
  fyi: 7,
});
export const MESSAGE_PRUNE_GRACE_DAYS = 7;
export const MESSAGE_MAX_PRUNE_GRACE_DAYS = 30;
export const MESSAGE_PRUNE_BATCH_LIMIT = 200;
export const MESSAGE_MAX_PRUNE_BATCH_LIMIT = 500;

function validTimestamp(value: string, field: string): number {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) throw new Error(`${field} must be a valid ISO timestamp`);
  return timestamp;
}

export function signalExpiresAt(kind: NotificationKind | string, createdAt: string): string {
  const days = MESSAGE_RETENTION_DAYS[kind as NotificationKind];
  if (days === undefined) throw new Error(`unknown notification kind: ${kind}`);
  return new Date(validTimestamp(createdAt, 'created_at') + days * 86_400_000).toISOString();
}

/** Deterministically backfill only missing expiry values; existing values are retained. */
export function backfillSignalExpiries(db: DatabaseSync): number {
  const rows = db.prepare(`SELECT signal_id, kind, created_at FROM signals
    WHERE expires_at IS NULL ORDER BY created_at, signal_id`).all() as Array<{
      signal_id: string; kind: string; created_at: string;
    }>;
  if (rows.length === 0) return 0;
  const update = db.prepare('UPDATE signals SET expires_at = ? WHERE signal_id = ? AND expires_at IS NULL');
  let updated = 0;
  for (const row of rows) updated += Number(update.run(signalExpiresAt(row.kind, row.created_at), row.signal_id).changes);
  return updated;
}

/** Delete only candidates whose removal cannot leave a reply without its ancestry. */
export function deletePrunableSignals(
  db: DatabaseSync,
  candidateIds: string[],
  dryRun: boolean,
): { signalIds: string[]; deleted: number } {
  const ids = (db.prepare(`WITH RECURSIVE
    candidates(id) AS (SELECT value FROM json_each(?)),
    retained(id) AS (
      SELECT parent.signal_id FROM signals parent JOIN candidates c ON c.id = parent.signal_id
      JOIN signals child ON child.reply_to = parent.signal_id OR child.thread_id = parent.signal_id
      WHERE child.signal_id NOT IN (SELECT id FROM candidates)
      UNION
      SELECT parent.signal_id FROM signals parent JOIN candidates c ON c.id = parent.signal_id
      JOIN signals child ON child.reply_to = parent.signal_id OR child.thread_id = parent.signal_id
      JOIN retained r ON r.id = child.signal_id
    )
    SELECT id FROM candidates WHERE id NOT IN (SELECT id FROM retained) ORDER BY id`)
    .all(JSON.stringify([...new Set(candidateIds)])) as Array<{ id: string }>).map(row => row.id);
  if (!dryRun && ids.length > 0) {
    db.prepare(SIGNALS_DELETE_BY_IDS(ids.map(() => '?').join(','))).run(...ids);
  }
  return { signalIds: ids, deleted: dryRun ? 0 : ids.length };
}

export interface ExpiredNotificationsResult {
  transitioned: number;
  wouldTransition?: number;
  deleted: number;
  wouldDelete?: number;
  partial?: true;
  next?: { pruneExpired: { now: string; pruneBefore: string; workspacePath?: string; limit: number } };
}

export function transitionExpiredNotifications(
  db: DatabaseSync,
  now = utcNow(),
  workspacePath?: string,
  limit = MESSAGE_PRUNE_BATCH_LIMIT,
): number {
  validTimestamp(now, 'now');
  if (!Number.isInteger(limit) || limit < 1 || limit > MESSAGE_MAX_PRUNE_BATCH_LIMIT) {
    throw new Error(`limit must be an integer between 1 and ${MESSAGE_MAX_PRUNE_BATCH_LIMIT}`);
  }
  const scope = workspacePath === undefined ? '' : ' AND workspace_path = ?';
  const bindings = workspacePath === undefined ? [now] : [now, workspacePath];
  let transitioned = 0;
  while (transitioned < limit) {
    const rows = db.prepare(`SELECT signal_id FROM signals
      WHERE status = 'open' AND expires_at <= ?${scope}
        AND NOT EXISTS (SELECT 1 FROM signals child
          WHERE child.signal_id <> signals.signal_id
            AND child.status = 'open' AND child.expires_at <= ?
            AND (child.reply_to = signals.signal_id OR child.thread_id = signals.signal_id))
      ORDER BY expires_at DESC, signal_id ASC LIMIT ?`)
      .all(...bindings, now, limit - transitioned) as Array<{ signal_id: string }>;
    if (rows.length === 0) break;
    const placeholders = rows.map(() => '?').join(',');
    const changed = Number(db.prepare(`UPDATE signals SET status = 'resolved', resolved_at = COALESCE(resolved_at, expires_at)
      WHERE signal_id IN (${placeholders}) AND status = 'open' AND expires_at <= ?${scope}`)
      .run(...rows.map(row => row.signal_id), ...bindings).changes);
    transitioned += changed;
    if (changed === 0) break;
  }
  return transitioned;
}

function countPrunableSignals(db: DatabaseSync, pruneBefore: string, workspacePath?: string): number {
  const scope = workspacePath === undefined ? '' : ' AND workspace_path = ?';
  const bindings = workspacePath === undefined ? [pruneBefore] : [pruneBefore, workspacePath];
  const row = db.prepare(`WITH RECURSIVE
    candidates(id) AS (
      SELECT signal_id FROM signals
      WHERE status IN ('open', 'resolved') AND expires_at <= ?${scope}
    ),
    retained(id) AS (
      SELECT parent.signal_id FROM signals parent JOIN candidates c ON c.id = parent.signal_id
      JOIN signals child ON child.reply_to = parent.signal_id OR child.thread_id = parent.signal_id
      WHERE child.signal_id NOT IN (SELECT id FROM candidates)
      UNION
      SELECT parent.signal_id FROM signals parent JOIN candidates c ON c.id = parent.signal_id
      JOIN signals child ON child.reply_to = parent.signal_id OR child.thread_id = parent.signal_id
      JOIN retained r ON r.id = child.signal_id
    )
    SELECT COUNT(*) AS count FROM candidates WHERE id NOT IN (SELECT id FROM retained)`)
    .get(...bindings) as { count: number };
  return Number(row.count);
}

/** Transition expired open messages and prune only expired, ancestry-safe rows. */
export function pruneExpiredNotifications(
  db: DatabaseSync,
  options: { now?: string; dryRun?: boolean; workspacePath?: string; pruneBefore?: string; graceDays?: number; limit?: number } = {},
): ExpiredNotificationsResult {
  const now = options.now ?? utcNow();
  validTimestamp(now, 'now');
  if (options.pruneBefore !== undefined && options.graceDays !== undefined) {
    throw new Error('provide pruneBefore or graceDays, not both');
  }
  if (options.graceDays !== undefined && (!Number.isInteger(options.graceDays)
    || options.graceDays < 0 || options.graceDays > MESSAGE_MAX_PRUNE_GRACE_DAYS)) {
    throw new Error(`graceDays must be an integer between 0 and ${MESSAGE_MAX_PRUNE_GRACE_DAYS}`);
  }
  const limit = options.limit ?? MESSAGE_PRUNE_BATCH_LIMIT;
  if (!Number.isInteger(limit) || limit < 1 || limit > MESSAGE_MAX_PRUNE_BATCH_LIMIT) {
    throw new Error(`limit must be an integer between 1 and ${MESSAGE_MAX_PRUNE_BATCH_LIMIT}`);
  }
  const pruneBefore = options.pruneBefore
    ?? new Date(validTimestamp(now, 'now') - (options.graceDays ?? MESSAGE_PRUNE_GRACE_DAYS) * 86_400_000).toISOString();
  if (validTimestamp(pruneBefore, 'pruneBefore') > validTimestamp(now, 'now')) {
    throw new Error('pruneBefore cannot be later than now');
  }
  const dryRun = options.dryRun === true;
  const scope = options.workspacePath === undefined ? '' : ' AND workspace_path = ?';
  const transitionBinds = options.workspacePath === undefined ? [now] : [now, options.workspacePath];
  const transitionCount = Number((db.prepare(`SELECT COUNT(*) AS count FROM signals
    WHERE status = 'open' AND expires_at <= ?${scope}`).get(...transitionBinds) as { count: number }).count);
  if (dryRun) {
    return {
      transitioned: 0,
      wouldTransition: transitionCount,
      deleted: 0,
      wouldDelete: countPrunableSignals(db, pruneBefore, options.workspacePath),
    };
  }
  const ownsTransaction = !db.isTransaction;
  if (ownsTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    const transitioned = transitionExpiredNotifications(db, now, options.workspacePath, limit);
    const candidateBinds = options.workspacePath === undefined ? [pruneBefore] : [pruneBefore, options.workspacePath];
    let deleted = 0;
    while (deleted < limit) {
      const candidateRows = db.prepare(`SELECT signal_id FROM signals
        WHERE status = 'resolved' AND expires_at <= ?${scope}
          AND NOT EXISTS (SELECT 1 FROM signals child
            WHERE child.signal_id <> signals.signal_id
              AND (child.reply_to = signals.signal_id OR child.thread_id = signals.signal_id))
        ORDER BY expires_at DESC, signal_id ASC LIMIT ?`)
        .all(...candidateBinds, limit - deleted) as Array<{ signal_id: string }>;
      if (candidateRows.length === 0) break;
      const pruned = deletePrunableSignals(db, candidateRows.map(row => row.signal_id), false);
      deleted += pruned.deleted;
      if (pruned.deleted === 0) break;
    }
    const partial = transitionCount > limit
      || countPrunableSignals(db, pruneBefore, options.workspacePath) > 0;
    const continuation = partial ? {
      pruneExpired: { now, pruneBefore, ...(options.workspacePath === undefined ? {} : { workspacePath: options.workspacePath }), limit },
    } : undefined;
    if (ownsTransaction) db.exec('COMMIT');
    const result = { transitioned, deleted };
    return continuation ? { ...result, partial: true, next: continuation } : result;
  } catch (error) {
    if (ownsTransaction) db.exec('ROLLBACK');
    throw error;
  }
}
