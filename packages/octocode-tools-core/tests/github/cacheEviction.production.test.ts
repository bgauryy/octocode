import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const state = vi.hoisted(() => ({
  onRead: undefined as ((path: string) => void) | undefined,
}));
vi.mock('node:fs', async original => {
  const fs = await original<typeof import('node:fs')>();
  return {
    ...fs,
    readFileSync: (...args: Parameters<typeof fs.readFileSync>) => {
      const value = fs.readFileSync(...args);
      state.onRead?.(String(args[0]));
      return value;
    },
  };
});

const {
  createCacheMeta,
  writeCacheMeta,
  evictExpiredTrees,
  evictExpiredClones,
} = await import('../../src/tools/github_clone_repo/cache.js');
const { getTreeDir, getTreeLockDir, getCloneDir } =
  await import('../../src/tools/github_clone_repo/cachePaths.js');
const {
  writeCloneLockMeta,
  tryRecoverStaleCloneLock,
  cleanupStaleMaterializationArtifacts,
} = await import('../../src/tools/github_clone_repo/cacheArtifacts.js');
let home = '';
beforeEach(() => {
  home = mkdtempSync(join(tmpdir(), 'octocode-eviction-race-'));
});
afterEach(() => {
  state.onRead = undefined;
  vi.unstubAllEnvs();
  rmSync(home, { recursive: true, force: true });
});

describe.each(['tree', 'clone'])('%s eviction exclusion', family => {
  it.each(['expired', 'capacity'])(
    'does not delete a writer that wins the %s eviction race',
    reason => {
      const root =
        family === 'tree'
          ? getTreeDir(home, 'o', 'r', 'a'.repeat(40))
          : getCloneDir(home, 'o', 'r', 'main');
      const lock =
        family === 'tree'
          ? getTreeLockDir(home, root)
          : join(
              home,
              'tmp',
              'clone-locks',
              createHash('sha256').update(root).digest('hex').slice(0, 16)
            );
      mkdirSync(root, { recursive: true });
      mkdirSync(join(lock, '..'), { recursive: true });
      const meta = createCacheMeta(
        'o',
        'r',
        'main',
        family === 'tree' ? 'treeFetch' : 'clone',
        undefined,
        1024
      );
      if (reason === 'expired') meta.expiresAt = new Date(0).toISOString();
      else vi.stubEnv('OCTOCODE_MAX_CACHE_SIZE', '1');
      writeCacheMeta(root, meta);
      let reads = 0;
      let attempted = false;
      let writerAcquired = false;
      state.onRead = path => {
        if (path !== join(root, '.octocode-clone-meta.json')) return;
        if (++reads !== (reason === 'expired' ? 1 : 2)) return;
        attempted = true;
        // Deterministic interleaving: another process attempts the writer lock
        // after the evictor inspected metadata but before it removes the entry.
        try {
          mkdirSync(lock);
          writeCloneLockMeta(lock);
          writerAcquired = true;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
        }
      };
      (family === 'tree' ? evictExpiredTrees : evictExpiredClones)(home);
      expect(attempted).toBe(true);
      if (writerAcquired) {
        expect(
          existsSync(root),
          'eviction erased a concurrent writer cache'
        ).toBe(true);
        expect(existsSync(lock), 'eviction erased another owner lock').toBe(
          true
        );
      } else {
        expect(existsSync(root)).toBe(false);
        expect(existsSync(lock), 'eviction leaked its lock').toBe(false);
      }
    }
  );
});

it('cannot reclaim the fresh owner installed by another stale-lock waiter', () => {
  const lock = getTreeLockDir(home, getTreeDir(home, 'o', 'r', 'a'.repeat(40)));
  mkdirSync(lock, { recursive: true });
  writeCloneLockMeta(lock, 2147483647, Date.now() - 600_000);
  let attempted = false;
  let otherReclaimed = false;
  state.onRead = path => {
    if (path !== join(lock, '.octocode-lock.json')) return;
    state.onRead = undefined;
    attempted = true;
    // A second waiter runs after this waiter has read the stale metadata.
    // If it reclaims the old lock, a writer immediately owns the replacement.
    otherReclaimed = tryRecoverStaleCloneLock(lock);
    if (otherReclaimed) {
      mkdirSync(lock);
      writeCloneLockMeta(lock);
    }
  };
  const reclaimed = tryRecoverStaleCloneLock(lock);
  expect(attempted).toBe(true);
  if (otherReclaimed) {
    expect(reclaimed, 'stale waiter reclaimed a fresh live lock').toBe(false);
    expect(existsSync(lock)).toBe(true);
  } else {
    expect(reclaimed).toBe(true);
    expect(existsSync(lock)).toBe(false);
  }
});

it('recovers an abandoned recovery claim without leaving a permanent lock', () => {
  const lock = getTreeLockDir(home, getTreeDir(home, 'o', 'r', 'a'.repeat(40)));
  for (const path of [lock, `${lock}.recovery`]) {
    mkdirSync(path, { recursive: true });
    writeCloneLockMeta(path, 2147483647, Date.now() - 600_000);
  }
  expect(tryRecoverStaleCloneLock(lock)).toBe(false);
  cleanupStaleMaterializationArtifacts(home);
  expect(tryRecoverStaleCloneLock(lock)).toBe(true);
  expect(existsSync(lock)).toBe(false);
  expect(existsSync(`${lock}.recovery`)).toBe(false);
  expect(existsSync(`${lock}.recovery.recovery`)).toBe(false);
});
