import type {
  OctocodeConfig,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
} from './types.js';
import {
  DEFAULT_GITHUB_CONFIG,
  DEFAULT_LOCAL_CONFIG,
  DEFAULT_TOOLS_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  DEFAULT_LSP_CONFIG,
  DEFAULT_OUTPUT_CONFIG,
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
} from './defaults.js';
import { getRuntimeSurface } from './runtimeSurface.js';

export function parseBooleanEnv(
  value: string | undefined
): boolean | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim().toLowerCase();
  if (trimmed === '') return undefined;
  if (trimmed === 'true' || trimmed === '1') return true;
  if (trimmed === 'false' || trimmed === '0') return false;
  return undefined;
}

export function parseIntEnv(value: string | undefined): number | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim();
  if (trimmed === '') return undefined;
  const parsed = parseInt(trimmed, 10);
  if (isNaN(parsed)) return undefined;
  return parsed;
}

export function parseStringArrayEnv(
  value: string | undefined
): string[] | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim();
  if (trimmed === '') return undefined;
  return trimmed
    .split(',')
    .map(s => s.trim())
    .filter(s => s.length > 0);
}

export function resolveGitHub(
  fileConfig?: OctocodeConfig['github']
): RequiredGitHubConfig {
  const envApiUrl = process.env.GITHUB_API_URL?.trim();

  return {
    apiUrl: envApiUrl || fileConfig?.apiUrl || DEFAULT_GITHUB_CONFIG.apiUrl,
  };
}

export function resolveLocal(
  fileConfig?: OctocodeConfig['local']
): RequiredLocalConfig {
  const isCli = getRuntimeSurface() === 'cli';
  const envEnableLocal = parseBooleanEnv(process.env.ENABLE_LOCAL);
  const envEnableClone = parseBooleanEnv(process.env.ENABLE_CLONE);
  const envAllowedPaths = parseStringArrayEnv(process.env.ALLOWED_PATHS);
  const envWorkspaceRoot = process.env.WORKSPACE_ROOT?.trim() || undefined;

  return {
    // Local tools are on by default. ENABLE_LOCAL=false (or local.enabled=false)
    // is an explicit opt-out for users who want to remove the local surface.
    enabled:
      envEnableLocal ?? fileConfig?.enabled ?? DEFAULT_LOCAL_CONFIG.enabled,
    // Clone: an explicit ENABLE_CLONE (env) or .octocoderc value wins for both
    // surfaces, so `false` disables everywhere. Otherwise the default is
    // surface-specific: ENABLED for the CLI, DISABLED for the MCP server.
    enableClone:
      envEnableClone ??
      fileConfig?.enableClone ??
      (isCli ? true : DEFAULT_LOCAL_CONFIG.enableClone),
    allowedPaths:
      envAllowedPaths ??
      fileConfig?.allowedPaths ??
      DEFAULT_LOCAL_CONFIG.allowedPaths,
    workspaceRoot:
      envWorkspaceRoot ??
      fileConfig?.workspaceRoot ??
      DEFAULT_LOCAL_CONFIG.workspaceRoot,
  };
}

export function resolveTools(
  fileConfig?: OctocodeConfig['tools']
): RequiredToolsConfig {
  const envToolsToRun = parseStringArrayEnv(process.env.TOOLS_TO_RUN);
  const envEnableTools = parseStringArrayEnv(process.env.ENABLE_TOOLS);
  const envDisableTools = parseStringArrayEnv(process.env.DISABLE_TOOLS);

  return {
    enabled:
      envToolsToRun ?? fileConfig?.enabled ?? DEFAULT_TOOLS_CONFIG.enabled,
    enableAdditional:
      envEnableTools ??
      fileConfig?.enableAdditional ??
      DEFAULT_TOOLS_CONFIG.enableAdditional,
    disabled:
      envDisableTools ?? fileConfig?.disabled ?? DEFAULT_TOOLS_CONFIG.disabled,
  };
}

export function resolveNetwork(
  fileConfig?: OctocodeConfig['network']
): RequiredNetworkConfig {
  const envTimeout = parseIntEnv(process.env.REQUEST_TIMEOUT);
  const envMaxRetries = parseIntEnv(process.env.MAX_RETRIES);

  let timeout =
    envTimeout ?? fileConfig?.timeout ?? DEFAULT_NETWORK_CONFIG.timeout;
  timeout = Math.max(MIN_TIMEOUT, Math.min(MAX_TIMEOUT, timeout));

  let maxRetries =
    envMaxRetries ??
    fileConfig?.maxRetries ??
    DEFAULT_NETWORK_CONFIG.maxRetries;
  maxRetries = Math.max(MIN_RETRIES, Math.min(MAX_RETRIES, maxRetries));

  return { timeout, maxRetries };
}

export function resolveLsp(
  fileConfig?: OctocodeConfig['lsp']
): RequiredLspConfig {
  const envConfigPath = process.env.OCTOCODE_LSP_CONFIG?.trim() || undefined;

  return {
    configPath:
      envConfigPath ?? fileConfig?.configPath ?? DEFAULT_LSP_CONFIG.configPath,
  };
}

const VALID_OUTPUT_FORMATS = new Set(['yaml', 'json']);

export function resolveOutput(
  fileConfig?: OctocodeConfig['output']
): RequiredOutputConfig {
  const envFormat = process.env.OCTOCODE_OUTPUT_FORMAT?.trim().toLowerCase();
  const envDefaultCharLength = parseIntEnv(
    process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH
  );
  const resolved =
    envFormat || fileConfig?.format || DEFAULT_OUTPUT_CONFIG.format;
  const configuredDefaultCharLength =
    envDefaultCharLength ??
    fileConfig?.pagination?.defaultCharLength ??
    DEFAULT_OUTPUT_CONFIG.pagination.defaultCharLength;
  const clampedDefaultCharLength = Math.max(
    MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
    Math.min(MAX_OUTPUT_DEFAULT_CHAR_LENGTH, configuredDefaultCharLength)
  );

  return {
    format: VALID_OUTPUT_FORMATS.has(resolved)
      ? (resolved as 'yaml' | 'json')
      : DEFAULT_OUTPUT_CONFIG.format,
    pagination: {
      defaultCharLength: clampedDefaultCharLength,
    },
  };
}
