import type { OctocodeConfig, ResolvedConfig } from './types.js';
import { DEFAULT_CONFIG } from './defaults.js';
import { loadConfigSync } from './loader.js';
import { validateConfig } from './validator.js';
import {
  resolveGitHub,
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveLsp,
  resolveOutput,
  resolveSession,
} from './resolverSections.js';

const CONFIG_ENV_KEYS = [
  'GITHUB_API_URL',
  'ENABLE_LOCAL',
  'ENABLE_CLONE',
  'ALLOWED_PATHS',
  'WORKSPACE_ROOT',
  'TOOLS_TO_RUN',
  'ENABLE_TOOLS',
  'DISABLE_TOOLS',
  'REQUEST_TIMEOUT',
  'MAX_RETRIES',
  'OCTOCODE_LSP_CONFIG',
  'OCTOCODE_OUTPUT_FORMAT',
  'OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH',
  'OCTOCODE_ENABLE_STATS',
] as const;

type FileState = 'absent' | 'valid' | 'invalid';

function hasEnvOverrides(env: NodeJS.ProcessEnv = process.env): boolean {
  return CONFIG_ENV_KEYS.some(key => env[key] !== undefined);
}

function sourceFor(fileState: FileState): ResolvedConfig['source'] {
  if (fileState === 'invalid') return 'invalid';
  const envOverrides = hasEnvOverrides();
  if (fileState === 'valid') return envOverrides ? 'mixed' : 'file';
  return envOverrides ? 'env' : 'defaults';
}

function warnInvalidConfig(configPath: string, errors: readonly string[]): void {
  const message = errors.length > 0 ? errors.join('; ') : 'Invalid configuration';
  process.stderr.write(`[octocode-config] Invalid .octocoderc at ${configPath}: ${message}\n`);
}

function buildResolvedConfig(
  fileConfig: OctocodeConfig | undefined,
  options: { configPath?: string; fileState?: FileState } = {}
): ResolvedConfig {
  const fileState = options.fileState ?? (fileConfig === undefined ? 'absent' : 'valid');
  const source = sourceFor(fileState);

  return {
    version: fileConfig?.version ?? DEFAULT_CONFIG.version,
    github: resolveGitHub(fileConfig?.github),
    local: resolveLocal(fileConfig?.local),
    tools: resolveTools(fileConfig?.tools),
    network: resolveNetwork(fileConfig?.network),
    lsp: resolveLsp(fileConfig?.lsp),
    output: resolveOutput(fileConfig?.output),
    session: resolveSession(),
    source,
    configPath: fileState !== 'absent' ? options.configPath : undefined,
  };
}

export function resolveConfigSync(): ResolvedConfig {
  const loadResult = loadConfigSync();

  if (loadResult.success && loadResult.config) {
    const validation = validateConfig(loadResult.config);

    if (!validation.valid) {
      warnInvalidConfig(loadResult.path, validation.errors);
      return buildResolvedConfig(undefined, {
        configPath: loadResult.path,
        fileState: 'invalid',
      });
    }

    return buildResolvedConfig(loadResult.config, {
      configPath: loadResult.path,
      fileState: 'valid',
    });
  }

  if (loadResult.error && loadResult.error !== 'Config file does not exist') {
    warnInvalidConfig(loadResult.path, [loadResult.error]);
    return buildResolvedConfig(undefined, {
      configPath: loadResult.path,
      fileState: 'invalid',
    });
  }

  return buildResolvedConfig(undefined);
}

export async function resolveConfig(): Promise<ResolvedConfig> {
  return resolveConfigSync();
}

export function getConfigSync(): ResolvedConfig {
  return resolveConfigSync();
}

export async function getConfig(): Promise<ResolvedConfig> {
  return getConfigSync();
}

export async function reloadConfig(): Promise<ResolvedConfig> {
  invalidateConfigCache();
  return getConfig();
}

export function invalidateConfigCache(): void {
  // Kept as a compatibility no-op. Config resolution is intentionally uncached
  // because it depends on mutable process.env, runtime surface, and home files.
}

export function _resetConfigCache(): void {
  // Test compatibility no-op; see invalidateConfigCache().
}

export function _getCacheState(): { cached: boolean; timestamp: number } {
  return { cached: false, timestamp: 0 };
}
