import { createRequire } from 'node:module';

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
  normalizeHostname,
  isTokenExpired,
  isRefreshTokenExpired,
} from './credentialUtils.js';

const require = createRequire(import.meta.url);

type NativeCredentialApi = {
  storeCredentials(value: StoredCredentials): { success?: boolean } | void;
  getCredentials(hostname?: string | null): StoredCredentials | null;
  deleteCredentials(
    hostname?: string | null
  ): { success?: boolean; deletedFromFile?: boolean } | void;
};

let nativeOverride: NativeCredentialApi | null | undefined;
let nativeCache: NativeCredentialApi | null | undefined;

export function _setNativeCredentialsForTesting(
  api: NativeCredentialApi | null | undefined
): void {
  nativeOverride = api;
  nativeCache = undefined;
}

function nativeCredentials(): NativeCredentialApi | null {
  if (nativeOverride !== undefined) {
    return nativeOverride;
  }
  if (nativeCache !== undefined) {
    return nativeCache;
  }
  nativeCache = loadNativeCredentials();
  return nativeCache;
}

function loadNativeCredentials(): NativeCredentialApi | null {
  const bindingPath = process.env.OCTOCODE_NATIVE_BINDING;
  if (!bindingPath) {
    return null;
  }
  try {
    const binding = require(bindingPath) as {
      nativeAbiVersion?: () => number;
      storeCredentials?: NativeCredentialApi['storeCredentials'];
      getCredentials?: NativeCredentialApi['getCredentials'];
      deleteCredentials?: NativeCredentialApi['deleteCredentials'];
    };
    const abi =
      typeof binding.nativeAbiVersion === 'function'
        ? binding.nativeAbiVersion()
        : 0;
    if (
      abi < 2 ||
      typeof binding.storeCredentials !== 'function' ||
      typeof binding.getCredentials !== 'function' ||
      typeof binding.deleteCredentials !== 'function'
    ) {
      return null;
    }
    return {
      storeCredentials: binding.storeCredentials,
      getCredentials: binding.getCredentials,
      deleteCredentials: binding.deleteCredentials,
    };
  } catch {
    return null;
  }
}

function readFileStoreCredentials(
  hostname: string
): StoredCredentials | null {
  const store = readCredentialsStore();
  return store.credentials[hostname] || null;
}

function tryNativeGet(hostname: string): StoredCredentials | undefined {
  const native = nativeCredentials();
  if (!native) {
    return undefined;
  }
  try {
    return asStoredCredentials(native.getCredentials(hostname)) ?? undefined;
  } catch {
    return undefined;
  }
}

function deleteFileStoreCredentials(hostname: string): boolean {
  const store = readCredentialsStore();
  if (!store.credentials[hostname]) {
    return false;
  }
  delete store.credentials[hostname];
  if (Object.keys(store.credentials).length === 0) {
    cleanupKeyFile();
  } else {
    writeCredentialsStore(store);
  }
  return true;
}

function asStoredCredentials(value: unknown): StoredCredentials | null {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const credentials = value as StoredCredentials;
  if (
    !credentials.token ||
    typeof credentials.token.token !== 'string' ||
    credentials.token.token.length === 0
  ) {
    return null;
  }
  return credentials;
}

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
    const native = nativeCredentials();
    if (native) {
      native.storeCredentials(normalizedCredentials);
      invalidateCredentialsCache(hostname);
      return { success: true };
    }

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

  const fromNative = tryNativeGet(normalizedHostname);
  const credentials =
    fromNative !== undefined
      ? fromNative
      : readFileStoreCredentials(normalizedHostname);

  setCachedCredentials(normalizedHostname, credentials);

  return credentials;
}

export function getCredentialsSync(
  hostname: string = 'github.com'
): StoredCredentials | null {
  const normalizedHostname = normalizeHostname(hostname);
  const fromNative = tryNativeGet(normalizedHostname);
  if (fromNative !== undefined) {
    return fromNative;
  }
  return readFileStoreCredentials(normalizedHostname);
}

export async function deleteCredentials(
  hostname: string = 'github.com'
): Promise<DeleteResult> {
  const normalizedHostname = normalizeHostname(hostname);
  const native = nativeCredentials();
  if (native) {
    try {
      const result = native.deleteCredentials(normalizedHostname);
      const deletedFromFile = deleteFileStoreCredentials(normalizedHostname);
      invalidateCredentialsCache(normalizedHostname);
      return {
        success: result?.success !== false,
        deletedFromFile,
      };
    } catch {
      invalidateCredentialsCache(normalizedHostname);
      return { success: false, deletedFromFile: false };
    }
  }

  const deletedFromFile = deleteFileStoreCredentials(normalizedHostname);
  invalidateCredentialsCache(normalizedHostname);
  return {
    success: deletedFromFile,
    deletedFromFile,
  };
}

export async function listStoredHosts(): Promise<string[]> {
  // File-store only: the native keychain has no list API.
  const store = readCredentialsStore();
  return Object.keys(store.credentials);
}

export function listStoredHostsSync(): string[] {
  // File-store only: the native keychain has no list API.
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
