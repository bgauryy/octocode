import type { ProviderType } from './providers/types.js';
import {
  resolveTokenFull,
  getConfigSync,
  invalidateConfigCache,
} from 'octocode-shared';
import { version } from '../package.json';
import type { ServerConfig, TokenSourceType } from './types/server.js';
import { CONFIG_ERRORS } from './errors/domainErrors.js';
import { maskSensitiveData } from 'octocode-security/mask';

let config: ServerConfig | null = null;
let initializationPromise: Promise<void> | null = null;

type ResolveTokenFullFn = typeof resolveTokenFull;
let _resolveTokenFull: ResolveTokenFullFn = resolveTokenFull;

export function _setTokenResolvers(resolvers: {
  resolveTokenFull?: ResolveTokenFullFn;
}): void {
  if (resolvers.resolveTokenFull) {
    _resolveTokenFull = resolvers.resolveTokenFull;
  }
}

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
    const resolved = getConfigSync();

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
  invalidateConfigCache();
}

export function getServerConfig(): ServerConfig {
  if (!config) {
    const sanitizedMessage = maskSensitiveData(
      CONFIG_ERRORS.NOT_INITIALIZED.message
    );
    throw new Error(sanitizedMessage);
  }
  return config;
}

export async function getGitHubToken(): Promise<string | null> {
  const result = await resolveGitHubToken();
  return result.token;
}

export function isLocalEnabled(): boolean {
  return getServerConfig().enableLocal;
}

export function isCloneEnabled(): boolean {
  const cfg = getServerConfig();
  return cfg.enableLocal && cfg.enableClone;
}

export function isLoggingEnabled(): boolean {
  return config?.loggingEnabled ?? false;
}

export async function getTokenSource(): Promise<TokenSourceType> {
  const result = await resolveGitHubToken();
  return result.source;
}

export function getActiveProvider(): ProviderType {
  return 'github';
}

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
