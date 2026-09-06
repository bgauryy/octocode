/**
 * sqlite.ts — the shared low-level `node:sqlite` runtime.
 *
 * Canonical home for the plumbing both Octocode SQLite stores duplicated: the
 * ExperimentalWarning-filtered `DatabaseSync` import, the bounded BUSY retry
 * (SharedArrayBuffer-backed wait, no busy-spin), and the WAL checkpoint helper.
 *
 * This module performs the `node:sqlite` import at load (top-level await) behind
 * a one-tick warning filter, so importing it is a side effect — keep it out of
 * pure/leaf modules (see sqlite-version.ts / schema.ts, which stay pure).
 *
 * Requires Node `^22.22.2 || ^24.15.0 || >=26.0.0` for the unflagged `node:sqlite` built-in.
 */
import type { DatabaseSync as NodeDatabaseSync } from 'node:sqlite';

export type DatabaseSync = NodeDatabaseSync;

// Node can emit an ExperimentalWarning after a static import has already
// bypassed executable banners. Load `node:sqlite` after installing a one-tick,
// precise filter; forward every unrelated warning and restore host listeners.
export const previousWarningListeners = process.listeners('warning');
process.removeAllListeners('warning');
export const sqliteWarningFilter = (warning: Error & { name?: string }) => {
  if (warning?.name === 'ExperimentalWarning' && String(warning?.message).includes('SQLite')) return;
  for (const listener of previousWarningListeners) listener.call(process, warning);
};
process.on('warning', sqliteWarningFilter);
export const { DatabaseSync } = await import('node:sqlite');
await new Promise<void>((resolveTick) => setImmediate(resolveTick));
process.removeAllListeners('warning');
for (const listener of previousWarningListeners) process.on('warning', listener);

export const SQLITE_BUSY_RETRY_MS = 25;
export const SQLITE_BUSY_DEADLINE_MS = 10_000;
export const SQLITE_WAIT = new Int32Array(new SharedArrayBuffer(4));

export function isSqliteBusy(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const sqlite = error as Error & { errcode?: number; errstr?: string };
  return sqlite.errcode === 5 || /database is (?:locked|busy)/i.test(`${sqlite.errstr ?? ''} ${error.message}`);
}

export function withSqliteBusyRetry<T>(operation: () => T): T {
  const deadline = Date.now() + SQLITE_BUSY_DEADLINE_MS;
  for (;;) {
    try {
      return operation();
    } catch (error) {
      if (!isSqliteBusy(error) || Date.now() >= deadline) throw error;
      Atomics.wait(SQLITE_WAIT, 0, 0, SQLITE_BUSY_RETRY_MS);
    }
  }
}

/**
 * Checkpoint the WAL so the main DB file absorbs pending pages.
 * Non-fatal on :memory: stores or when a concurrent reader blocks TRUNCATE.
 */
export function checkpointWal(db: DatabaseSync): void {
  try {
    db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  } catch {
    /* non-fatal */
  }
}
