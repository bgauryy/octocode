/**
 * Configuration cache management and core resolution
 *
 * Contains both the resolution logic (loading + merging) and the cache layer.
 * This avoids the circular dependency that would arise if the cache and resolver
 * were in separate modules (cache needs resolver, resolver re-exports cache).
 */

import type { OctocodeConfig, ResolvedConfig } from './types.js';
import { DEFAULT_CONFIG } from './defaults.js';
import { loadConfigSync, configExists } from './loader.js';
import { validateConfig } from './validator.js';
import { createLogger } from '../logger/index.js';
import {
  resolveGitHub,
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveTelemetry,
  resolveLsp,
  resolveOutput,
} from './resolverSections.js';

const logger = createLogger('octocode-config');

/**
 * Build resolved configuration from file config and environment.
 *
 * @param fileConfig - Configuration loaded from file (optional)
 * @param configPath - Path to config file (if loaded)
 * @returns Fully resolved configuration
 */
function buildResolvedConfig(
  fileConfig: OctocodeConfig | undefined,
  configPath?: string
): ResolvedConfig {
  const hasFile = fileConfig !== undefined;
  const hasEnvOverrides =
    process.env.GITHUB_API_URL !== undefined ||
    process.env.ENABLE_LOCAL !== undefined ||
    process.env.ENABLE_CLONE !== undefined ||
    process.env.ALLOWED_PATHS !== undefined ||
    process.env.TOOLS_TO_RUN !== undefined ||
    process.env.ENABLE_TOOLS !== undefined ||
    process.env.DISABLE_TOOLS !== undefined ||
    process.env.REQUEST_TIMEOUT !== undefined ||
    process.env.MAX_RETRIES !== undefined ||
    process.env.LOG !== undefined ||
    process.env.OCTOCODE_LSP_CONFIG !== undefined ||
    process.env.OCTOCODE_OUTPUT_FORMAT !== undefined ||
    process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH !== undefined;

  // Determine source
  let source: ResolvedConfig['source'];
  if (hasFile && hasEnvOverrides) {
    source = 'mixed';
  } else if (hasFile) {
    source = 'file';
  } else {
    source = 'defaults';
  }

  return {
    version: fileConfig?.version ?? DEFAULT_CONFIG.version,
    github: resolveGitHub(fileConfig?.github),
    local: resolveLocal(fileConfig?.local),
    tools: resolveTools(fileConfig?.tools),
    network: resolveNetwork(fileConfig?.network),
    telemetry: resolveTelemetry(fileConfig?.telemetry),
    lsp: resolveLsp(fileConfig?.lsp),
    output: resolveOutput(fileConfig?.output),
    source,
    configPath: hasFile ? configPath : undefined,
  };
}

/**
 * Resolve configuration synchronously.
 * Loads from file, applies env overrides, returns with defaults.
 *
 * @returns Fully resolved configuration
 */
export function resolveConfigSync(): ResolvedConfig {
  // Try to load config file
  const loadResult = loadConfigSync();

  if (loadResult.success && loadResult.config) {
    // Validate loaded config
    const validation = validateConfig(loadResult.config);

    if (validation.warnings.length > 0) {
      // Log warnings but continue
      for (const warning of validation.warnings) {
        logger.warn(`Warning: ${warning}`);
      }
    }

    if (!validation.valid) {
      // Log errors and fall back to defaults — invalid config is not loaded
      for (const error of validation.errors) {
        logger.warn(`Validation error: ${error}`);
      }
      logger.warn(
        'Config file has validation errors — falling back to defaults with env overrides'
      );
      return buildResolvedConfig(undefined);
    }

    // Config is valid — build resolved config from file + defaults + env
    return buildResolvedConfig(loadResult.config, loadResult.path);
  }

  // No file or file error - use defaults with env overrides
  if (loadResult.error && configExists()) {
    // File exists but failed to parse - log warning
    logger.warn(loadResult.error);
  }

  return buildResolvedConfig(undefined);
}

/**
 * Resolve configuration asynchronously.
 * Currently just wraps sync version, but allows for future async operations.
 *
 * @returns Promise resolving to fully resolved configuration
 */
export async function resolveConfig(): Promise<ResolvedConfig> {
  return resolveConfigSync();
}

/** Cached resolved configuration */
let cachedConfig: ResolvedConfig | null = null;

/** Timestamp when config was cached */
let cacheTimestamp: number = 0;

/** Cache TTL in milliseconds (1 minute) */
const CACHE_TTL_MS = 60000;

/**
 * Get fully resolved configuration (sync).
 * Uses cached config if available and not expired.
 *
 * @example
 * ```typescript
 * const config = getConfigSync();
 * if (config.local.enabled) {
 *   // Local tools are enabled
 * }
 * ```
 */
export function getConfigSync(): ResolvedConfig {
  const now = Date.now();

  if (cachedConfig && now - cacheTimestamp < CACHE_TTL_MS) {
    return cachedConfig;
  }

  // Resolve fresh config
  cachedConfig = resolveConfigSync();
  cacheTimestamp = now;

  return cachedConfig;
}

/**
 * Get fully resolved configuration (async).
 * Loads from file, applies env overrides, returns with defaults.
 *
 * Results are cached for performance - call reloadConfig() to refresh.
 *
 * @example
 * ```typescript
 * const config = await getConfig();
 * console.log(config.github.apiUrl); // 'https://api.github.com'
 * console.log(config.local.enabled); // true (default; set ENABLE_LOCAL=false to disable)
 * ```
 */
export async function getConfig(): Promise<ResolvedConfig> {
  return getConfigSync();
}

/**
 * Reload configuration from disk, bypassing cache.
 * Useful when config file has been modified.
 *
 * @returns Fresh resolved configuration
 */
export async function reloadConfig(): Promise<ResolvedConfig> {
  invalidateConfigCache();
  return getConfig();
}

/**
 * Invalidate the configuration cache.
 * Next call to getConfig/getConfigSync will reload from disk.
 */
export function invalidateConfigCache(): void {
  cachedConfig = null;
  cacheTimestamp = 0;
}

/**
 * @internal - For testing only
 * Reset the configuration cache
 */
export function _resetConfigCache(): void {
  cachedConfig = null;
  cacheTimestamp = 0;
}

/**
 * @internal - For testing only
 * Get cache state for assertions
 */
export function _getCacheState(): { cached: boolean; timestamp: number } {
  return {
    cached: cachedConfig !== null,
    timestamp: cacheTimestamp,
  };
}
