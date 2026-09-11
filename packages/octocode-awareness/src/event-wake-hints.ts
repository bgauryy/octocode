import { watch, type FSWatcher } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';

export interface AwarenessEventHintOptions {
  database: string;
  onHint(): void;
  onError?(error: unknown): void;
}

/**
 * Filesystem notifications are only hints. A read-only outbox sequence check
 * suppresses our own receipt/WAL churn; every delivery still reads SQLite.
 * Watching the directory survives WAL recreation and atomic DB replacement.
 * No idle polling: failed watches/reads retry quickly, then stay armed on a
 * sparse recovery interval until the source returns or the watcher is closed.
 */
export function watchAwarenessEventHints(options: AwarenessEventHintOptions): { close(): void } {
  if (options.database === ':memory:') return { close() {} };
  const database = resolve(options.database);
  const filename = basename(database);
  const relevant = new Set([filename, `${filename}-wal`, `${filename}-journal`]);
  let watcher: FSWatcher | undefined;
  const fileWatchers = new Map<string, FSWatcher>();
  let scheduled: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let sequence = 0;
  let failures = 0;
  const retryDelays = [100, 500, 2000];
  const recoveryRetryMs = 30_000;

  const schedule = (delay = 50): void => {
    if (closed || scheduled) return;
    scheduled = setTimeout(check, delay);
    scheduled.unref();
  };
  const failed = (error: unknown): void => {
    if (closed) return;
    try { options.onError?.(error); } catch { /* diagnostics never own delivery */ }
    const delay = retryDelays[failures] ?? recoveryRetryMs;
    failures = Math.min(failures + 1, retryDelays.length);
    schedule(delay);
  };
  const attach = (): void => {
    if (closed) return;
    if (!watcher) {
      watcher = watch(dirname(database), { persistent: false }, (_event, changed) => {
        if (changed === null || relevant.has(String(changed))) schedule();
      });
      watcher.on('error', (error) => {
        watcher?.close();
        watcher = undefined;
        failed(error);
      });
    }
    // kqueue directory watches report entry changes, not writes to an existing
    // WAL. Watch both files too; the directory reattaches them after recreation.
    for (const file of [database, `${database}-wal`]) {
      if (fileWatchers.has(file)) continue;
      try {
        const fileWatcher = watch(file, { persistent: false }, (event) => {
          if (event === 'rename') { fileWatcher.close(); fileWatchers.delete(file); }
          schedule();
        });
        fileWatcher.on('error', (error) => {
          fileWatcher.close(); fileWatchers.delete(file); failed(error);
        });
        fileWatchers.set(file, fileWatcher);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
    }
  };
  function check(): void {
    scheduled = undefined;
    if (closed) return;
    let db: DatabaseSync | undefined;
    try {
      attach();
      // Never initialize, migrate, or journal-switch a store merely to watch it.
      db = new DatabaseSync(database, { readOnly: true });
      const row = db.prepare('SELECT MAX(sequence) AS sequence FROM event_outbox').get() as { sequence: number | null };
      const next = row.sequence ?? 0;
      if (next !== sequence) {
        options.onHint();
        sequence = next;
      }
      failures = 0;
    } catch (error) { failed(error); }
    finally { db?.close(); }
  }
  try { attach(); schedule(); } catch (error) { failed(error); }
  return {
    close(): void {
      closed = true;
      if (scheduled) clearTimeout(scheduled);
      scheduled = undefined;
      watcher?.close();
      watcher = undefined;
      for (const fileWatcher of fileWatchers.values()) fileWatcher.close();
      fileWatchers.clear();
    },
  };
}
