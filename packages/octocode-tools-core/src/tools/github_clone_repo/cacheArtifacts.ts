import {
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';

const CLONE_ARTIFACT_MAX_AGE_MS = 15 * 60 * 1000;
const CLONE_LOCK_MAX_AGE_MS = 5 * 60 * 1000;
const CLONE_LOCK_META_FILE = '.octocode-lock.json';

interface CloneLockMeta {
  pid: number;
  createdAt: number;
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

export function cleanupStaleCloneArtifacts(
  octocodeDir: string,
  now: number = Date.now()
): number {
  const tempBase = join(octocodeDir, 'tmp', 'clone-tmp');
  let evicted = 0;
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

  const locksBase = join(octocodeDir, 'tmp', 'clone-locks');
  for (const name of listDir(locksBase)) {
    if (tryRecoverStaleCloneLock(join(locksBase, name), now)) evicted++;
  }
  return evicted;
}
