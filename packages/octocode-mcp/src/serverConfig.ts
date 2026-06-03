import type { ProviderType } from './providers/types.js';
import {
  resolveTokenFull,
  getConfigSync,
  invalidateConfigCache,
} from 'octocode-shared';
import { version } from '../package.json';
import type { ServerConfig, TokenSourceType } from './types/server.js';
import { CONFIG_ERRORS } from './errors/domainErrors.js';
import { maskSensitiveData } from 'octocode-security-utils/mask';

let config: ServerConfig | null = null;
let initializationPromise: Promise<void> | null = null;

// Injectable resolveTokenFull for testing
type ResolveTokenFullFn = typeof resolveTokenFull;
let _resolveTokenFull: ResolveTokenFullFn = resolveTokenFull;

/**
 * @internal - For testing only
 * Use `resolveTokenFull` to mock the entire resolution chain
 */
export function _setTokenResolvers(resolvers: {
  resolveTokenFull?: ResolveTokenFullFn;
}): void {
  if (resolvers.resolveTokenFull) {
    _resolveTokenFull = resolvers.resolveTokenFull;
  }
}

/**
 * @internal - For testing only
 */
export function _resetTokenResolvers(): void {
  _resolveTokenFull = resolveTokenFull;
}

const VALID_TOKEN_SOURCES = new Set<string>([
  'env:OCTOCODE_TOKEN',
  'env:GH_TOKEN',
  'env:GITHUB_TOKEN',
  'octocode-storage',
  'gh-cli',
  'none',
]);

async function resolveGitHubToken(): Promise<{
  token: string | null;
  source: TokenSourceType;
}> {
  // Delegate fully to octocode-shared's resolveTokenFull for centralized logic.
  // Priority: env vars (1-3) → octocode storage → gh CLI
  // The gh CLI fallback uses the default getGhCliToken from octocode-shared.
  try {
    const result = await _resolveTokenFull({ hostname: 'github.com' });
    if (result?.token) {
      const raw = result.source ?? 'none';
      return {
        token: result.token,
        source: VALID_TOKEN_SOURCES.has(raw)
          ? (raw as TokenSourceType)
          : 'none',
      };
    }
    return { token: null, source: 'none' };
  } catch {
    return { token: null, source: 'none' };
  }
}

export async function initialize(): Promise<void> {
  if (config !== null) {
    return;
  }
  if (initializationPromise !== null) {
    return initializationPromise;
  }

  const pendingInitialization = (async () => {
    // Load fully-resolved configuration from ~/.octocode/.octocoderc
    // Already handles: env vars > config file > hardcoded defaults
    const resolved = getConfigSync();

    // Resolve token once at startup for initial config (source tracking)
    // Token is NOT cached - subsequent calls to getGitHubToken() will re-resolve
    const tokenResult = await resolveGitHubToken();

    config = {
      version: version,
      githubApiUrl: resolved.github.apiUrl,
      toolsToRun: resolved.tools.enabled ?? undefined,
      enableTools: resolved.tools.enableAdditional ?? undefined,
      disableTools: resolved.tools.disabled ?? undefined,
      timeout: resolved.network.timeout,
      maxRetries: resolved.network.maxRetries,
      loggingEnabled: resolved.telemetry.logging,
      enableLocal: resolved.local.enabled,
      enableClone: resolved.local.enableClone,
      outputFormat: resolved.output.format,
      tokenSource: tokenResult.source,
    };
  })();

  initializationPromise = pendingInitialization;

  try {
    await pendingInitialization;
  } catch (error) {
    if (initializationPromise === pendingInitialization) {
      config = null;
      initializationPromise = null;
    }
    throw error;
  }
}

export function cleanup(): void {
  config = null;
  initializationPromise = null;
  invalidateConfigCache(); // Reset shared config cache to pick up new defaults/env vars
}

export function getServerConfig(): ServerConfig {
  if (!config) {
    // NOTE: Circular dependency prevents calling logSessionError here
    const sanitizedMessage = maskSensitiveData(
      CONFIG_ERRORS.NOT_INITIALIZED.message
    );
    throw new Error(sanitizedMessage);
  }
  return config;
}

/**
 * Get the current GitHub token.
 * Always resolves fresh - no caching. Let octocode-shared handle fallbacks.
 * Token can change at runtime (deletion, refresh, new login).
 */
export async function getGitHubToken(): Promise<string | null> {
  const result = await resolveGitHubToken();
  return result.token;
}

export function isLocalEnabled(): boolean {
  return getServerConfig().enableLocal;
}

export function isCloneEnabled(): boolean {
  const cfg = getServerConfig();
  // Clone requires both enableLocal AND enableClone
  return cfg.enableLocal && cfg.enableClone;
}

export function isLoggingEnabled(): boolean {
  return config?.loggingEnabled ?? false;
}

/**
 * Get the source of the current GitHub token.
 * Always resolves fresh - no caching. Token source can change at runtime.
 * Returns the type indicating where the token was found:
 * - 'env:OCTOCODE_TOKEN', 'env:GH_TOKEN', 'env:GITHUB_TOKEN' for env vars
 * - 'gh-cli' for GitHub CLI
 * - 'octocode-storage' for stored credentials
 * - 'none' if no token was found
 */
export async function getTokenSource(): Promise<TokenSourceType> {
  const result = await resolveGitHubToken();
  return result.source;
}

/**
 * Get the active provider. Always 'github'.
 */
export function getActiveProvider(): ProviderType {
  return 'github';
}

/**
 * Get active provider configuration for tool execution.
 * Returns provider type and base URL based on global config.
 */
export function getActiveProviderConfig(): {
  provider: ProviderType;
  baseUrl?: string;
  token?: string;
} {
  const githubApiUrl = getConfigSync().github.apiUrl;
  const baseUrl =
    githubApiUrl !== 'https://api.github.com' ? githubApiUrl : undefined;
  return {
    provider: 'github',
    baseUrl,
  };
}
