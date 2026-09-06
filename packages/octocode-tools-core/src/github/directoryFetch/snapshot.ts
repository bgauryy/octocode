import {
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import { randomUUID } from 'node:crypto';
import type { CloneCacheMeta } from '../../tools/github_clone_repo/types.js';
import { readCacheMeta } from '../../tools/github_clone_repo/cache.js';
import { getTreeLockDir } from '../../tools/github_clone_repo/cachePaths.js';
import {
  tryRecoverStaleCloneLock,
  writeCloneLockMeta,
} from '../../tools/github_clone_repo/cacheArtifacts.js';

const META_FILE = '.octocode-clone-meta.json';

/** Serializes overlapping file/directory updates across CLI and MCP processes. */
export async function withTreeLock<T>(
  home: string,
  cacheRoot: string,
  run: () => Promise<T>
): Promise<T> {
  const lock = getTreeLockDir(home, cacheRoot);
  mkdirSync(dirname(lock), { recursive: true, mode: 0o700 });
  const started = Date.now();
  for (;;) {
    try {
      mkdirSync(lock, { mode: 0o700 });
      writeCloneLockMeta(lock);
      break;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      if (tryRecoverStaleCloneLock(lock)) continue;
      if (Date.now() - started > 30_000)
        throw new Error('Timed out waiting for tree materialization lock.');
      await new Promise(resolve => setTimeout(resolve, 25));
    }
  }
  try {
    return await run();
  } finally {
    rmSync(lock, { recursive: true, force: true });
  }
}

export function ensureSnapshotDirectory(
  root: string,
  relativePath: string
): string {
  let directory = root;
  for (const part of relativePath.split('/').filter(Boolean)) {
    if (part === '..' || part === '.')
      throw new Error('Invalid snapshot directory path.');
    directory = join(directory, part);
    if (existsSync(directory) && lstatSync(directory).isSymbolicLink())
      throw new Error('Snapshot paths cannot traverse symbolic links.');
    mkdirSync(directory, { recursive: true, mode: 0o700 });
  }
  return directory;
}

export function currentTreeSnapshot(cacheRoot: string): string | undefined {
  const id = readCacheMeta(cacheRoot)?.snapshotId;
  if (!id || !/^[0-9a-f-]{36}$/.test(id)) return undefined;
  const root = join(cacheRoot, 'snapshots', id);
  try {
    return statSync(root).isDirectory() ? root : undefined;
  } catch {
    return undefined;
  }
}

/** Publish a fully written immutable generation by atomically replacing its metadata pointer.
 * Existing callers retain their generation; a failed update cannot remove their files.
 * Call under withTreeLock. Whole-cache TTL/capacity eviction owns old generations.
 */
export function publishTreeSnapshot(
  home: string,
  cacheRoot: string,
  previousRoot: string | undefined,
  meta: CloneCacheMeta,
  write: (root: string) => void
): string {
  const stagingBase = join(home, 'tmp', 'tree-staging');
  mkdirSync(stagingBase, { recursive: true, mode: 0o700 });
  const stage = mkdtempSync(join(stagingBase, 'snapshot-'));
  const id = randomUUID();
  const destination = join(cacheRoot, 'snapshots', id);
  const pointer = join(cacheRoot, `.snapshot-${id}.tmp`);
  let published = false;
  try {
    if (previousRoot)
      cpSync(previousRoot, stage, { recursive: true, dereference: false });
    write(stage);
    mkdirSync(dirname(destination), { recursive: true, mode: 0o700 });
    renameSync(stage, destination);
    writeFileSync(pointer, JSON.stringify({ ...meta, snapshotId: id }), {
      encoding: 'utf8',
      mode: 0o600,
    });
    renameSync(pointer, join(cacheRoot, META_FILE));
    published = true;
    return destination;
  } finally {
    rmSync(stage, { recursive: true, force: true });
    rmSync(pointer, { force: true });
    if (!published) rmSync(destination, { recursive: true, force: true });
  }
}
