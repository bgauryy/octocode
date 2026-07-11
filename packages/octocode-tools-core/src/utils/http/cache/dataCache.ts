import {
  cache,
  cacheStats,
  cleanupStalePendingRequests,
  getTTLForPrefix,
  pendingRequests,
  recordGitHubCacheHit,
  safeCacheSet,
} from './store.js';

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
        let ttl = options.ttl;
        if (!ttl) {
          const prefixMatch = cacheKey.match(/^v\d+-([^:]+):/);
          const prefix = prefixMatch?.[1] ?? 'default';
          ttl = getTTLForPrefix(prefix);
        }
        safeCacheSet(cacheKey, result, ttl);
      }

      return result;
    } finally {
      pendingRequests.delete(cacheKey);
    }
  })();

  pendingRequests.set(cacheKey, { promise, startedAt: Date.now() });
  return promise as Promise<T>;
}
