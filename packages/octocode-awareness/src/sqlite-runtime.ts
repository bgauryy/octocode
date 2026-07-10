import type { DatabaseSync } from 'node:sqlite';

export interface ConcurrentWalSafety {
  sqliteVersion: string;
  safe: boolean;
  reason: string;
}

const FIXED_BRANCHES = new Map<number, number>([
  [44, 6],
  [50, 7],
  [51, 3],
]);

function parseSqliteVersion(version: string): [major: number, minor: number, patch: number] | null {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:\D.*)?$/.exec(version.trim());
  if (!match) return null;
  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);
  if (![major, minor, patch].every(Number.isSafeInteger)) return null;
  return [major, minor, patch];
}

/**
 * SQLite's WAL-reset concurrency fix shipped in 3.51.3 and was backported to
 * 3.50.7 and 3.44.6. Other older branches are conservatively rejected.
 */
export function assessConcurrentWalSafety(sqliteVersion: string): ConcurrentWalSafety {
  const parsed = parseSqliteVersion(sqliteVersion);
  if (!parsed) {
    return {
      sqliteVersion,
      safe: false,
      reason: 'the embedded SQLite version could not be parsed',
    };
  }

  const [major, minor, patch] = parsed;
  const futureFixedLine = major > 3 || (major === 3 && minor > 51);
  const fixedPatch = major === 3 ? FIXED_BRANCHES.get(minor) : undefined;
  const safe = futureFixedLine || (fixedPatch !== undefined && patch >= fixedPatch);
  return {
    sqliteVersion,
    safe,
    reason: safe
      ? 'the embedded SQLite includes the concurrent WAL reset fix'
      : 'concurrent WAL requires SQLite 3.44.6, 3.50.7, or 3.51.3 (or a newer fixed release)',
  };
}

export function assertConcurrentWalSafe(sqliteVersion: string): void {
  const assessment = assessConcurrentWalSafety(sqliteVersion);
  if (assessment.safe) return;
  throw new Error(
    `SQLite ${sqliteVersion} is unsafe for concurrent WAL: ${assessment.reason}. Upgrade the Node runtime before enabling WAL.`,
  );
}

/** WAL is used only when the embedded SQLite contains the reset-race fix. */
export function journalModeForSqliteVersion(sqliteVersion: string): 'WAL' | 'DELETE' {
  return assessConcurrentWalSafety(sqliteVersion).safe ? 'WAL' : 'DELETE';
}

export function inspectSqliteRuntime(db: DatabaseSync): ConcurrentWalSafety {
  const row = db.prepare('SELECT sqlite_version() AS version').get() as { version: string };
  return assessConcurrentWalSafety(row.version);
}
