import NodeCache from 'node-cache';
import crypto from 'crypto';
import { incrementGitHubCacheHits } from 'octocode-shared';
import type { CacheStats } from '../core/types.js';

const VERSION = 'v1';

const PENDING_REQUEST_MAX_AGE_MS = 5 * 60 * 1000;

const cache = new NodeCache({
  stdTTL: 86400,
  checkperiod: 300,
  maxKeys: 5000,
  deleteOnExpire: true,
  useClones: false,
});

const cacheStats: CacheStats = {
  hits: 0,
  misses: 0,
  sets: 0,
  totalKeys: 0,
  lastReset: new Date(),
};

const CACHE_TTL_CONFIG = {
  'gh-api-code': 3600,
  'gh-api-repos': 7200,
  'gh-api-prs': 1800,
  'gh-api-file-content': 300,
  'gh-repo-structure-api': 7200,
  'github-user': 900,
  'npm-search': 14400,
  default: 86400,
} as const;

interface PendingRequest {
  promise: Promise<unknown>;
  startedAt: number;
}
const pendingRequests = new Map<string, PendingRequest>();

function extractCachePrefix(cacheKey: string): string | undefined {
  return cacheKey.match(/^v\d+-([^:]+):/)?.[1];
}

function isGitHubCachePrefix(prefix: string): boolean {
  return (
    prefix.startsWith('gh-api-') ||
    prefix.startsWith('gh-repo-') ||
    prefix === 'github-user'
  );
}

function recordGitHubCacheHit(cacheKey: string): void {
  const prefix = extractCachePrefix(cacheKey);
  if (!prefix || !isGitHubCachePrefix(prefix)) return;

  try {
    incrementGitHubCacheHits(prefix, 1);
  } catch {
    void 0;
  }
}

function cleanupStalePendingRequests(): void {
  const now = Date.now();
  for (const [key, pending] of pendingRequests.entries()) {
    if (now - pending.startedAt > PENDING_REQUEST_MAX_AGE_MS) {
      pendingRequests.delete(key);
    }
  }
}

const CACHE_KEY_EXCLUDED_FIELDS: ReadonlySet<string> = new Set([]);

function stripCacheKeyExcludedFields(params: unknown): unknown {
  if (params === null || typeof params !== 'object' || Array.isArray(params)) {
    return params;
  }
  const obj = params as Record<string, unknown>;
  let touched = false;
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(obj)) {
    if (CACHE_KEY_EXCLUDED_FIELDS.has(k)) {
      touched = true;
      continue;
    }
    out[k] = obj[k];
  }
  return touched ? out : params;
}

export function generateCacheKey(
  prefix: string,
  params: unknown,
  sessionId?: string
): string {
  const paramString = createStableParamString(
    stripCacheKeyExcludedFields(params)
  );

  const finalParamString = sessionId
    ? `${sessionId}:${paramString}`
    : paramString;

  const hash = crypto
    .createHash('sha256')
    .update(finalParamString)
    .digest('hex');

  return `${VERSION}-${prefix}:${hash}`;
}

function createStableParamString(
  params: unknown,
  visited: WeakSet<object> = new WeakSet()
): string {
  if (params === null) {
    return 'null';
  }

  if (params === undefined) {
    return 'undefined';
  }

  if (typeof params !== 'object') {
    return String(params);
  }

  if (visited.has(params as object)) {
    return '"[Circular]"';
  }
  visited.add(params as object);

  if (Array.isArray(params)) {
    return `[${params.map(p => createStableParamString(p, visited)).join(',')}]`;
  }

  const sortedKeys = Object.keys(params as Record<string, unknown>).sort();
  const sortedEntries = sortedKeys.map(key => {
    const value = (params as Record<string, unknown>)[key];
    return `"${key}":${createStableParamString(value, visited)}`;
  });

  return `{${sortedEntries.join(',')}}`;
}

function getTTLForPrefix(prefix: string): number {
  return (
    (CACHE_TTL_CONFIG as Record<string, number>)[prefix] ||
    CACHE_TTL_CONFIG.default
  );
}

function safeCacheSet(key: string, value: unknown, ttl: number): boolean {
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

export function clearAllCache(): void {
  cache.flushAll();
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
