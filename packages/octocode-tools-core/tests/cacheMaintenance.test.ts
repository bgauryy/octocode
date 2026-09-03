import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import {
  createCacheMeta,
  getCloneDir,
  getTreeDir,
  writeCacheMeta,
} from '../src/tools/github_clone_repo/cache.js';
import { getDiskCacheEntryPath } from '../src/utils/http/cache.js';
import {
  CACHE_MAINTENANCE_INTERVAL_MS,
  getCacheMaintenanceDelayMs,
  runCacheMaintenanceIfDue,
  startCacheGC,
  stopCacheGC,
} from '../src/cacheMaintenance.js';

function writeExpiredClone(home: string, name: string): string {
  const path = getCloneDir(home, 'owner', 'repo', name);
  mkdirSync(path, { recursive: true });
  const meta = createCacheMeta('owner', 'repo', name, 'clone');
  meta.expiresAt = new Date(Date.now() - 1_000).toISOString();
  writeCacheMeta(path, meta);
  return path;
}

describe('cache maintenance lifecycle', () => {
  let home: string;
  let previousDiskCache: string | undefined;
  let previousStorageMode: string | undefined;

  beforeEach(() => {
    home = mkdtempSync(join(tmpdir(), 'octocode-cache-maintenance-'));
    previousDiskCache = process.env.OCTOCODE_DISK_CACHE;
    previousStorageMode = process.env.OCTOCODE_STORAGE_MODE;
    process.env.OCTOCODE_STORAGE_MODE = 'persistent';
    process.env.OCTOCODE_DISK_CACHE = 'false';
  });

  afterEach(() => {
    stopCacheGC();
    vi.useRealTimers();
    if (previousDiskCache === undefined) delete process.env.OCTOCODE_DISK_CACHE;
    else process.env.OCTOCODE_DISK_CACHE = previousDiskCache;
    if (previousStorageMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
    else process.env.OCTOCODE_STORAGE_MODE = previousStorageMode;
    rmSync(home, { recursive: true, force: true });
  });

  it('runs one full bootstrap scan per persisted 24-hour window', async () => {
    const now = Date.now();
    const firstExpired = writeExpiredClone(home, 'first');
    const unrelated = join(home, 'tmp', 'plan', 'keep.txt');
    mkdirSync(join(home, 'tmp', 'plan'), { recursive: true });
    writeFileSync(unrelated, 'keep');

    expect(await runCacheMaintenanceIfDue(home, now)).toBe(true);
    expect(existsSync(firstExpired)).toBe(false);
    expect(existsSync(unrelated)).toBe(true);

    const secondExpired = writeExpiredClone(home, 'second');
    expect(await runCacheMaintenanceIfDue(home, now + 1_000)).toBe(false);
    expect(existsSync(secondExpired)).toBe(true);
    expect(getCacheMaintenanceDelayMs(home, now + 1_000)).toBe(
      CACHE_MAINTENANCE_INTERVAL_MS - 1_000
    );
  });

  it('cleans expired clone, tree, and response entries while preserving live and unrelated state', async () => {
    const now = Date.now();
    process.env.OCTOCODE_DISK_CACHE = 'true';

    const expiredClone = writeExpiredClone(home, 'expired-clone');
    const liveClone = getCloneDir(home, 'owner', 'repo', 'live-clone');
    mkdirSync(liveClone, { recursive: true });
    writeCacheMeta(
      liveClone,
      createCacheMeta('owner', 'repo', 'live-clone', 'clone')
    );

    const sha = '0123456789abcdef0123456789abcdef01234567';
    const expiredTree = getTreeDir(home, 'owner', 'repo', sha);
    mkdirSync(expiredTree, { recursive: true });
    const treeMeta = createCacheMeta('owner', 'repo', sha, 'treeFetch');
    treeMeta.expiresAt = new Date(now - 1_000).toISOString();
    writeCacheMeta(expiredTree, treeMeta);

    const writeResponse = (key: string, staleUntil: number): string => {
      const path = getDiskCacheEntryPath(key, home);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(
        path,
        JSON.stringify({
          version: 1,
          key,
          createdAt: now - 2_000,
          expiresAt: staleUntil,
          staleUntil,
          value: { key },
        })
      );
      return path;
    };
    const expiredResponse = writeResponse('v1-gh-api-prs:expired', now - 1);
    const liveResponse = writeResponse(
      'v1-gh-api-prs:live',
      now + CACHE_MAINTENANCE_INTERVAL_MS
    );
    const unrelated = join(home, 'tmp', 'plan', 'keep.txt');
    mkdirSync(dirname(unrelated), { recursive: true });
    writeFileSync(unrelated, 'keep');

    expect(await runCacheMaintenanceIfDue(home, now)).toBe(true);
    expect(existsSync(expiredClone)).toBe(false);
    expect(existsSync(expiredTree)).toBe(false);
    expect(existsSync(expiredResponse)).toBe(false);
    expect(existsSync(liveClone)).toBe(true);
    expect(existsSync(liveResponse)).toBe(true);
    expect(existsSync(unrelated)).toBe(true);
  });

  it('still maintains clone state when the response cache is disabled', async () => {
    const expired = writeExpiredClone(home, 'disk-disabled');

    expect(await runCacheMaintenanceIfDue(home)).toBe(true);
    expect(existsSync(expired)).toBe(false);
  });

  it('never blocks bootstrap when the cache home is not writable', async () => {
    const blockedHome = join(home, 'not-a-directory');
    writeFileSync(blockedHome, 'blocked');

    await expect(runCacheMaintenanceIfDue(blockedHome)).resolves.toBe(false);
  });

  it('does not touch cache state or schedule GC in memory mode', async () => {
    process.env.OCTOCODE_STORAGE_MODE = 'memory';
    const expired = writeExpiredClone(home, 'memory-mode');

    expect(await runCacheMaintenanceIfDue(home)).toBe(false);
    expect(existsSync(expired)).toBe(true);

    vi.useFakeTimers();
    startCacheGC(home);
    await vi.advanceTimersByTimeAsync(CACHE_MAINTENANCE_INTERVAL_MS);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('uses an unreferenced MCP timer and runs again at the persisted deadline', async () => {
    const now = new Date('2026-08-29T00:00:00.000Z');
    vi.useFakeTimers({ now });
    const firstExpired = writeExpiredClone(home, 'first-scheduled');

    startCacheGC(home);
    await vi.advanceTimersByTimeAsync(0);
    expect(existsSync(firstExpired)).toBe(false);
    expect(vi.getTimerCount()).toBe(1);

    const secondExpired = writeExpiredClone(home, 'second-scheduled');
    await vi.advanceTimersByTimeAsync(CACHE_MAINTENANCE_INTERVAL_MS);
    expect(existsSync(secondExpired)).toBe(false);
    expect(vi.getTimerCount()).toBe(1);

    stopCacheGC();
    expect(vi.getTimerCount()).toBe(0);
  });
});
