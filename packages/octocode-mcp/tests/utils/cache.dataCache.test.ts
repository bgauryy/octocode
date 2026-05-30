import { describe, it, expect, beforeEach } from 'vitest';
import { incrementGitHubCacheHits } from 'octocode-shared';
import {
  generateCacheKey,
  withDataCache,
  clearAllCache,
  getCacheStats,
} from '../../src/utils/http/cache.js';

describe('withDataCache typed data cache', () => {
  beforeEach(() => {
    clearAllCache();
  });

  it('caches successful values and returns cached on next call', async () => {
    let calls = 0;
    const op = async () => {
      calls += 1;
      return { value: `run-${calls}` } as const;
    };

    const key = generateCacheKey('gh-api-code', { test: 'data' });
    const r1 = await withDataCache(key, op);
    const r2 = await withDataCache(key, op);

    expect(calls).toBe(1);
    expect(r1).toEqual(r2);

    const stats = getCacheStats();
    expect(stats.hits).toBe(1);
    expect(stats.misses).toBe(1);
    expect(stats.sets).toBe(1);
    expect(incrementGitHubCacheHits).toHaveBeenCalledWith('gh-api-code', 1);
  });

  it('respects skipCache and forceRefresh options', async () => {
    let calls = 0;
    const op = async () => {
      calls += 1;
      return { ok: true, run: calls } as const;
    };
    const key = generateCacheKey('gh-api-code', { mode: 'options' });

    // skipCache: always executes operation
    await withDataCache(key, op, { skipCache: true });
    await withDataCache(key, op, { skipCache: true });
    expect(calls).toBe(2);

    // Normal call caches
    await withDataCache(key, op);
    expect(calls).toBe(3);

    // Cached hit does not execute
    await withDataCache(key, op);
    expect(calls).toBe(3);

    // forceRefresh executes and overwrites cache
    await withDataCache(key, op, { forceRefresh: true });
    expect(calls).toBe(4);
  });

  it('uses shouldCache to decide whether to cache a value', async () => {
    let calls = 0;
    const op = async () => {
      calls += 1;
      return calls % 2 === 0 ? { data: calls } : { error: 'e', data: null };
    };

    const key = generateCacheKey('gh-api-code', { mode: 'should' });

    const a = await withDataCache(key, op, {
      shouldCache: v => !(v as { error?: unknown }).error,
    });
    expect(typeof (a as { error?: unknown }).error).toEqual('string');

    const b = await withDataCache(key, op, {
      shouldCache: v => !(v as { error?: unknown }).error,
    });
    expect((b as { error?: unknown }).error).toEqual(undefined);

    const before = calls;
    const c = await withDataCache(key, op, {
      shouldCache: v => !(v as { error?: unknown }).error,
    });
    expect(c).toEqual(b);
    expect(calls).toEqual(before);
  });

  it('should use default TTL for unrecognized cache key prefixes', async () => {
    let calls = 0;
    const op = async () => {
      calls += 1;
      return { result: calls };
    };

    // Use a key that doesn't match any known prefix pattern
    const key = 'unusual-key-without-prefix';

    const r1 = await withDataCache(key, op);
    const r2 = await withDataCache(key, op);

    expect(calls).toBe(1);
    expect(r1).toEqual(r2);
  });

  it('should extract TTL from known prefix in cache key', async () => {
    let calls = 0;
    const op = async () => {
      calls += 1;
      return { data: 'test' };
    };

    // Use a key with a known prefix format (v1-gh-api-code:...)
    const key = generateCacheKey('gh-api-code', { test: 'ttl-prefix' });

    const r1 = await withDataCache(key, op);
    const r2 = await withDataCache(key, op);

    expect(calls).toBe(1);
    expect(r1).toEqual(r2);
  });
});
