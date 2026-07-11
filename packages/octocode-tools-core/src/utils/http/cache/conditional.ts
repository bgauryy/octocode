import NodeCache from 'node-cache';
import {
  cache,
  cacheStats,
  cleanupStalePendingRequests,
  getTTLForPrefix,
  pendingRequests,
  recordGitHubCacheHit,
  safeCacheSet,
} from './store.js';

/**
 * Soft ETag store: survives primary TTL so a miss can send If-None-Match and
 * recover a 304 without paying a full body download (GitHub authorized 304s
 * do not burn primary rate-limit quota).
 */
export const etagSoftCache = new NodeCache({
  stdTTL: 86400,
  checkperiod: 300,
  maxKeys: 5000,
  deleteOnExpire: true,
  useClones: false,
});

export type ConditionalFetchResult<T> = {
  value: T;
  etag?: string;
  /** True when the upstream responded 304 Not Modified. */
  notModified?: boolean;
};

export async function withDataCacheConditional<T>(
  cacheKey: string,
  operation: (opts: {
    ifNoneMatch?: string;
  }) => Promise<ConditionalFetchResult<T>>,
  options: {
    ttl?: number;
    skipCache?: boolean;
    forceRefresh?: boolean;
    shouldCache?: (value: T) => boolean;
  } = {}
): Promise<T> {
  if (options.skipCache) {
    const fresh = await operation({});
    return fresh.value;
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
      const soft = options.forceRefresh
        ? undefined
        : etagSoftCache.get<{ data: T; etag?: string }>(cacheKey);
      const result = await operation({
        ifNoneMatch:
          !options.forceRefresh && soft?.etag ? soft.etag : undefined,
      });

      if (result.notModified && soft) {
        cacheStats.hits++;
        recordGitHubCacheHit(cacheKey);
        let ttl = options.ttl;
        if (!ttl) {
          const prefixMatch = cacheKey.match(/^v\d+-([^:]+):/);
          const prefix = prefixMatch?.[1] ?? 'default';
          ttl = getTTLForPrefix(prefix);
        }
        safeCacheSet(cacheKey, soft.data, ttl);
        return soft.data;
      }

      if (!options.forceRefresh) {
        cacheStats.misses++;
      }

      const shouldCache = options.shouldCache ?? (() => true);
      if (shouldCache(result.value)) {
        let ttl = options.ttl;
        if (!ttl) {
          const prefixMatch = cacheKey.match(/^v\d+-([^:]+):/);
          const prefix = prefixMatch?.[1] ?? 'default';
          ttl = getTTLForPrefix(prefix);
        }
        safeCacheSet(cacheKey, result.value, ttl);
        etagSoftCache.set(cacheKey, {
          data: result.value,
          etag: result.etag,
        });
      }

      return result.value;
    } finally {
      pendingRequests.delete(cacheKey);
    }
  })();

  pendingRequests.set(cacheKey, { promise, startedAt: Date.now() });
  return promise as Promise<T>;
}
