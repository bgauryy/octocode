/** Repository coordination identity without a Git executable or environment overrides. */
import { spawnSync } from 'node:child_process';
import { closeSync, constants, existsSync, fstatSync, openSync, readSync } from 'node:fs';
import { dirname, isAbsolute, join, sep } from 'node:path';
import { execute, type Database } from './database.js';
import { ValidationError } from './errors.js';
import { isDirectory, isFile, lexists, realpath } from './paths.js';
import { decodeUtf8 } from './json-text.js';

const METADATA_LIMIT = 4096;

/** The directory a Git metadata file (`.git` file or `commondir`) points to, relative to the file. */
export function metadataPath(path: string, prefix = ''): string {
  const descriptor = openSync(path, constants.O_RDONLY | (constants.O_NONBLOCK ?? 0));
  let raw: Buffer;
  try {
    if (!fstatSync(descriptor).isFile()) throw new ValidationError('Git metadata must be a regular file');
    raw = Buffer.alloc(METADATA_LIMIT + 1);
    raw = raw.subarray(0, readSync(descriptor, raw, 0, raw.length, null));
  } finally {
    closeSync(descriptor);
  }
  if (raw.length > METADATA_LIMIT) throw new ValidationError('Git workspace metadata exceeds 4096 bytes');
  let value = decodeUtf8(raw).replace(/[\r\n]+$/, '');
  if (prefix && !value.startsWith(prefix)) throw new ValidationError('Invalid Git workspace metadata: ' + path);
  if (prefix) value = value.slice(prefix.length).trimStart();
  if (!value || value.includes('\n') || value.includes('\r')) throw new ValidationError('Invalid Git workspace metadata: ' + path);
  return realpath(isAbsolute(value) ? value : dirname(path) + sep + value, true);
}

/** The Git common directory of the enclosing repository, else the workspace itself. */
export function coordinationScope(workspace: string): string {
  const resolved = realpath(workspace);
  for (let root = resolved; ; root = dirname(root)) {
    const marker = join(root, '.git');
    if (lexists(marker)) {
      const gitdir = isDirectory(marker) ? realpath(marker, true) : metadataPath(marker, 'gitdir:');
      const common = existsSync(join(gitdir, 'commondir')) ? metadataPath(join(gitdir, 'commondir')) : gitdir;
      if (!isFile(join(gitdir, 'HEAD')) || !isDirectory(join(common, 'objects'))) throw new ValidationError('Invalid Git repository metadata: ' + marker);
      return common;
    }
    if (dirname(root) === root) return resolved;
  }
}

/** The current branch of `workspace`, or null for a detached HEAD, a non-repository or a slow Git. */
export function currentBranch(workspace: string): string | null {
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')));
  const result = spawnSync('git', ['-C', workspace, 'symbolic-ref', '--quiet', '--short', 'HEAD'], { encoding: 'utf8', timeout: 2000, stdio: ['ignore', 'pipe', 'ignore'], env });
  if (result.error || result.status !== 0) return null;
  return result.stdout.trim() || null;
}

/** The repository scope `workspace` joined this database under; the workspace itself when it has not joined. */
export function storedScope(db: Database, workspace: string): string {
  const row = db.get('SELECT coordinationScope FROM workspaces WHERE workspace=?', [workspace]);
  return row ? (row.coordinationScope as string) : workspace;
}

/** Fails when `workspace` first joined this database under another repository scope. */
export function checkScope(db: Database, workspace: string, scope: string): void {
  if (storedScope(db, workspace) !== scope) throw new ValidationError('Workspace repository changed since this path first joined this database; bind this path to a new --database file (existing records stay readable in the old one)');
}

/** Records the repository scope a workspace first joins under, and fails when it changed since. Runs inside the caller's transaction. */
export function registerWorkspace(db: Database, workspace: string, scope: string): void {
  execute(db, 'INSERT OR IGNORE INTO workspaces(workspace,coordinationScope) VALUES(?,?)', [workspace, scope]);
  checkScope(db, workspace, scope);
}

/** The workspace itself, then every other workspace of the same scope in name order. */
export function scopeWorkspaces(db: Database, scope: string, workspace: string): string[] {
  return [workspace, ...db.all('SELECT workspace FROM workspaces WHERE coordinationScope=? AND workspace<>? ORDER BY workspace', [scope, workspace]).map(row => row.workspace as string)];
}
