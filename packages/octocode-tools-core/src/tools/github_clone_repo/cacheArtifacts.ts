import {
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
  mkdirSync,
} from 'node:fs';
import { dirname, join } from 'node:path';

const CLONE_ARTIFACT_MAX_AGE_MS = 15 * 60 * 1000;
const CLONE_LOCK_MAX_AGE_MS = 5 * 60 * 1000;
const CLONE_LOCK_META_FILE = '.octocode-lock.json';

interface CloneLockMeta {
  pid: number;
  createdAt: number;
}

/** Nonblocking exclusive acquisition shared by cache eviction and writers. */
export function tryAcquireMaterializationLock(
  lockDir: string
): (() => void) | undefined {
  mkdirSync(dirname(lockDir), { recursive: true, mode: 0o700 });
  try {
    mkdirSync(lockDir, { mode: 0o700 });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'EEXIST') return undefined;
    throw error;
  }
  writeCloneLockMeta(lockDir);
  return () => rmSync(lockDir, { recursive: true, force: true });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function listDir(path: string): string[] {
  try {
    return readdirSync(path);
  } catch {
    return [];
  }
}

export function writeCloneLockMeta(
  lockDir: string,
  pid: number = process.pid,
  createdAt: number = Date.now()
): void {
  try {
    writeFileSync(
      join(lockDir, CLONE_LOCK_META_FILE),
      JSON.stringify({ pid, createdAt } satisfies CloneLockMeta),
      { encoding: 'utf8', mode: 0o600 }
    );
  } catch {
    void 0;
  }
}

function readCloneLockMeta(lockDir: string): CloneLockMeta | null {
  try {
    const parsed = JSON.parse(
      readFileSync(join(lockDir, CLONE_LOCK_META_FILE), 'utf8')
    ) as unknown;
    if (!isRecord(parsed)) return null;
    if (!Number.isInteger(parsed.pid) || (parsed.pid as number) <= 0)
      return null;
    if (typeof parsed.createdAt !== 'number') return null;
    return { pid: parsed.pid as number, createdAt: parsed.createdAt };
  } catch {
    return null;
  }
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

export function tryRecoverStaleCloneLock(
  lockDir: string,
  now: number = Date.now()
): boolean {
  // Only one contender may inspect and rename a stale owner. Without this
  // claim, another contender can replace the stale lock between our read and
  // rename, causing us to erase that new live owner's lock (an ABA race).
  let release: (() => void) | undefined;
  try {
    release = tryAcquireMaterializationLock(`${lockDir}.recovery`);
  } catch {
    return false;
  }
  if (!release) return false;
  try {
    return recoverStaleLockUnderClaim(lockDir, now);
  } finally {
    release();
  }
}

function recoverStaleLockUnderClaim(lockDir: string, now: number): boolean {
  let createdAt: number;
  const meta = readCloneLockMeta(lockDir);
  if (meta) {
    createdAt = meta.createdAt;
    if (now - createdAt <= CLONE_LOCK_MAX_AGE_MS || isProcessAlive(meta.pid)) {
      return false;
    }
  } else {
    try {
      createdAt = statSync(lockDir).mtimeMs;
    } catch {
      return false;
    }
    if (now - createdAt <= CLONE_LOCK_MAX_AGE_MS) return false;
  }

  const tombstone = `${lockDir}.stale-${process.pid}-${now}`;
  try {
    renameSync(lockDir, tombstone);
    rmSync(tombstone, { recursive: true, force: true });
    return true;
  } catch {
    return false;
  }
}

export function cleanupStaleMaterializationArtifacts(
  octocodeDir: string,
  now: number = Date.now()
): number {
  let evicted = 0;
  for (const folder of ['clone-tmp', 'tree-staging']) {
    const tempBase = join(octocodeDir, 'tmp', folder);
    for (const name of listDir(tempBase)) {
      const path = join(tempBase, name);
      try {
        const info = statSync(path);
        if (now - info.mtimeMs <= CLONE_ARTIFACT_MAX_AGE_MS) continue;
        rmSync(path, { recursive: true, force: true });
        evicted++;
      } catch {
        void 0;
      }
    }
  }
  for (const folder of ['clone-locks', 'tree-locks']) {
    const locksBase = join(octocodeDir, 'tmp', folder);
    for (const name of listDir(locksBase)) {
      if (tryRecoverStaleCloneLock(join(locksBase, name), now)) evicted++;
    }
  }
  return evicted;
}
