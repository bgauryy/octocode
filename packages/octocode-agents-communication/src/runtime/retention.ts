/** Explicit, history-preserving maintenance; expiry never authorizes deletion. */
import { lstatSync, statSync } from 'node:fs';
import * as catalog from './catalog.js';
import { fingerprint, openDatabase, query, readTransaction, rowInteger, rowText, schema, type Database } from './database.js';
import type { CommandResults, DbRetentionInput, Storage } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { DAY_MS, now } from './timing.js';

function fileSize(path: string): number {
  try {
    return statSync(path).size;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return 0;
    throw error;
  }
}

function storage(db: Database, path: string): Storage {
  const size = db.integer('PRAGMA page_size');
  const count = db.integer('PRAGMA page_count');
  const free = db.integer('PRAGMA freelist_count');
  return { pageSize: size, pageCount: count, freePages: free, logicalBytes: size * count, reusableBytes: size * free, mainFileBytes: statSync(path).size, walBytes: fileSize(path + '-wal') };
}

export function report(path: string, input: DbRetentionInput): CommandResults['db retention'] {
  catalog.command('db retention', input);
  const at = now();
  const before = input.before ?? Math.max(0, at - 30 * DAY_MS);
  if (before > at) throw new ValidationError('before must be no later than now');
  const after = input.afterId ?? 0;
  const limit = input.limit ?? 100;
  const db = openDatabase(path, true, false);
  try {
    return readTransaction(db, () => {
      const found = query(db, "SELECT m.id,m.expiresAt,length(CAST(m.parts AS BLOB)) AS contentBytes, EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.acknowledgedAt IS NULL) AS unacknowledged, EXISTS(SELECT 1 FROM dispatches x WHERE x.message=m.id AND x.state IN ('staged','uncertain')) AS unresolved FROM messages m WHERE m.id>? ORDER BY m.id LIMIT ?", [after, limit + 1]);
      const more = found.length > limit;
      const rows = found.slice(0, limit);
      const settled = rows.filter(row => rowInteger(row, 'expiresAt') <= before && !row.unacknowledged && !row.unresolved);
      const last = rows.at(-1);
      return {
        scope: 'all-workspaces', readOnly: true, deletionSupported: false, before, cutoffField: 'messages.expiresAt', observedAt: at, storage: storage(db, path),
        page: { afterId: after, limit, count: rows.length, hasMore: more, expiredSettledMessages: settled.length, expiredSettledContentBytes: settled.reduce((sum, row) => sum + rowInteger(row, 'contentBytes'), 0), unacknowledgedMessages: rows.filter(row => row.unacknowledged).length, unresolvedDispatchMessages: rows.filter(row => row.unresolved).length },
        retainedBecause: ['Audit is append-only and does not duplicate message content.', 'Message keys preserve idempotency; replyTo and delivery records preserve correlation and visibility.', 'Expiry and acknowledgement do not authorize deletion; pending and uncertain attempts remain recoverable.'],
        maintenance: { compact: 'db compact {} reclaims reusable SQLite pages without deleting history.', archive: 'db export creates a verified full snapshot; preserve workspace documents separately.' },
        ...(more && last ? { next: { command: 'db retention', input: { before, afterId: rowInteger(last, 'id'), limit } } } : {}),
      };
    });
  } finally {
    db.close();
  }
}

const identity = (path: string) => lstatSync(path, { bigint: true });

export function compact(path: string): CommandResults['db compact'] {
  const initial = identity(path);
  if (!initial.isFile()) throw new ValidationError('Compaction requires a regular database file, not a symlink');
  const db = openDatabase(path, false, false);
  try {
    db.exec('PRAGMA busy_timeout=2000');
    const digest = fingerprint(db);
    const before = storage(db, path);
    try {
      db.exec('VACUUM');
    } catch (error) {
      throw new ValidationError(`Compaction failed; no automatic retry was attempted: ${(error as Error).message}`);
    }
    if (fingerprint(db) !== digest) throw new ValidationError('Compaction completed but schema verification failed');
    const integrity = query(db, 'PRAGMA integrity_check');
    if (integrity.length !== 1 || rowText(integrity[0]!, 'integrity_check') !== 'ok' || query(db, 'PRAGMA foreign_key_check').length) throw new ValidationError('Compaction completed but database integrity verification failed');
    const checkpoint = query(db, 'PRAGMA wal_checkpoint(PASSIVE)');
    const current = identity(path);
    if (!current.isFile() || initial.dev !== current.dev || initial.ino !== current.ino) throw new ValidationError('Database path was replaced during compaction');
    const after = storage(db, path);
    return {
      scope: 'all-workspaces', compacted: true, deletedRecords: 0, schemaVersion: schema().schemaVersion, schemaSha256: digest, integrity: 'ok',
      reclaimedBytes: Math.max(0, before.mainFileBytes - after.mainFileBytes), logicalReclaimedBytes: Math.max(0, before.logicalBytes - after.logicalBytes), before, after,
      checkpoint: checkpoint.map(row => ({ busy: rowInteger(row, 'busy'), log: rowInteger(row, 'log'), checkpointed: rowInteger(row, 'checkpointed') })),
      measurement: 'reclaimedBytes is the nonnegative main-file size difference; WAL allocation and concurrent writers can mask savings. No WAL truncation is forced.',
    };
  } finally {
    db.close();
  }
}
