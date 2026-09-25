import { chmodSync, existsSync, lstatSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

export const PRIVATE_DIRECTORY_MODE = 0o700;
export const PRIVATE_FILE_MODE = 0o600;
const PRIVATE_LAYOUT_MARKER = '.permissions-v1';

function chmodPortable(path: string, mode: number): void {
  if (process.platform !== 'win32') chmodSync(path, mode);
}

/** Create a state directory and repair permissive modes left by older releases. */
export function ensurePrivateDirectory(directory: string): void {
  mkdirSync(directory, { recursive: true, mode: PRIVATE_DIRECTORY_MODE });
  const stats = lstatSync(directory);
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`Private state directory must be a real directory: ${directory}`);
  }
  chmodPortable(directory, PRIVATE_DIRECTORY_MODE);
}

/** Reject links/non-files before a sensitive path is opened and repair its mode. */
export function hardenPrivateFile(file: string): void {
  if (!existsSync(file)) return;
  const stats = lstatSync(file);
  if (!stats.isFile() || stats.isSymbolicLink()) {
    throw new Error(`Private state path must be a regular file: ${file}`);
  }
  chmodPortable(file, PRIVATE_FILE_MODE);
}

/**
 * One-time upgrade for state written by releases that relied on the process
 * umask. Links and special files are never followed; owner execute permission
 * is preserved for bundled/user tools while all group/other access is removed.
 */
export function migratePrivateHome(home: string): void {
  ensurePrivateDirectory(home);
  const marker = `${home}/${PRIVATE_LAYOUT_MARKER}`;
  if (existsSync(marker)) {
    hardenPrivateFile(marker);
    return;
  }

  const visit = (entry: string): void => {
    let stats;
    try {
      stats = lstatSync(entry);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return;
      throw error;
    }
    if (stats.isSymbolicLink()) return;
    if (stats.isDirectory()) {
      chmodPortable(entry, PRIVATE_DIRECTORY_MODE);
      for (const child of readdirSync(entry)) visit(`${entry}/${child}`);
      return;
    }
    if (stats.isFile()) chmodPortable(entry, stats.mode & 0o100 ? 0o700 : PRIVATE_FILE_MODE);
  };

  visit(home);
  try {
    writeFileSync(marker, 'owner-only\n', { encoding: 'utf8', mode: PRIVATE_FILE_MODE, flag: 'wx' });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
  }
  hardenPrivateFile(marker);
}

/** Prepare a file-backed SQLite path before DatabaseSync can follow or create it. */
export function preparePrivateSqlitePath(file: string): void {
  if (file === ':memory:') return;
  const parent = dirname(file);
  ensurePrivateDirectory(parent);
  // The canonical database is directly under Octocode home. Opening it is the
  // earliest common upgrade boundary shared by the launcher and the harness.
  if (file.endsWith('/octocode.sqlite3') || file.endsWith('\\octocode.sqlite3')) migratePrivateHome(parent);
  hardenPrivateFile(file);
}

/** Harden the database plus any sidecars that exist after journal/schema setup. */
export function hardenSqliteFiles(file: string): void {
  if (file === ':memory:') return;
  for (const suffix of ['', '-wal', '-shm', '-journal']) hardenPrivateFile(`${file}${suffix}`);
}
