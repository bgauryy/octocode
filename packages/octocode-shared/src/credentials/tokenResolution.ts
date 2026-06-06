import type { TokenSource } from './types.js';
import type { TokenWithRefreshResult } from './tokenRefresh.js';
import { resolveEnvToken } from './envTokens.js';
import { getGhCliToken as defaultGetGhCliToken } from './ghCli.js';
import {
  OCTOCODE_GITHUB_APP_CLIENT_ID,
  DEFAULT_HOSTNAME,
} from './constants.js';

interface StorageDeps {
  getTokenWithRefresh: (
    hostname?: string,
    clientId?: string
  ) => Promise<TokenWithRefreshResult>;
}

let _storage: StorageDeps | null = null;

export function initTokenResolution(deps: StorageDeps): void {
  _storage = deps;
}

export function resetTokenResolution(): void {
  _storage = null;
}

function getStorage(): StorageDeps {
  if (!_storage) {
    throw new Error(
      'Token resolution not initialized. Call initTokenResolution() first.'
    );
  }
  return _storage;
}

export interface FullTokenResolution {
  token: string;
  source: TokenSource | 'gh-cli';
  wasRefreshed?: boolean;
  username?: string;
}

export type GhCliTokenGetter = (
  hostname?: string
) => string | null | Promise<string | null>;

export async function resolveTokenFull(options?: {
  hostname?: string;
  clientId?: string;
  getGhCliToken?: GhCliTokenGetter;
}): Promise<FullTokenResolution | null> {
  const hostname = options?.hostname ?? DEFAULT_HOSTNAME;
  const clientId = options?.clientId ?? OCTOCODE_GITHUB_APP_CLIENT_ID;
  const getGhCliToken = options?.getGhCliToken ?? defaultGetGhCliToken;

  const envResult = resolveEnvToken();
  if (envResult) {
    return {
      token: envResult.token,
      source: envResult.source,
      wasRefreshed: false,
    };
  }

  const result = await getStorage().getTokenWithRefresh(hostname, clientId);
  if (result.token) {
    return {
      token: result.token,
      source: 'octocode-storage',
      wasRefreshed: result.source === 'refreshed',
      username: result.username,
    };
  }

  try {
    const ghToken = await Promise.resolve(getGhCliToken(hostname));
    if (ghToken?.trim()) {
      return {
        token: ghToken.trim(),
        source: 'gh-cli',
        wasRefreshed: false,
      };
    }
  } catch {
    void 0;
  }

  return null;
}

export interface ResolvedToken {
  token: string;
  source: TokenSource;
}

export async function resolveToken(
  hostname: string = DEFAULT_HOSTNAME
): Promise<ResolvedToken | null> {
  const result = await resolveTokenFull({
    hostname,
    getGhCliToken: () => null,
  });
  if (!result) return null;
  return { token: result.token, source: result.source as TokenSource };
}

export interface ResolvedTokenWithRefresh extends ResolvedToken {
  wasRefreshed?: boolean;
  username?: string;
}

export async function resolveTokenWithRefresh(
  hostname: string = DEFAULT_HOSTNAME,
  clientId: string = OCTOCODE_GITHUB_APP_CLIENT_ID
): Promise<ResolvedTokenWithRefresh | null> {
  const result = await resolveTokenFull({
    hostname,
    clientId,
    getGhCliToken: () => null,
  });
  if (!result) return null;
  return {
    token: result.token,
    source: result.source as TokenSource,
    wasRefreshed: result.wasRefreshed,
    username: result.username,
  };
}
