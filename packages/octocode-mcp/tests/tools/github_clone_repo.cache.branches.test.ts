import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import type { PathLike, StatSyncOptions, Stats } from 'node:fs';
import { tmpdir } from 'node:os';

vi.mock('node:fs', async importOriginal => {
  const actual = (await importOriginal()) as typeof import('node:fs');
  return {
    ...actual,
    existsSync: vi.fn(actual.existsSync),
    mkdirSync: vi.fn(actual.mkdirSync),
    rmSync: vi.fn(actual.rmSync),
    statSync: vi.fn(actual.statSync),
  };
});

import * as fs from 'node:fs';
import {
  ensureCloneParentDir,
  removeCloneDir,
  evictExpiredClones,
  startCacheGC,
  stopCacheGC,
  getReposBaseDir,
  getCloneDir,
  createCacheMeta,
  writeCacheMeta,
  isCacheHit,
} from '../../../octocode-tools-core/src/tools/github_clone_repo/cache.js';

describe('github_clone_repo cache - branch coverage', () => {
  const testBaseDir = join(tmpdir(), `octocode-cache-branches-${Date.now()}`);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    stopCacheGC();
    vi.restoreAllMocks();
    if (existsSync(testBaseDir)) {
      rmSync(testBaseDir, { recursive: true, force: true });
    }
  });

  describe('isCacheHit', () => {
    it('returns hit=false when meta valid but clone dir missing (line 141)', async () => {
      const octocodeDir = join(testBaseDir, 'cache-meta-valid-dir-gone');
      const cloneDir = getCloneDir(octocodeDir, 'owner', 'repo', 'main');
      mkdirSync(cloneDir, { recursive: true });
      const meta = createCacheMeta('owner', 'repo', 'main', 'clone');
      writeCacheMeta(cloneDir, meta);

      const realFs = await vi.importActual<typeof import('node:fs')>('node:fs');
      const actualExistsSync = realFs.existsSync as (path: PathLike) => boolean;
      vi.mocked(fs.existsSync).mockImplementation((pathLike: PathLike) => {
        if (String(pathLike) === cloneDir) return false;
        return actualExistsSync(pathLike);
      });

      const result = isCacheHit(cloneDir);
      expect(result.hit).toBe(false);
    });
  });

  describe('ensureCloneParentDir', () => {
    it('throws with String(error) when mkdirSync throws non-Error value', () => {
      const cloneDir = join(testBaseDir, 'level1', 'level2', 'clone');

      vi.mocked(fs.existsSync).mockReturnValue(false);
      vi.mocked(fs.mkdirSync).mockImplementationOnce(() => {
        throw 'permission denied';
      });

      expect(() => ensureCloneParentDir(cloneDir)).toThrow(
        /Failed to create clone parent directory.*permission denied/
      );
    });

    it('throws with error.message when mkdirSync throws Error', () => {
      const cloneDir = join(testBaseDir, 'level1', 'level2', 'clone');

      vi.mocked(fs.existsSync).mockReturnValue(false);
      vi.mocked(fs.mkdirSync).mockImplementationOnce(() => {
        throw new Error('EACCES: permission denied');
      });

      expect(() => ensureCloneParentDir(cloneDir)).toThrow(
        /Failed to create clone parent directory.*EACCES: permission denied/
      );
    });
  });

  describe('removeCloneDir', () => {
    it('swallows error when rmSync throws (best-effort cleanup)', () => {
      const dir = join(testBaseDir, 'to-remove');
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.rmSync).mockImplementationOnce(() => {
        throw new Error('Permission denied');
      });

      expect(() => removeCloneDir(dir)).not.toThrow();
    });
  });

  describe('evictExpiredClones', () => {
    it('returns 0 when reposBase dir does not exist (early return)', () => {
      const octocodeDir = join(testBaseDir, 'nonexistent-base');
      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(0);
    });

    it('skips branchDir when statSync throws in isDir (isDir catch)', async () => {
      const octocodeDir = join(testBaseDir, 'evict-stat-throws');
      const reposBase = getReposBaseDir(octocodeDir);
      const branchDir = join(reposBase, 'owner', 'repo', 'main');
      mkdirSync(branchDir, { recursive: true });
      const expiredMeta = createCacheMeta('owner', 'repo', 'main', 'clone');
      expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
      writeCacheMeta(branchDir, expiredMeta);

      const realFs = await vi.importActual<typeof import('node:fs')>('node:fs');
      const actualStatSync = realFs.statSync as (
        path: PathLike,
        options?: StatSyncOptions
      ) => Stats;
      vi.mocked(fs.statSync).mockImplementation(
        (pathLike: PathLike, options?: StatSyncOptions) => {
          if (String(pathLike) === branchDir) {
            throw new Error('Permission denied');
          }

          return actualStatSync(pathLike, options);
        }
      );

      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(0);
    });

    it('skips branchDir when rmSync throws (inner catch)', () => {
      const octocodeDir = join(testBaseDir, 'evict-rm-throws');
      const reposBase = getReposBaseDir(octocodeDir);
      const branchDir = join(reposBase, 'owner', 'repo', 'main');
      mkdirSync(branchDir, { recursive: true });
      const expiredMeta = createCacheMeta('owner', 'repo', 'main', 'clone');
      expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
      writeCacheMeta(branchDir, expiredMeta);

      vi.mocked(fs.rmSync).mockImplementationOnce(() => {
        throw new Error('Permission denied');
      });

      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(0);
    });

    it('skips ownerDir when not a directory (isDir returns false)', () => {
      const octocodeDir = join(testBaseDir, 'evict-owner-not-dir');
      const reposBase = getReposBaseDir(octocodeDir);
      mkdirSync(reposBase, { recursive: true });
      writeFileSync(join(reposBase, 'not-a-dir'), 'file', 'utf-8');

      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(0);
    });

    it('skips repoDir when not a directory (isDir returns false)', () => {
      const octocodeDir = join(testBaseDir, 'evict-repo-not-dir');
      const reposBase = getReposBaseDir(octocodeDir);
      const ownerDir = join(reposBase, 'owner');
      mkdirSync(ownerDir, { recursive: true });
      writeFileSync(join(ownerDir, 'not-a-dir'), 'file', 'utf-8');

      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(0);
    });

    it('skips branchDir when not a directory (isDir returns false)', () => {
      const octocodeDir = join(testBaseDir, 'evict-branch-not-dir');
      const reposBase = getReposBaseDir(octocodeDir);
      const repoDir = join(reposBase, 'owner', 'repo');
      mkdirSync(repoDir, { recursive: true });
      writeFileSync(join(repoDir, 'not-a-dir'), 'file', 'utf-8');

      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(0);
    });

    it('skips empty repoDir when rmSync throws (cleanup catch)', async () => {
      const octocodeDir = join(testBaseDir, 'evict-repo-rm-throws');
      const reposBase = getReposBaseDir(octocodeDir);
      const branchDir = join(reposBase, 'owner', 'repo', 'main');
      mkdirSync(branchDir, { recursive: true });
      const expiredMeta = createCacheMeta('owner', 'repo', 'main', 'clone');
      expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
      writeCacheMeta(branchDir, expiredMeta);

      const realFs = await vi.importActual<typeof import('node:fs')>('node:fs');
      let callCount = 0;
      vi.mocked(fs.rmSync).mockImplementation((path, opts) => {
        callCount++;
        if (callCount === 2) throw new Error('Permission denied');
        return realFs.rmSync(path as string, opts);
      });

      const count = evictExpiredClones(octocodeDir);
      expect(count).toBe(1);
    });
  });

  describe('startCacheGC', () => {
    it('second call returns early without creating double interval', () => {
      const dir = join(testBaseDir, 'gc-idempotent');
      mkdirSync(dir, { recursive: true });

      const setIntervalSpy = vi.spyOn(global, 'setInterval');

      startCacheGC(dir);
      const firstCallCount = setIntervalSpy.mock.calls.length;

      startCacheGC(dir);
      const secondCallCount = setIntervalSpy.mock.calls.length;

      expect(firstCallCount).toBe(1);
      expect(secondCallCount).toBe(1);
      setIntervalSpy.mockRestore();
    });

    it('setInterval callback runs when timer fires (fake timers)', () => {
      const dir = join(testBaseDir, 'gc-timer');
      mkdirSync(dir, { recursive: true });

      vi.useFakeTimers();
      try {
        startCacheGC(dir);
        expect(() => vi.advanceTimersByTime(10 * 60 * 1000)).not.toThrow();
        expect(() => vi.advanceTimersByTime(10 * 60 * 1000)).not.toThrow();
      } finally {
        vi.useRealTimers();
      }
    });
  });
});
