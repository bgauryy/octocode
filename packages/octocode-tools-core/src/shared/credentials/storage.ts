import type { StoredCredentials, StoreResult, DeleteResult } from './types.js';

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

export { getTokenFromEnv, getEnvTokenSource, hasEnvToken, ENV_TOKEN_VARS };

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

    invalidateCredentialsCache(hostname);

    return { success: true };
  } catch {
    throw new Error('Failed to store credentials');
  }
}

export interface GetCredentialsOptions {
  bypassCache?: boolean;
}

export async function getCredentials(
  hostname: string = 'github.com',
  options?: GetCredentialsOptions
): Promise<StoredCredentials | null> {
  const normalizedHostname = normalizeHostname(hostname);

  if (!options?.bypassCache) {
    const cached = getCachedCredentials(normalizedHostname);
    if (cached !== undefined) {
      return cached;
    }
  }

  const store = readCredentialsStore();
  const credentials = store.credentials[normalizedHostname] || null;

  setCachedCredentials(normalizedHostname, credentials);

  return credentials;
}

export function getCredentialsSync(
  hostname: string = 'github.com'
): StoredCredentials | null {
  const normalizedHostname = normalizeHostname(hostname);
  const store = readCredentialsStore();
  return store.credentials[normalizedHostname] || null;
}

export async function deleteCredentials(
  hostname: string = 'github.com'
): Promise<DeleteResult> {
  const normalizedHostname = normalizeHostname(hostname);
  let deletedFromFile = false;

  const store = readCredentialsStore();
  if (store.credentials[normalizedHostname]) {
    delete store.credentials[normalizedHostname];

    if (Object.keys(store.credentials).length === 0) {
      cleanupKeyFile();
    } else {
      writeCredentialsStore(store);
    }
    deletedFromFile = true;
  }

  invalidateCredentialsCache(normalizedHostname);

  return {
    success: deletedFromFile,
    deletedFromFile,
  };
}

export async function listStoredHosts(): Promise<string[]> {
  const store = readCredentialsStore();
  return Object.keys(store.credentials);
}

export function listStoredHostsSync(): string[] {
  const store = readCredentialsStore();
  return Object.keys(store.credentials);
}

export async function hasCredentials(
  hostname: string = 'github.com'
): Promise<boolean> {
  return (await getCredentials(hostname)) !== null;
}

export function hasCredentialsSync(hostname: string = 'github.com'): boolean {
  return getCredentialsSync(hostname) !== null;
}

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

export function getCredentialsFilePath(): string {
  return CREDENTIALS_FILE;
}

export async function getToken(
  hostname: string = 'github.com'
): Promise<string | null> {
  const credentials = await getCredentials(hostname);

  if (!credentials || !credentials.token) {
    return null;
  }

  if (isTokenExpired(credentials)) {
    return null;
  }

  return credentials.token.token;
}

export function getTokenSync(hostname: string = 'github.com'): string | null {
  const credentials = getCredentialsSync(hostname);

  if (!credentials || !credentials.token) {
    return null;
  }

  if (isTokenExpired(credentials)) {
    return null;
  }

  return credentials.token.token;
}

export { invalidateCredentialsCache, _getCacheStats, _resetCredentialsCache };

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

initTokenResolution({ getTokenWithRefresh });

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

export { isTokenExpired, isRefreshTokenExpired };
