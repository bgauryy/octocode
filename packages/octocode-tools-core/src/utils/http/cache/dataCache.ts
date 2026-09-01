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
import {
  markResponseCacheHit,
  runCacheLayer,
  type CacheRole,
} from './trace.js';

interface DataCacheOptions<T> {
  ttl?: number;
  skipCache?: boolean;
  forceRefresh?: boolean;
  shouldCache?: (value: T) => boolean;
  /** Whether a hit supplies the tool response or only supporting metadata. */
  cacheRole?: CacheRole;
}

function resolveTTL(cacheKey: string, configured?: number): number {
  if (configured) return configured;
  const prefixMatch = cacheKey.match(/^v\d+-([^:]+):/);
  return getTTLForPrefix(prefixMatch?.[1] ?? 'default');
}

export async function withDataCache<T>(
  cacheKey: string,
  operation: () => Promise<T>,
  options: DataCacheOptions<T> = {}
): Promise<T> {
  return runCacheLayer(
    () => withDataCacheInternal(cacheKey, operation, options),
    options.cacheRole
  );
}

async function withDataCacheInternal<T>(
  cacheKey: string,
  operation: () => Promise<T>,
  options: DataCacheOptions<T>
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
        markResponseCacheHit();
        return cached;
      }
    } catch {
      void 0;
    }

    const disk = await readDiskCache<T>(cacheKey);
    if (disk?.state === 'fresh') {
      cacheStats.hits++;
      recordGitHubCacheHit(cacheKey);
      markResponseCacheHit();
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

  const requestId = Symbol(cacheKey);
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
      if (pendingRequests.get(cacheKey)?.requestId === requestId) {
        pendingRequests.delete(cacheKey);
      }
    }
  })();

  pendingRequests.set(cacheKey, {
    promise,
    startedAt: Date.now(),
    requestId,
  });
  return promise;
}
