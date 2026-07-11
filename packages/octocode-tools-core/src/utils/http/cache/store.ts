import NodeCache from 'node-cache';
import { incrementGitHubCacheHits } from '../../../shared/index.js';
import type { CacheStats } from '../../core/types.js';

export const PENDING_REQUEST_MAX_AGE_MS = 5 * 60 * 1000;

export const cache = new NodeCache({
  stdTTL: 86400,
  checkperiod: 300,
  maxKeys: 5000,
  deleteOnExpire: true,
  useClones: false,
});

export const cacheStats: CacheStats = {
  hits: 0,
  misses: 0,
  sets: 0,
  totalKeys: 0,
  lastReset: new Date(),
};

export const CACHE_TTL_CONFIG = {
  'gh-api-code': 3600,
  'gh-api-repos': 7200,
  'gh-api-prs': 1800,
  'gh-api-issues': 1800,
  'gh-api-history': 1800,
  'gh-api-releases': 3600,
  'gh-api-file-content': 300,
  'gh-repo-structure-api': 7200,
  'github-user': 900,
  'npm-search': 14400,
  default: 86400,
} as const;

export interface PendingRequest {
  promise: Promise<unknown>;
  startedAt: number;
}
export const pendingRequests = new Map<string, PendingRequest>();

export function extractCachePrefix(cacheKey: string): string | undefined {
  return cacheKey.match(/^v\d+-([^:]+):/)?.[1];
}

function isGitHubCachePrefix(prefix: string): boolean {
  return (
    prefix.startsWith('gh-api-') ||
    prefix.startsWith('gh-repo-') ||
    prefix === 'github-user'
  );
}

export function recordGitHubCacheHit(cacheKey: string): void {
  const prefix = extractCachePrefix(cacheKey);
  if (!prefix || !isGitHubCachePrefix(prefix)) return;

  try {
    incrementGitHubCacheHits(prefix, 1);
  } catch {
    void 0;
  }
}

export function cleanupStalePendingRequests(): void {
  const now = Date.now();
  for (const [key, pending] of pendingRequests.entries()) {
    if (now - pending.startedAt > PENDING_REQUEST_MAX_AGE_MS) {
      pendingRequests.delete(key);
    }
  }
}

export function getTTLForPrefix(prefix: string): number {
  return (
    (CACHE_TTL_CONFIG as Record<string, number>)[prefix] ||
    CACHE_TTL_CONFIG.default
  );
}

export function safeCacheSet(
  key: string,
  value: unknown,
  ttl: number
): boolean {
  try {
    cache.set(key, value, ttl);
    cacheStats.sets++;
    cacheStats.totalKeys = cache.keys().length;
    return true;
  } catch {
    try {
      const keys = cache.keys();
      for (const k of keys) {
        cache.get(k);
      }
      cache.set(key, value, ttl);
      cacheStats.sets++;
      cacheStats.totalKeys = cache.keys().length;
      return true;
    } catch {
      return false;
    }
  }
}
