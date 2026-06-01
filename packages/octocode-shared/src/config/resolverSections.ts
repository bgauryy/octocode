/**
 * Configuration section resolvers
 */

import type {
  OctocodeConfig,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredTelemetryConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
} from './types.js';
import {
  DEFAULT_GITHUB_CONFIG,
  DEFAULT_LOCAL_CONFIG,
  DEFAULT_TOOLS_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  DEFAULT_TELEMETRY_CONFIG,
  DEFAULT_LSP_CONFIG,
  DEFAULT_OUTPUT_CONFIG,
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
} from './defaults.js';

/**
 * Parse a boolean environment variable.
 *
 * @param value - Environment variable value
 * @returns Parsed boolean or undefined if not set
 */
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

/**
 * Parse an integer environment variable.
 *
 * @param value - Environment variable value
 * @returns Parsed integer or undefined if not set/invalid
 */
export function parseIntEnv(value: string | undefined): number | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim();
  if (trimmed === '') return undefined;
  const parsed = parseInt(trimmed, 10);
  if (isNaN(parsed)) return undefined;
  return parsed;
}

/**
 * Parse a string array environment variable (comma-separated).
 *
 * @param value - Environment variable value
 * @returns Parsed string array or undefined if not set
 */
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

/**
 * Parse LOG env var with "default to true" semantics.
 * Returns true unless explicitly set to 'false' or '0'.
 * Returns undefined if not set (to allow config fallback).
 *
 * @param value - The LOG environment variable value
 * @returns true, false, or undefined for fallback
 */
export function parseLoggingEnv(
  value: string | undefined
): boolean | undefined {
  if (value === undefined || value === null) return undefined;
  const trimmed = value.trim().toLowerCase();
  if (trimmed === '') return undefined;
  // Only return false if explicitly set to 'false' or '0'
  if (trimmed === 'false' || trimmed === '0') return false;
  // Any other value (including 'true', '1', 'yes', 'anything') means enabled
  return true;
}

/**
 * Resolve GitHub configuration.
 */
export function resolveGitHub(
  fileConfig?: OctocodeConfig['github']
): RequiredGitHubConfig {
  // Env var: GITHUB_API_URL
  const envApiUrl = process.env.GITHUB_API_URL?.trim();

  return {
    apiUrl: envApiUrl || fileConfig?.apiUrl || DEFAULT_GITHUB_CONFIG.apiUrl,
  };
}

/**
 * Resolve local tools configuration.
 */
export function resolveLocal(
  fileConfig?: OctocodeConfig['local']
): RequiredLocalConfig {
  // Env vars: ENABLE_LOCAL, ENABLE_CLONE, ALLOWED_PATHS
  const envEnableLocal = parseBooleanEnv(process.env.ENABLE_LOCAL);
  const envEnableClone = parseBooleanEnv(process.env.ENABLE_CLONE);
  const envAllowedPaths = parseStringArrayEnv(process.env.ALLOWED_PATHS);

  return {
    enabled:
      envEnableLocal ?? fileConfig?.enabled ?? DEFAULT_LOCAL_CONFIG.enabled,
    enableClone:
      envEnableClone ??
      fileConfig?.enableClone ??
      DEFAULT_LOCAL_CONFIG.enableClone,
    allowedPaths:
      envAllowedPaths ??
      fileConfig?.allowedPaths ??
      DEFAULT_LOCAL_CONFIG.allowedPaths,
  };
}

/**
 * Resolve tools configuration.
 */
export function resolveTools(
  fileConfig?: OctocodeConfig['tools']
): RequiredToolsConfig {
  // Env vars: TOOLS_TO_RUN, ENABLE_TOOLS, DISABLE_TOOLS
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

/**
 * Resolve network configuration.
 */
export function resolveNetwork(
  fileConfig?: OctocodeConfig['network']
): RequiredNetworkConfig {
  // Env vars: REQUEST_TIMEOUT, MAX_RETRIES
  const envTimeout = parseIntEnv(process.env.REQUEST_TIMEOUT);
  const envMaxRetries = parseIntEnv(process.env.MAX_RETRIES);

  // Clamp values to valid ranges
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

/**
 * Resolve telemetry configuration.
 */
export function resolveTelemetry(
  fileConfig?: OctocodeConfig['telemetry']
): RequiredTelemetryConfig {
  // Env var: LOG - uses "default to true" semantics
  const envLogging = parseLoggingEnv(process.env.LOG);

  return {
    logging:
      envLogging ?? fileConfig?.logging ?? DEFAULT_TELEMETRY_CONFIG.logging,
  };
}

/**
 * Resolve LSP configuration.
 */
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

/**
 * Resolve output format configuration.
 */
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
