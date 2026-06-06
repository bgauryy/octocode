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
    process.env.WORKSPACE_ROOT !== undefined ||
    process.env.TOOLS_TO_RUN !== undefined ||
    process.env.ENABLE_TOOLS !== undefined ||
    process.env.DISABLE_TOOLS !== undefined ||
    process.env.REQUEST_TIMEOUT !== undefined ||
    process.env.MAX_RETRIES !== undefined ||
    process.env.LOG !== undefined ||
    process.env.OCTOCODE_LSP_CONFIG !== undefined ||
    process.env.OCTOCODE_OUTPUT_FORMAT !== undefined ||
    process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH !== undefined;

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

export function resolveConfigSync(): ResolvedConfig {
  const loadResult = loadConfigSync();

  if (loadResult.success && loadResult.config) {
    const validation = validateConfig(loadResult.config);

    if (validation.warnings.length > 0) {
      for (const warning of validation.warnings) {
        logger.warn(`Warning: ${warning}`);
      }
    }

    if (!validation.valid) {
      for (const error of validation.errors) {
        logger.warn(`Validation error: ${error}`);
      }
      logger.warn(
        'Config file has validation errors — falling back to defaults with env overrides'
      );
      return buildResolvedConfig(undefined);
    }

    return buildResolvedConfig(loadResult.config, loadResult.path);
  }

  if (loadResult.error && configExists()) {
    logger.warn(loadResult.error);
  }

  return buildResolvedConfig(undefined);
}

export async function resolveConfig(): Promise<ResolvedConfig> {
  return resolveConfigSync();
}

let cachedConfig: ResolvedConfig | null = null;

let cacheTimestamp: number = 0;

const CACHE_TTL_MS = 60000;

export function getConfigSync(): ResolvedConfig {
  const now = Date.now();

  if (cachedConfig && now - cacheTimestamp < CACHE_TTL_MS) {
    return cachedConfig;
  }

  cachedConfig = resolveConfigSync();
  cacheTimestamp = now;

  return cachedConfig;
}

export async function getConfig(): Promise<ResolvedConfig> {
  return getConfigSync();
}

export async function reloadConfig(): Promise<ResolvedConfig> {
  invalidateConfigCache();
  return getConfig();
}

export function invalidateConfigCache(): void {
  cachedConfig = null;
  cacheTimestamp = 0;
}

export function _resetConfigCache(): void {
  cachedConfig = null;
  cacheTimestamp = 0;
}

export function _getCacheState(): { cached: boolean; timestamp: number } {
  return {
    cached: cachedConfig !== null,
    timestamp: cacheTimestamp,
  };
}
