import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

describe('shared file-backed response cache', () => {
  let home: string;
  let previousHome: string | undefined;
  let previousEnabled: string | undefined;
  let previousMaxEntries: string | undefined;

  beforeEach(() => {
    home = mkdtempSync(join(tmpdir(), 'octocode-disk-cache-'));
    previousHome = process.env.OCTOCODE_HOME;
    previousEnabled = process.env.OCTOCODE_DISK_CACHE;
    previousMaxEntries = process.env.OCTOCODE_DISK_CACHE_MAX_ENTRIES;
    process.env.OCTOCODE_HOME = home;
    process.env.OCTOCODE_DISK_CACHE = 'true';
    vi.resetModules();
  });

  afterEach(async () => {
    const { clearAllCache } =
      await import('../../../src/utils/http/cache/management.js');
    clearAllCache();
    if (previousHome === undefined) delete process.env.OCTOCODE_HOME;
    else process.env.OCTOCODE_HOME = previousHome;
    if (previousEnabled === undefined) delete process.env.OCTOCODE_DISK_CACHE;
    else process.env.OCTOCODE_DISK_CACHE = previousEnabled;
    if (previousMaxEntries === undefined)
      delete process.env.OCTOCODE_DISK_CACHE_MAX_ENTRIES;
    else process.env.OCTOCODE_DISK_CACHE_MAX_ENTRIES = previousMaxEntries;
    rmSync(home, { recursive: true, force: true });
    vi.resetModules();
  });

  it('reuses a response after the memory cache is cleared', async () => {
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { withDataCache } =
      await import('../../../src/utils/http/cache/dataCache.js');
    const { clearAllCache, getCacheStats } =
      await import('../../../src/utils/http/cache/management.js');
    const key = generateCacheKey('gh-api-prs', {
      owner: 'octocode',
      query: 'persistent',
      auth: 'fingerprint-a',
    });
    let upstreamCalls = 0;

    const first = await withDataCache(
      key,
      async () => {
        upstreamCalls++;
        return { items: [{ number: 1 }] };
      },
      { ttl: 60 }
    );
    clearAllCache();

    const second = await withDataCache(
      key,
      async () => {
        upstreamCalls++;
        return { items: [{ number: 2 }] };
      },
      { ttl: 60 }
    );

    expect(second).toEqual(first);
    expect(upstreamCalls).toBe(1);
    expect(getCacheStats().diskHits).toBe(1);
  });

  it('marks a bulk result restored from the disk response cache', async () => {
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { withDataCache } =
      await import('../../../src/utils/http/cache/dataCache.js');
    const { clearAllCache, getCacheStats } =
      await import('../../../src/utils/http/cache/management.js');
    const { executeBulkOperation } =
      await import('../../../src/utils/response/bulk/response.js');
    const key = generateCacheKey('gh-api-prs', {
      owner: 'octocode',
      query: 'disk-response-marker',
      auth: 'fingerprint-a',
    });
    let upstreamCalls = 0;
    const execute = () =>
      executeBulkOperation(
        [{ id: 'query' }],
        async () => ({
          value: await withDataCache(
            key,
            async () => {
              upstreamCalls++;
              return 'payload';
            },
            { ttl: 60 }
          ),
        }),
        { toolName: 'ghSearchPullRequests' }
      );
    const rows = (result: Awaited<ReturnType<typeof execute>>) =>
      (
        result.structuredContent as {
          results: Array<Record<string, unknown>>;
        }
      ).results;

    expect(rows(await execute())[0]).not.toHaveProperty('cache');
    clearAllCache();
    expect(rows(await execute())[0]).toMatchObject({ cache: 1 });
    expect(upstreamCalls).toBe(1);
    expect(getCacheStats().diskHits).toBe(1);
  });

  it('keeps auth fingerprints isolated and forceRefresh bypasses disk', async () => {
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { withDataCache } =
      await import('../../../src/utils/http/cache/dataCache.js');
    const { clearAllCache } =
      await import('../../../src/utils/http/cache/management.js');
    const keyA = generateCacheKey('gh-api-prs', {
      query: 'same',
      auth: 'fingerprint-a',
    });
    const keyB = generateCacheKey('gh-api-prs', {
      query: 'same',
      auth: 'fingerprint-b',
    });

    await withDataCache(keyA, async () => 'a', { ttl: 60 });
    clearAllCache();

    let calls = 0;
    const isolated = await withDataCache(
      keyB,
      async () => {
        calls++;
        return 'b';
      },
      { ttl: 60 }
    );
    const refreshed = await withDataCache(
      keyA,
      async () => {
        calls++;
        return 'fresh-a';
      },
      { ttl: 60, forceRefresh: true }
    );

    expect(isolated).toBe('b');
    expect(refreshed).toBe('fresh-a');
    expect(calls).toBe(2);
  });

  it('self-heals a corrupt entry and keeps cache files private', async () => {
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { getDiskCacheEntryPath } =
      await import('../../../src/utils/http/cache/diskStore.js');
    const { withDataCache } =
      await import('../../../src/utils/http/cache/dataCache.js');
    const key = generateCacheKey('gh-api-file-content', {
      owner: 'o',
      repo: 'r',
      path: 'secret.ts',
      auth: 'fingerprint-private',
    });
    const entryPath = getDiskCacheEntryPath(key);
    mkdirSync(dirname(entryPath), { recursive: true, mode: 0o700 });
    writeFileSync(entryPath, '{broken', { mode: 0o600 });

    let calls = 0;
    const value = await withDataCache(
      key,
      async () => {
        calls++;
        return { content: 'safe' };
      },
      { ttl: 60 }
    );

    expect(value).toEqual({ content: 'safe' });
    expect(calls).toBe(1);
    expect(existsSync(entryPath)).toBe(true);
    expect(statSync(entryPath).mode & 0o777).toBe(0o600);
    expect(statSync(dirname(entryPath)).mode & 0o077).toBe(0);
  });

  it('uses a stale disk body and etag to recover a conditional 304', async () => {
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { withDataCacheConditional } =
      await import('../../../src/utils/http/cache/conditional.js');
    const { clearAllCache } =
      await import('../../../src/utils/http/cache/management.js');
    const key = generateCacheKey('gh-api-file-content', {
      owner: 'o',
      repo: 'r',
      path: 'etag.ts',
      auth: 'fingerprint-a',
    });

    await withDataCacheConditional(
      key,
      async () => ({ value: { content: 'v1' }, etag: '"v1"' }),
      { ttl: 1 }
    );
    clearAllCache();
    await new Promise(resolve => setTimeout(resolve, 1100));

    let receivedEtag: string | undefined;
    const value = await withDataCacheConditional(
      key,
      async ({ ifNoneMatch }) => {
        receivedEtag = ifNoneMatch;
        return {
          value: { content: '' },
          etag: '"v1"',
          notModified: true,
        };
      },
      { ttl: 1 }
    );

    expect(receivedEtag).toBe('"v1"');
    expect(value).toEqual({ content: 'v1' });
  });

  it('fails open when the response root cannot be written', async () => {
    const { getDiskCacheRoot } =
      await import('../../../src/utils/http/cache/diskStore.js');
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { withDataCache } =
      await import('../../../src/utils/http/cache/dataCache.js');
    const responseRoot = getDiskCacheRoot();
    mkdirSync(dirname(responseRoot), { recursive: true, mode: 0o700 });
    writeFileSync(responseRoot, 'not-a-directory', { mode: 0o600 });
    const key = generateCacheKey('gh-api-prs', { query: 'readonly' });

    const value = await withDataCache(key, async () => 'upstream', {
      ttl: 60,
    });

    expect(value).toBe('upstream');
  });

  it('keeps concurrent writes atomic and enforces the entry bound', async () => {
    process.env.OCTOCODE_DISK_CACHE_MAX_ENTRIES = '1';
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const {
      writeDiskCache,
      readDiskCache,
      getDiskCacheEntryPath,
      sweepDiskCacheIfDue,
    } = await import('../../../src/utils/http/cache/diskStore.js');
    const keyA = generateCacheKey('gh-api-prs', { query: 'atomic-a' });
    const keyB = generateCacheKey('gh-api-prs', { query: 'atomic-b' });

    await Promise.all(
      Array.from({ length: 10 }, (_, value) =>
        writeDiskCache(keyA, { value }, 60)
      )
    );
    const first = await readDiskCache<{ value: number }>(keyA);
    expect(first?.value.value).toBeGreaterThanOrEqual(0);
    expect(first?.value.value).toBeLessThan(10);
    expect(
      readdirSync(dirname(getDiskCacheEntryPath(keyA))).filter(name =>
        name.includes('.tmp-')
      )
    ).toEqual([]);

    await new Promise(resolve => setTimeout(resolve, 5));
    await writeDiskCache(keyB, { value: 10 }, 60);
    expect(await sweepDiskCacheIfDue(home, Date.now())).toBe(1);
    expect(
      [keyA, keyB].filter(key => existsSync(getDiskCacheEntryPath(key)))
    ).toHaveLength(1);
  });

  it('sweeps expired response entries without touching unrelated tmp state', async () => {
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const { writeDiskCache, sweepDiskCacheIfDue, getDiskCacheEntryPath } =
      await import('../../../src/utils/http/cache/diskStore.js');
    const key = generateCacheKey('gh-api-prs', { query: 'expired' });
    await writeDiskCache(key, { value: 1 }, 0.001, {
      staleTtlSeconds: 0.001,
    });
    const sentinel = join(home, 'tmp', 'plan', 'keep.txt');
    mkdirSync(dirname(sentinel), { recursive: true });
    writeFileSync(sentinel, 'keep');
    await new Promise(resolve => setTimeout(resolve, 10));

    const evicted = await sweepDiskCacheIfDue(home, Date.now());

    expect(evicted).toBe(1);
    expect(existsSync(getDiskCacheEntryPath(key))).toBe(false);
    expect(existsSync(sentinel)).toBe(true);
  });

  it('allows the shared lifecycle to force a sweep inside the response due window', async () => {
    const {
      sweepDiskCacheIfDue,
      writeDiskCache,
      getDiskCacheEntryPath,
      sweepDiskCacheNow,
    } = await import('../../../src/utils/http/cache/diskStore.js');
    const { generateCacheKey } =
      await import('../../../src/utils/http/cache/key.js');
    const now = Date.now();
    await sweepDiskCacheIfDue(home, now);
    const key = generateCacheKey('gh-api-prs', { query: 'forced' });
    await writeDiskCache(key, { value: 1 }, 0.001, {
      staleTtlSeconds: 0.001,
    });
    await new Promise(resolve => setTimeout(resolve, 10));

    expect(await sweepDiskCacheIfDue(home, Date.now())).toBe(0);
    expect(existsSync(getDiskCacheEntryPath(key))).toBe(true);
    expect(await sweepDiskCacheNow(home, Date.now())).toBe(1);
    expect(existsSync(getDiskCacheEntryPath(key))).toBe(false);
  });
});
