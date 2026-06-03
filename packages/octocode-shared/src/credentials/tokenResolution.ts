/**
 * Token Resolution
 *
 * Single-implementation token resolution with priority chain
 * (env vars → storage → gh CLI). All three public functions delegate
 * to the same core (`resolveTokenFull`), eliminating the previous
 * three-implementation inconsistency.
 *
 * Uses dependency injection (initTokenResolution) to receive storage
 * functions, breaking the former circular dependency with storage.ts.
 */

import type { TokenSource } from './types.js';
import type { TokenWithRefreshResult } from './tokenRefresh.js';
import { resolveEnvToken } from './envTokens.js';
import { getGhCliToken as defaultGetGhCliToken } from './ghCli.js';
import {
  OCTOCODE_GITHUB_APP_CLIENT_ID,
  DEFAULT_HOSTNAME,
} from './constants.js';

/** Storage functions injected at init time by storage.ts. */
interface StorageDeps {
  getTokenWithRefresh: (
    hostname?: string,
    clientId?: string
  ) => Promise<TokenWithRefreshResult>;
}

let _storage: StorageDeps | null = null;

/**
 * Initialize token resolution with storage dependencies.
 * Called once by storage.ts to break the circular import.
 */
export function initTokenResolution(deps: StorageDeps): void {
  _storage = deps;
}

/**
 * Reset injected storage dependencies.
 * @internal For testing only — call between tests that use vi.resetModules().
 */
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

/**
 * Resolve a GitHub token using the full priority chain:
 *   1. Env vars (OCTOCODE_TOKEN > GH_TOKEN > GITHUB_TOKEN) — no auto-refresh
 *   2. Octocode encrypted storage — auto-refresh for GitHub App tokens
 *   3. gh CLI fallback — gh manages its own refresh
 *
 * Returns null when no usable token is found from any source.
 */
export async function resolveTokenFull(options?: {
  hostname?: string;
  clientId?: string;
  getGhCliToken?: GhCliTokenGetter;
}): Promise<FullTokenResolution | null> {
  const hostname = options?.hostname ?? DEFAULT_HOSTNAME;
  const clientId = options?.clientId ?? OCTOCODE_GITHUB_APP_CLIENT_ID;
  const getGhCliToken = options?.getGhCliToken ?? defaultGetGhCliToken;

  // 1. Env vars — highest priority, user-managed, never auto-refreshed
  const envResult = resolveEnvToken();
  if (envResult) {
    return {
      token: envResult.token,
      source: envResult.source,
      wasRefreshed: false,
    };
  }

  // 2. Octocode storage — auto-refresh for GitHub App tokens only
  const result = await getStorage().getTokenWithRefresh(hostname, clientId);
  if (result.token) {
    return {
      token: result.token,
      source: 'octocode-storage',
      wasRefreshed: result.source === 'refreshed',
      username: result.username,
    };
  }

  // 3. gh CLI fallback — gh manages its own token lifecycle
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
    // gh CLI not available or failed — fall through to null
  }

  return null;
}

// ─── Backward-compat wrappers ─────────────────────────────────────────────────
// These delegate to resolveTokenFull so the three functions share one
// implementation. Marked @deprecated — prefer resolveTokenFull directly.

export interface ResolvedToken {
  token: string;
  source: TokenSource;
}

/** @deprecated Use resolveTokenFull instead. */
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

/** @deprecated Use resolveTokenFull instead. */
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
