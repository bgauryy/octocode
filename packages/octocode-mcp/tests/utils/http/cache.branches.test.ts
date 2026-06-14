import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  withDataCache,
  clearAllCache,
  generateCacheKey,
  getCacheStats,
  clearLocalToolCache,
  clearLSPToolCache,
  clearRemoteAPICache,
} from '../../../../octocode-tools-core/src/utils/http/cache.js';

describe('cache - branch coverage', () => {
  beforeEach(() => {
    clearAllCache();
  });

  describe('withDataCache', () => {
    it('should skip cache when skipCache is true', async () => {
      const operation = vi.fn().mockResolvedValue('fresh');
      const result = await withDataCache('skip-key', operation, {
        skipCache: true,
      });
      expect(result).toBe('fresh');
      expect(operation).toHaveBeenCalledTimes(1);
    });

    it('should return cached value on second call', async () => {
      const operation = vi.fn().mockResolvedValue('cached-val');
      const key = generateCacheKey('test-prefix', { q: 'cache-hit' });

      await withDataCache(key, operation);
      const result = await withDataCache(key, operation);

      expect(result).toBe('cached-val');
      expect(operation).toHaveBeenCalledTimes(1);
    });

    it('should bypass cache on forceRefresh', async () => {
      const operation = vi
        .fn()
        .mockResolvedValueOnce('v1')
        .mockResolvedValueOnce('v2');
      const key = generateCacheKey('test-prefix', { q: 'force' });

      await withDataCache(key, operation);
      const result = await withDataCache(key, operation, {
        forceRefresh: true,
      });

      expect(result).toBe('v2');
      expect(operation).toHaveBeenCalledTimes(2);
    });

    it('should use custom TTL when provided', async () => {
      const operation = vi.fn().mockResolvedValue('ttl-val');
      const key = generateCacheKey('test-prefix', { q: 'ttl' });

      await withDataCache(key, operation, { ttl: 10 });

      const stats = getCacheStats();
      expect(stats.sets).toBeGreaterThanOrEqual(1);
    });

    it('should not cache when shouldCache returns false', async () => {
      const operation = vi.fn().mockResolvedValue(null);
      const key = generateCacheKey('test-prefix', { q: 'no-cache' });

      await withDataCache(key, operation, {
        shouldCache: val => val !== null,
      });

      getCacheStats();

      const operation2 = vi.fn().mockResolvedValue('second');
      await withDataCache(key, operation2);
      expect(operation2).toHaveBeenCalledTimes(1);
    });

    it('should deduplicate concurrent requests for the same key', async () => {
      let resolveOp: (val: string) => void;
      const operation = vi.fn(() => new Promise<string>(r => (resolveOp = r)));
      const key = generateCacheKey('test-prefix', { q: 'dedup' });

      const p1 = withDataCache(key, operation);
      const p2 = withDataCache(key, operation);

      resolveOp!('deduped');
      const [r1, r2] = await Promise.all([p1, p2]);

      expect(r1).toBe('deduped');
      expect(r2).toBe('deduped');
      expect(operation).toHaveBeenCalledTimes(1);
    });

    it('should clean up pending request after completion', async () => {
      const operation = vi.fn().mockResolvedValue('done');
      const key = generateCacheKey('test-prefix', { q: 'cleanup' });

      await withDataCache(key, operation);

      const operation2 = vi.fn().mockResolvedValue('done2');
      const result = await withDataCache(key, operation2);
      expect(result).toBe('done');
      expect(operation2).not.toHaveBeenCalled();
    });

    it('should handle operation error and clean up pending', async () => {
      const operation = vi.fn().mockRejectedValue(new Error('fail'));
      const key = generateCacheKey('test-prefix', { q: 'error' });

      await expect(withDataCache(key, operation)).rejects.toThrow('fail');

      const operation2 = vi.fn().mockResolvedValue('recovered');
      const result = await withDataCache(key, operation2);
      expect(result).toBe('recovered');
    });
  });

  describe('generateCacheKey', () => {
    it('should generate different keys for different params', () => {
      const key1 = generateCacheKey('prefix', { a: 1 });
      const key2 = generateCacheKey('prefix', { a: 2 });
      expect(key1).not.toBe(key2);
    });

    it('should include sessionId in key generation', () => {
      const key1 = generateCacheKey('prefix', { a: 1 }, 'session1');
      const key2 = generateCacheKey('prefix', { a: 1 }, 'session2');
      expect(key1).not.toBe(key2);
    });

    it('should handle null and undefined params', () => {
      const key1 = generateCacheKey('prefix', null);
      const key2 = generateCacheKey('prefix', undefined);
      expect(key1).not.toBe(key2);
    });

    it('should handle nested objects with stable key ordering', () => {
      const key1 = generateCacheKey('prefix', { b: 2, a: 1 });
      const key2 = generateCacheKey('prefix', { a: 1, b: 2 });
      expect(key1).toBe(key2);
    });

    it('should handle arrays', () => {
      const key = generateCacheKey('prefix', [1, 'two', { three: 3 }]);
      expect(key).toContain('prefix');
    });
  });

  describe('clearAllCache', () => {
    it('should reset all stats', async () => {
      const key = generateCacheKey('test-prefix', { q: 'stats' });
      await withDataCache(key, async () => 'val');

      clearAllCache();
      const stats = getCacheStats();
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.sets).toBe(0);
    });
  });

  describe('prefix cache cleanup', () => {
    it('should clear only local tool cache entries', async () => {
      const localKey = generateCacheKey('local-search', { q: 'local' });
      const lspKey = generateCacheKey('lsp-definition', { q: 'lsp' });

      await withDataCache(localKey, async () => 'local');
      await withDataCache(lspKey, async () => 'lsp');

      const cleared = clearLocalToolCache();
      expect(cleared).toBe(1);

      const localOp = vi.fn().mockResolvedValue('local-fresh');
      const lspOp = vi.fn().mockResolvedValue('lsp-fresh');
      await withDataCache(localKey, localOp);
      await withDataCache(lspKey, lspOp);

      expect(localOp).toHaveBeenCalledTimes(1);
      expect(lspOp).not.toHaveBeenCalled();
    });

    it('should clear only lsp cache entries', async () => {
      const localKey = generateCacheKey('local-search', { q: 'local-2' });
      const lspKey = generateCacheKey('lsp-references', { q: 'lsp-2' });

      await withDataCache(localKey, async () => 'local');
      await withDataCache(lspKey, async () => 'lsp');

      const cleared = clearLSPToolCache();
      expect(cleared).toBe(1);

      const localOp = vi.fn().mockResolvedValue('local-fresh');
      const lspOp = vi.fn().mockResolvedValue('lsp-fresh');
      await withDataCache(localKey, localOp);
      await withDataCache(lspKey, lspOp);

      expect(localOp).not.toHaveBeenCalled();
      expect(lspOp).toHaveBeenCalledTimes(1);
    });

    it('should clear remote API cache entries across providers', async () => {
      const ghKey = generateCacheKey('gh-api-code', { q: 'gh' });
      const bbKey = generateCacheKey('bb-api-code', { q: 'bb' });
      const npmKey = generateCacheKey('npm-search', { q: 'pkg' });
      const localKey = generateCacheKey('local-file-content', { q: 'local' });

      await withDataCache(ghKey, async () => ({ ok: 'gh' }));
      await withDataCache(bbKey, async () => ({ ok: 'bb' }));
      await withDataCache(npmKey, async () => ({ ok: 'npm' }));
      await withDataCache(localKey, async () => ({ ok: 'local' }));

      const cleared = clearRemoteAPICache();
      expect(cleared).toBe(3);

      const ghOp = vi.fn().mockResolvedValue({ fresh: 'gh' });
      const bbOp = vi.fn().mockResolvedValue({ fresh: 'bb' });
      const npmOp = vi.fn().mockResolvedValue({ fresh: 'npm' });
      const localOp = vi.fn().mockResolvedValue({ fresh: 'local' });

      await withDataCache(ghKey, ghOp);
      await withDataCache(bbKey, bbOp);
      await withDataCache(npmKey, npmOp);
      await withDataCache(localKey, localOp);

      expect(ghOp).toHaveBeenCalledTimes(1);
      expect(bbOp).toHaveBeenCalledTimes(1);
      expect(npmOp).toHaveBeenCalledTimes(1);
      expect(localOp).not.toHaveBeenCalled();
    });
  });
});
