import type { ProviderType } from './providers/types.js';
import { getGithubCLIToken } from './utils/exec/npm.js';
import {
  resolveTokenFull,
  type FullTokenResolution,
  type GhCliTokenGetter,
  getConfigSync,
  invalidateConfigCache,
} from 'octocode-shared';
import { version } from '../package.json';
import type { ServerConfig, TokenSourceType } from './types.js';
import { CONFIG_ERRORS } from './errors/domainErrors.js';
import { maskSensitiveData } from 'octocode-security-utils/mask';
import {
  getGitLabConfig as resolveGitLabConfig,
  getGitLabToken,
  getGitLabHost,
  isGitLabConfigured,
} from './gitlabConfig.js';
import {
  getBitbucketConfig as resolveBitbucketConfig,
  getBitbucketToken,
  getBitbucketHost,
  isBitbucketConfigured,
} from './bitbucketConfig.js';

/** Result of token resolution with source tracking */
interface TokenResolutionResult {
  token: string | null;
  source: TokenSourceType;
}

let config: ServerConfig | null = null;
let initializationPromise: Promise<void> | null = null;

// Injectable function for testing (gh CLI is passed to resolveTokenFull)
let _getGithubCLIToken = getGithubCLIToken;

// Injectable resolveTokenFull for testing
type ResolveTokenFullFn = (options?: {
  hostname?: string;
  clientId?: string;
  getGhCliToken?: GhCliTokenGetter;
}) => Promise<FullTokenResolution | null>;
let _resolveTokenFull: ResolveTokenFullFn = resolveTokenFull;

/**
 * Maps source strings from octocode-shared to internal TokenSourceType.
 *
 * @param source - Source string from resolver ('env:*', 'gh-cli', 'file')
 */
function mapSharedSourceToInternal(
  source: string | null | undefined
): TokenSourceType {
  if (!source) return 'none';

  // Already prefixed env source
  if (source.startsWith('env:')) return source as TokenSourceType;

  // CLI source
  if (source === 'gh-cli') return 'gh-cli';

  // Storage sources
  if (source === 'file' || source === 'octocode-storage') {
    return 'octocode-storage';
  }

  return 'none';
}

/**
 * @internal - For testing only
 * Use `resolveTokenFull` to mock the entire resolution chain
 */
export function _setTokenResolvers(resolvers: {
  getGithubCLIToken?: typeof getGithubCLIToken;
  resolveTokenFull?: ResolveTokenFullFn;
}): void {
  if (resolvers.getGithubCLIToken) {
    _getGithubCLIToken = resolvers.getGithubCLIToken;
  }
  if (resolvers.resolveTokenFull) {
    _resolveTokenFull = resolvers.resolveTokenFull;
  }
}

/**
 * @internal - For testing only
 */
export function _resetTokenResolvers(): void {
  _getGithubCLIToken = getGithubCLIToken;
  _resolveTokenFull = resolveTokenFull;
}

async function resolveGitHubToken(): Promise<TokenResolutionResult> {
  // Delegate to octocode-shared's resolveTokenFull for centralized logic
  // Priority: env vars (1-3) → octocode storage (4-5) → gh CLI (6)
  try {
    const result = await _resolveTokenFull({
      hostname: 'github.com',
      getGhCliToken: _getGithubCLIToken,
    });

    if (result?.token) {
      return {
        token: result.token,
        source: mapSharedSourceToInternal(result.source),
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
      disablePrompts: resolved.tools.disablePrompts,
      outputFormat: resolved.output.format,
      tokenSource: tokenResult.source,
      gitlab: resolveGitLabConfig(),
      bitbucket: resolveBitbucketConfig(),
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

export async function getToken(): Promise<string | null> {
  return getGitHubToken();
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

export function arePromptsEnabled(): boolean {
  return !(config?.disablePrompts ?? false);
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
 * Get the active provider based on environment configuration.
 * Priority: GITLAB_TOKEN → 'gitlab', BITBUCKET_TOKEN → 'bitbucket', otherwise → 'github' (default)
 */
export function getActiveProvider(): ProviderType {
  if (isGitLabConfigured()) return 'gitlab';
  if (isBitbucketConfigured()) return 'bitbucket';
  return 'github';
}

/**
 * Get active provider configuration for tool execution.
 * Returns provider type and base URL based on environment and global config.
 * Priority: env vars > config file > defaults
 */
export function getActiveProviderConfig(): {
  provider: ProviderType;
  baseUrl?: string;
  token?: string;
} {
  if (isGitLabConfigured()) {
    return {
      provider: 'gitlab',
      baseUrl: getGitLabHost(),
      token: getGitLabToken() ?? undefined,
    };
  }
  if (isBitbucketConfigured()) {
    return {
      provider: 'bitbucket',
      baseUrl: getBitbucketHost(),
      token: getBitbucketToken() ?? undefined,
    };
  }
  const githubApiUrl = getConfigSync().github.apiUrl;
  const baseUrl =
    githubApiUrl !== 'https://api.github.com' ? githubApiUrl : undefined;
  return {
    provider: 'github',
    baseUrl,
  };
}

/**
 * Check if the active provider is GitLab.
 */
export function isGitLabActive(): boolean {
  return getActiveProvider() === 'gitlab';
}
