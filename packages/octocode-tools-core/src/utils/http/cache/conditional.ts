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
import { readDiskCache, writeDiskCache } from './diskStore.js';
import {
  markResponseCacheHit,
  runCacheLayer,
  type CacheRole,
} from './trace.js';

const ETAG_SOFT_TTL_SECONDS = 86400;

function resolveTTL(cacheKey: string, configured?: number): number {
  if (configured) return configured;
  const prefixMatch = cacheKey.match(/^v\d+-([^:]+):/);
  return getTTLForPrefix(prefixMatch?.[1] ?? 'default');
}

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

interface ConditionalCacheOptions<T> {
  ttl?: number;
  skipCache?: boolean;
  forceRefresh?: boolean;
  shouldCache?: (value: T) => boolean;
  /** Whether a hit supplies the tool response or only supporting metadata. */
  cacheRole?: CacheRole;
}

export async function withDataCacheConditional<T>(
  cacheKey: string,
  operation: (opts: {
    ifNoneMatch?: string;
  }) => Promise<ConditionalFetchResult<T>>,
  options: ConditionalCacheOptions<T> = {}
): Promise<T> {
  return runCacheLayer(
    () => withDataCacheConditionalInternal(cacheKey, operation, options),
    options.cacheRole
  );
}

async function withDataCacheConditionalInternal<T>(
  cacheKey: string,
  operation: (opts: {
    ifNoneMatch?: string;
  }) => Promise<ConditionalFetchResult<T>>,
  options: ConditionalCacheOptions<T>
): Promise<T> {
  if (options.skipCache) {
    const fresh = await operation({});
    return fresh.value;
  }

  let diskSoft:
    | { value: T; etag?: string; state: 'fresh' | 'stale'; expiresAt: number }
    | undefined;

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

    diskSoft = await readDiskCache<T>(cacheKey, { allowStale: true });
    if (diskSoft?.state === 'fresh') {
      cacheStats.hits++;
      recordGitHubCacheHit(cacheKey);
      markResponseCacheHit();
      safeCacheSet(
        cacheKey,
        diskSoft.value,
        Math.max(1, (diskSoft.expiresAt - Date.now()) / 1000)
      );
      if (diskSoft.etag) {
        etagSoftCache.set(cacheKey, {
          data: diskSoft.value,
          etag: diskSoft.etag,
        });
      }
      return diskSoft.value;
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
      const soft = options.forceRefresh
        ? undefined
        : etagSoftCache.get<{ data: T; etag?: string }>(cacheKey);
      const recoverable =
        soft ??
        (diskSoft ? { data: diskSoft.value, etag: diskSoft.etag } : undefined);
      const result = await operation({
        ifNoneMatch:
          !options.forceRefresh && recoverable?.etag
            ? recoverable.etag
            : undefined,
      });

      if (result.notModified && recoverable) {
        cacheStats.hits++;
        recordGitHubCacheHit(cacheKey);
        markResponseCacheHit();
        const ttl = resolveTTL(cacheKey, options.ttl);
        safeCacheSet(cacheKey, recoverable.data, ttl);
        await writeDiskCache(cacheKey, recoverable.data, ttl, {
          ...(recoverable.etag ? { etag: recoverable.etag } : {}),
          staleTtlSeconds: ETAG_SOFT_TTL_SECONDS,
        });
        return recoverable.data;
      }

      if (!options.forceRefresh) {
        cacheStats.misses++;
      }

      const shouldCache = options.shouldCache ?? (() => true);
      if (shouldCache(result.value)) {
        const ttl = resolveTTL(cacheKey, options.ttl);
        safeCacheSet(cacheKey, result.value, ttl);
        etagSoftCache.set(cacheKey, {
          data: result.value,
          etag: result.etag,
        });
        await writeDiskCache(cacheKey, result.value, ttl, {
          ...(result.etag ? { etag: result.etag } : {}),
          staleTtlSeconds: ETAG_SOFT_TTL_SECONDS,
        });
      }

      return result.value;
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
