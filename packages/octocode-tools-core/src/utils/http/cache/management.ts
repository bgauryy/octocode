import type { CacheStats } from '../../core/types.js';
import { cache, cacheStats, pendingRequests } from './store.js';
import { etagSoftCache } from './conditional.js';

export function clearAllCache(): void {
  cache.flushAll();
  etagSoftCache.flushAll();
  pendingRequests.clear();

  cacheStats.hits = 0;
  cacheStats.misses = 0;
  cacheStats.sets = 0;
  cacheStats.totalKeys = 0;
  cacheStats.lastReset = new Date();
}

function clearCacheByPrefix(prefix: string): number {
  const keys = cache.keys();
  let cleared = 0;

  for (const key of keys) {
    const keyPrefixMatch = key.match(/^v\d+-([^:]+):/);
    const keyPrefix = keyPrefixMatch?.[1];
    if (!keyPrefix || !keyPrefix.startsWith(prefix)) continue;

    if (cache.del(key) > 0) {
      cleared++;
    }
  }

  if (cleared > 0) {
    cacheStats.totalKeys = cache.keys().length;
  }

  return cleared;
}

export function clearLocalToolCache(): number {
  return clearCacheByPrefix('local-');
}

export function clearLSPToolCache(): number {
  return clearCacheByPrefix('lsp-');
}

export function clearRemoteAPICache(): number {
  let cleared = 0;
  cleared += clearCacheByPrefix('gh-api-');
  cleared += clearCacheByPrefix('bb-api-');
  cleared += clearCacheByPrefix('gh-repo-');
  cleared += clearCacheByPrefix('bb-repo-');
  cleared += clearCacheByPrefix('github-user');
  cleared += clearCacheByPrefix('npm-search');
  etagSoftCache.flushAll();
  return cleared;
}

export function getCacheStats(): CacheStats & {
  hitRate: number;
  cacheSize: number;
} {
  const total = cacheStats.hits + cacheStats.misses;
  return {
    ...cacheStats,
    hitRate: total > 0 ? (cacheStats.hits / total) * 100 : 0,
    cacheSize: cache.keys().length,
  };
}
