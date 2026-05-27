/**
 * Global Configuration Module
 *
 * Provides global control over Octocode execution via ~/.octocode/.octocoderc file.
 *
 * @example
 * ```typescript
 * import { getConfig, getConfigSync, reloadConfig } from 'octocode-shared';
 *
 * // Async (recommended)
 * const config = await getConfig();
 * console.log(config.github.apiUrl);  // 'https://api.github.com'
 * console.log(config.local.enabled);  // true (default; set ENABLE_LOCAL=false to disable)
 *
 * // Sync (for hot paths)
 * const config = getConfigSync();
 *
 * // Force reload from disk
 * const freshConfig = await reloadConfig();
 * ```
 *
 * Resolution Priority:
 * 1. Environment variables (highest - always wins)
 * 2. ~/.octocode/.octocoderc file
 * 3. Hardcoded defaults (lowest)
 */

export type {
  // Main config types
  OctocodeConfig,
  ResolvedConfig,
  ValidationResult,
  LoadConfigResult,

  // Section types (raw)
  GitHubConfigOptions,
  LocalConfigOptions,
  ToolsConfigOptions,
  NetworkConfigOptions,
  TelemetryConfigOptions,
  LspConfigOptions,
  OutputConfigOptions,
  OutputPaginationConfigOptions,

  // Section types (resolved)
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredTelemetryConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
  RequiredOutputPaginationConfig,
} from './types.js';

export { CONFIG_SCHEMA_VERSION, CONFIG_FILE_NAME } from './types.js';

export {
  // Default configs
  DEFAULT_CONFIG,
  DEFAULT_GITHUB_CONFIG,
  DEFAULT_LOCAL_CONFIG,
  DEFAULT_TOOLS_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  DEFAULT_TELEMETRY_CONFIG,
  DEFAULT_LSP_CONFIG,
  DEFAULT_OUTPUT_CONFIG,

  // Limits
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
  MIN_OUTPUT_DEFAULT_CHAR_LENGTH,
  MAX_OUTPUT_DEFAULT_CHAR_LENGTH,
} from './defaults.js';

export { CONFIG_FILE_PATH } from './loader.js';
// Note: OCTOCODE_DIR is already exported from credentials module

// Loader functions
export {
  loadConfig,
  loadConfigSync,
  configExists,
  getConfigPath,
  getOctocodeDir,
} from './loader.js';

// Validator functions
export { validateConfig } from './validator.js';

// Resolver functions (main API)
export {
  getConfig,
  getConfigSync,
  reloadConfig,
  resolveConfig,
  resolveConfigSync,
  invalidateConfigCache,
  getConfigValue,

  // Testing utilities
  _resetConfigCache,
  _getCacheState,
} from './resolver.js';

// Env var parsing utilities
export { parseLoggingEnv } from './resolverSections.js';

// Schemas
export { OctocodeConfigSchema } from './schemas.js';
