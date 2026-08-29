import {
  cache,
  cacheStats,
  cleanupStalePendingRequests,
  getTTLForPrefix,
  pendingRequests,
  recordGitHubCacheHit,
  safeCacheSet,
} from './store.js';
import { readDiskCache, writeDiskCache } from './diskStore.js';

function resolveTTL(cacheKey: string, configured?: number): number {
  if (configured) return configured;
  const prefixMatch = cacheKey.match(/^v\d+-([^:]+):/);
  return getTTLForPrefix(prefixMatch?.[1] ?? 'default');
}

export async function withDataCache<T>(
  cacheKey: string,
  operation: () => Promise<T>,
  options: {
    ttl?: number;
    skipCache?: boolean;
    forceRefresh?: boolean;
    shouldCache?: (value: T) => boolean;
  } = {}
): Promise<T> {
  if (options.skipCache) {
    return await operation();
  }

  if (!options.forceRefresh) {
    try {
      const cached = cache.get<T>(cacheKey);
      if (cached !== undefined) {
        cacheStats.hits++;
        recordGitHubCacheHit(cacheKey);
        return cached;
      }
    } catch {
      void 0;
    }

    const disk = await readDiskCache<T>(cacheKey);
    if (disk?.state === 'fresh') {
      cacheStats.hits++;
      recordGitHubCacheHit(cacheKey);
      safeCacheSet(
        cacheKey,
        disk.value,
        Math.max(1, (disk.expiresAt - Date.now()) / 1000)
      );
      return disk.value;
    }
  }

  cleanupStalePendingRequests();

  const existingPending = pendingRequests.get(cacheKey);
  if (existingPending) {
    return existingPending.promise as Promise<T>;
  }

  const promise = (async () => {
    try {
      const result = await operation();

      if (!options.forceRefresh) {
        cacheStats.misses++;
      }

      const shouldCache = options.shouldCache ?? (() => true);
      if (shouldCache(result)) {
        const ttl = resolveTTL(cacheKey, options.ttl);
        safeCacheSet(cacheKey, result, ttl);
        await writeDiskCache(cacheKey, result, ttl);
      }

      return result;
    } finally {
      pendingRequests.delete(cacheKey);
    }
  })();

  pendingRequests.set(cacheKey, { promise, startedAt: Date.now() });
  return promise as Promise<T>;
}
