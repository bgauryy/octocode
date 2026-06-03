/**
 * Token Storage Utility
 *
 * Stores OAuth tokens securely using encrypted file storage (~/.octocode/credentials.json).
 * Uses AES-256-GCM encryption with a random key stored in ~/.octocode/.key.
 *
 * This provides a pure JavaScript solution that works across all environments
 * (CI, containers, SSH, desktop) without native dependencies.
 *
 * This file orchestrates credential management by delegating to focused modules:
 * - credentialCache.ts: In-memory cache management
 * - credentialEncryption.ts: Encryption/decryption and file I/O
 * - tokenRefresh.ts: OAuth token refresh logic
 * - tokenResolution.ts: Token resolution with priority chain
 * - credentialUtils.ts: Shared utility functions
 */

import type { StoredCredentials, StoreResult, DeleteResult } from './types.js';
import { createLogger } from '../logger/index.js';

import {
  invalidateCredentialsCache,
  _getCacheStats,
  _resetCredentialsCache,
  getCachedCredentials,
  setCachedCredentials,
} from './credentialCache.js';
import {
  OCTOCODE_DIR,
  CREDENTIALS_FILE,
  KEY_FILE,
  encrypt,
  decrypt,
  ensureOctocodeDir,
  cleanupKeyFile,
  readCredentialsStore,
  writeCredentialsStore,
} from './credentialEncryption.js';
import {
  refreshAuthToken as _refreshAuthTokenCore,
  type RefreshResult,
  getTokenWithRefresh as _getTokenWithRefreshCore,
  type TokenWithRefreshResult,
} from './tokenRefresh.js';
import {
  initTokenResolution,
  resolveToken,
  type ResolvedToken,
  resolveTokenWithRefresh,
  type ResolvedTokenWithRefresh,
  resolveTokenFull,
  type FullTokenResolution,
  type GhCliTokenGetter,
  resetTokenResolution,
} from './tokenResolution.js';
import {
  getTokenFromEnv,
  getEnvTokenSource,
  hasEnvToken,
  ENV_TOKEN_VARS,
} from './envTokens.js';
import {
  normalizeHostname,
  isTokenExpired,
  isRefreshTokenExpired,
} from './credentialUtils.js';

const logger = createLogger('token-storage');

// Re-export env functions from envTokens.ts (backward compat)
export { getTokenFromEnv, getEnvTokenSource, hasEnvToken, ENV_TOKEN_VARS };

/**
 * Store credentials using encrypted file storage
 *
 * @returns StoreResult with success status
 */
export async function storeCredentials(
  credentials: StoredCredentials
): Promise<StoreResult> {
  const hostname = normalizeHostname(credentials.hostname);
  const normalizedCredentials: StoredCredentials = {
    ...credentials,
    hostname,
    updatedAt: new Date().toISOString(),
  };

  try {
    const store = readCredentialsStore();
    store.credentials[hostname] = normalizedCredentials;
    writeCredentialsStore(store);

    // Invalidate cache for this hostname
    invalidateCredentialsCache(hostname);

    return { success: true };
  } catch (fileError) {
    const errorMsg =
      fileError instanceof Error ? fileError.message : String(fileError);
    logger.error('CRITICAL: Storage failed', {
      error: errorMsg
        .replace(
          /\b(ghp_|gho_|ghu_|ghs_|ghr_)[a-zA-Z0-9]{36,}\b/g,
          '***MASKED***'
        )
        .replace(/\b[a-zA-Z0-9]{40,}\b/g, '***MASKED***'),
    });
    throw new Error('Failed to store credentials');
  }
}

/**
 * Options for getCredentials
 */
export interface GetCredentialsOptions {
  /** Bypass cache and fetch fresh credentials from storage */
  bypassCache?: boolean;
}

/**
 * Get credentials from encrypted file storage
 *
 * Flow:
 * 1. Check in-memory cache (unless bypassed)
 * 2. Read from file storage
 * 3. Cache result for future calls
 *
 * @param hostname - GitHub hostname (default: 'github.com')
 * @param options - Optional settings (e.g., bypassCache)
 * @returns Stored credentials or null if not found
 */
export async function getCredentials(
  hostname: string = 'github.com',
  options?: GetCredentialsOptions
): Promise<StoredCredentials | null> {
  const normalizedHostname = normalizeHostname(hostname);

  // 1. Check cache first (unless bypassed)
  if (!options?.bypassCache) {
    const cached = getCachedCredentials(normalizedHostname);
    if (cached !== undefined) {
      return cached;
    }
  }

  // 2. Fetch from file storage
  const store = readCredentialsStore();
  const credentials = store.credentials[normalizedHostname] || null;

  // 3. Update cache, including misses, to avoid repeated disk reads
  setCachedCredentials(normalizedHostname, credentials);

  return credentials;
}

/**
 * Get credentials synchronously (file storage only)
 *
 * @param hostname - GitHub hostname (default: 'github.com')
 * @returns Stored credentials from file or null if not found
 */
export function getCredentialsSync(
  hostname: string = 'github.com'
): StoredCredentials | null {
  const normalizedHostname = normalizeHostname(hostname);
  const store = readCredentialsStore();
  return store.credentials[normalizedHostname] || null;
}

/**
 * Delete credentials from file storage
 *
 * @returns DeleteResult with details about what was deleted
 */
export async function deleteCredentials(
  hostname: string = 'github.com'
): Promise<DeleteResult> {
  const normalizedHostname = normalizeHostname(hostname);
  let deletedFromFile = false;

  // Delete from file storage
  const store = readCredentialsStore();
  if (store.credentials[normalizedHostname]) {
    delete store.credentials[normalizedHostname];

    // Clean up files if no more credentials remain
    if (Object.keys(store.credentials).length === 0) {
      cleanupKeyFile();
    } else {
      writeCredentialsStore(store);
    }
    deletedFromFile = true;
  }

  // Invalidate cache for this hostname
  invalidateCredentialsCache(normalizedHostname);

  return {
    success: deletedFromFile,
    deletedFromFile,
  };
}

/**
 * List all stored hostnames from file storage
 */
export async function listStoredHosts(): Promise<string[]> {
  const store = readCredentialsStore();
  return Object.keys(store.credentials);
}

/**
 * List stored hosts synchronously (file storage only)
 */
export function listStoredHostsSync(): string[] {
  const store = readCredentialsStore();
  return Object.keys(store.credentials);
}

/**
 * Check if credentials exist for a hostname
 */
export async function hasCredentials(
  hostname: string = 'github.com'
): Promise<boolean> {
  return (await getCredentials(hostname)) !== null;
}

/**
 * Check if credentials exist synchronously (file storage only)
 */
export function hasCredentialsSync(hostname: string = 'github.com'): boolean {
  return getCredentialsSync(hostname) !== null;
}

/**
 * Update token for a hostname (used for refresh)
 */
export async function updateToken(
  hostname: string,
  token: StoredCredentials['token']
): Promise<boolean> {
  const credentials = await getCredentials(hostname);

  if (!credentials) {
    return false;
  }

  credentials.token = token;
  credentials.updatedAt = new Date().toISOString();
  await storeCredentials(credentials);

  return true;
}

/**
 * Get the credentials storage location (for display purposes)
 */
export function getCredentialsFilePath(): string {
  return CREDENTIALS_FILE;
}

/**
 * Get token from stored credentials (file only)
 *
 * Convenience function that retrieves credentials and returns just the token string.
 * Checks for token expiration before returning.
 *
 * NOTE: This does NOT check environment variables. Use resolveToken() for full resolution.
 * NOTE: This does NOT refresh expired tokens. Use getTokenWithRefresh() for auto-refresh.
 *
 * @param hostname - GitHub hostname (default: 'github.com')
 * @returns Token string or null if not found/expired
 */
export async function getToken(
  hostname: string = 'github.com'
): Promise<string | null> {
  const credentials = await getCredentials(hostname);

  if (!credentials || !credentials.token) {
    return null;
  }

  // Check if token is expired
  if (isTokenExpired(credentials)) {
    return null; // Let caller handle re-auth or use getTokenWithRefresh()
  }

  return credentials.token.token;
}

/**
 * Get token synchronously (file storage only)
 *
 * @param hostname - GitHub hostname (default: 'github.com')
 * @returns Token string or null if not found/expired
 */
export function getTokenSync(hostname: string = 'github.com'): string | null {
  const credentials = getCredentialsSync(hostname);

  if (!credentials || !credentials.token) {
    return null;
  }

  // Check if token is expired
  if (isTokenExpired(credentials)) {
    return null;
  }

  return credentials.token.token;
}

// Cache management
export { invalidateCredentialsCache, _getCacheStats, _resetCredentialsCache };

// Encryption and file I/O
export {
  encrypt,
  decrypt,
  ensureOctocodeDir,
  readCredentialsStore,
  writeCredentialsStore,
  OCTOCODE_DIR,
  CREDENTIALS_FILE,
  KEY_FILE,
};

// Token refresh — bound wrappers that inject storage dependencies,
// breaking the circular import between tokenRefresh.ts and storage.ts.
/** @see _refreshAuthTokenCore for implementation details */
export async function refreshAuthToken(
  hostname?: string,
  clientId?: string
): Promise<RefreshResult> {
  return _refreshAuthTokenCore(
    { getCredentials, updateToken },
    hostname,
    clientId
  );
}

/** @see _getTokenWithRefreshCore for implementation details */
export async function getTokenWithRefresh(
  hostname?: string,
  clientId?: string
): Promise<TokenWithRefreshResult> {
  return _getTokenWithRefreshCore(
    { getCredentials, updateToken },
    hostname,
    clientId
  );
}

export type { RefreshResult, TokenWithRefreshResult };

// Inject storage deps into tokenResolution (breaks the cycle).
// Only getTokenWithRefresh is needed — all resolution paths use it.
initTokenResolution({ getTokenWithRefresh });

// Token resolution
export {
  resolveToken,
  resolveTokenWithRefresh,
  resolveTokenFull,
  resetTokenResolution,
  type ResolvedToken,
  type ResolvedTokenWithRefresh,
  type FullTokenResolution,
  type GhCliTokenGetter,
};

// Utility functions
export { isTokenExpired, isRefreshTokenExpired };
