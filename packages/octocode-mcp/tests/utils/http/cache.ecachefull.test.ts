import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockCacheGet = vi.fn();
const mockCacheSet = vi.fn();
const mockCacheKeys = vi.fn().mockReturnValue([]);
const mockCacheFlushAll = vi.fn();

class MockNodeCache {
  get = mockCacheGet;
  set = mockCacheSet;
  keys = mockCacheKeys;
  flushAll = mockCacheFlushAll;
}

vi.mock('node-cache', () => ({
  default: MockNodeCache,
}));

const { withDataCache, clearAllCache, generateCacheKey } =
  await import('../../../src/utils/http/cache.js');

describe('cache ECACHEFULL handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCacheGet.mockReturnValue(undefined);
    mockCacheSet.mockImplementation(() => true);
    mockCacheKeys.mockReturnValue([]);
    clearAllCache();
  });

  it('should handle ECACHEFULL by evicting and retrying', async () => {
    let callCount = 0;
    mockCacheSet.mockImplementation(() => {
      callCount++;
      if (callCount === 1) {
        throw new Error('ECACHEFULL');
      }
      return true;
    });
    mockCacheKeys.mockReturnValue(['old-key-1', 'old-key-2']);
    mockCacheGet.mockReturnValue(undefined);

    const key = generateCacheKey('test', { q: 'full' });
    const result = await withDataCache(key, async () => 'value-after-eviction');

    expect(result).toBe('value-after-eviction');
    expect(mockCacheSet).toHaveBeenCalledTimes(2);
  });

  it('should skip caching when ECACHEFULL persists after eviction', async () => {
    mockCacheSet.mockImplementation(() => {
      throw new Error('ECACHEFULL');
    });
    mockCacheKeys.mockReturnValue(['key1', 'key2']);
    mockCacheGet.mockReturnValue(undefined);

    const key = generateCacheKey('test', { q: 'full-persist' });
    const result = await withDataCache(key, async () => 'still-works');

    expect(result).toBe('still-works');
  });
});

describe('stale pending request cleanup', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCacheGet.mockReturnValue(undefined);
    mockCacheSet.mockImplementation(() => true);
    mockCacheKeys.mockReturnValue([]);
    clearAllCache();
  });

  it('should clean up stale pending requests older than 5 minutes', async () => {
    const originalDateNow = Date.now;
    let fakeTime = 1_000_000_000;
    Date.now = () => fakeTime;

    const key1 = generateCacheKey('test', { q: 'stale-request' });
    let resolve1: (val: string) => void;
    const slowOp = new Promise<string>(r => (resolve1 = r));

    const p1 = withDataCache(key1, () => slowOp);

    fakeTime += 6 * 60 * 1000;

    const key2 = generateCacheKey('test', { q: 'trigger-cleanup' });
    const result2 = await withDataCache(key2, async () => 'fresh');

    expect(result2).toBe('fresh');

    resolve1!('done');
    await p1;

    Date.now = originalDateNow;
  });
});
