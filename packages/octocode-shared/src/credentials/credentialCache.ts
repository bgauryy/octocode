import type { StoredCredentials } from './types.js';
import { isTokenExpired, normalizeHostname } from './credentialUtils.js';

interface CachedCredentials {
  credentials: StoredCredentials | null;
  cachedAt: number;
}

const credentialsCache = new Map<string, CachedCredentials>();

const CACHE_TTL_MS = 5 * 60 * 1000;

function isCacheValid(hostname: string): boolean {
  const normalizedHostname = normalizeHostname(hostname);
  const cached = credentialsCache.get(normalizedHostname);
  if (!cached) return false;

  const age = Date.now() - cached.cachedAt;
  if (age >= CACHE_TTL_MS) return false;

  if (!cached.credentials) {
    return true;
  }

  return !isTokenExpired(cached.credentials);
}

export function invalidateCredentialsCache(hostname?: string): void {
  if (hostname) {
    credentialsCache.delete(normalizeHostname(hostname));
  } else {
    credentialsCache.clear();
  }
}

export function _getCacheStats(): {
  size: number;
  entries: Array<{ hostname: string; age: number; valid: boolean }>;
} {
  const now = Date.now();
  return {
    size: credentialsCache.size,
    entries: Array.from(credentialsCache.entries()).map(
      ([hostname, entry]) => ({
        hostname,
        age: now - entry.cachedAt,
        valid: isCacheValid(hostname),
      })
    ),
  };
}

export function _resetCredentialsCache(): void {
  credentialsCache.clear();
}

export function getCachedCredentials(
  hostname: string
): StoredCredentials | null | undefined {
  const normalizedHostname = normalizeHostname(hostname);

  if (isCacheValid(normalizedHostname)) {
    return credentialsCache.get(normalizedHostname)!.credentials;
  }

  return undefined;
}

export function setCachedCredentials(
  hostname: string,
  credentials: StoredCredentials | null
): void {
  const normalizedHostname = normalizeHostname(hostname);
  credentialsCache.set(normalizedHostname, {
    credentials,
    cachedAt: Date.now(),
  });
}
