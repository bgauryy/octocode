// @octocodeai/config — single source of truth for ALL Octocode config + token env logic.
//
// Surfaces: MCP server · CLI · VS Code extension · Pi extension · agent · standalone skills
//
// Zero dependencies (Node builtins only). Cross-platform.
//
// Precedence:
//   explicit process.env  >  <project>/.octocode/.env  >  <home>/.env  >  <home>/.octocoderc  >  defaults

import fs from 'node:fs';
import path from 'node:path';
import { parseBooleanEnv } from './config/resolverSections.js';
import {
  HOME_TRUSTED_ENV_KEYS,
  PROTECTED_KEY_NAMES,
} from './config/contract.generated.js';

// ─── Re-export getOctocodeHome (defined in home.ts to break circular deps) ───
export { getOctocodeHome } from './home.js';
import { getOctocodeHome } from './home.js';

// ─── Re-exports from config/ and tokens/ ─────────────────────────────────────
export type {
  OctocodeConfig,
  ResolvedConfig,
  ValidationResult,
  LoadConfigResult,
  ExtensionConfigOptions,
  GitHubConfigOptions,
  LocalConfigOptions,
  ToolsConfigOptions,
  NetworkConfigOptions,
  LspConfigOptions,
  OutputConfigOptions,
  OutputFormat,
  OutputPaginationConfigOptions,
  StorageConfigOptions,
  StorageMode,
  RequiredExtensionConfig,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
  RequiredOutputPaginationConfig,
  RequiredSessionConfig,
  RequiredStorageConfig,
  MinifyMode,
} from './config/types.js';
export { CONFIG_SCHEMA_VERSION, CONFIG_FILE_NAME } from './config/types.js';
export {
  DEFAULT_CONFIG,
  DEFAULT_EXTENSION_CONFIG,
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
  DEFAULT_SESSION_CONFIG,
  DEFAULT_STORAGE_CONFIG,
} from './config/defaults.js';
export {
  type RuntimeSurface,
  setRuntimeSurface,
  getRuntimeSurface,
  _resetRuntimeSurface,
} from './config/runtimeSurface.js';
export { validateConfig } from './config/validator.js';
export {
  getConfigFilePath,
  configExists,
  loadConfigSync,
  loadConfig,
} from './config/loader.js';
export {
  CONFIG_SOURCE_ENV_KEYS,
  type ConfigSourceEnvKey,
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  resolveExtensionStorage,
  resolveGitHub,
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveLsp,
  resolveOutput,
  resolveSession,
  resolveStorage,
} from './config/resolverSections.js';
export {
  resolveConfigSync,
  getConfigSync,
  getConfigValue,
} from './config/resolver.js';
export type { TokenSource } from './tokens/types.js';
export {
  ENV_TOKEN_VARS,
  type EnvTokenVar,
  getTokenFromEnv,
  getEnvTokenSource,
  hasEnvToken,
  resolveEnvToken,
} from './tokens/envTokens.js';

// ─── Env loading (uses loadConfigSync from config/loader.ts below) ────────────

import { loadConfigSync } from './config/loader.js';
import { getConfigSync } from './config/resolver.js';

/** Keys a project/global .env must never override — infrastructure + all auth tokens. */
export const PROTECTED_KEYS: ReadonlySet<string> = new Set(PROTECTED_KEY_NAMES);

/**
 * Parse dotenv text into a { KEY: VALUE } map. Strict KEY=VALUE, `#` comments,
 * optional `export ` prefix, surrounding quotes stripped. No shell expansion.
 */
export function parseEnv(
  text: string | null | undefined
): Record<string, string> {
  const out: Record<string, string> = {};
  if (!text) return out;
  for (const rawLine of text.split('\n')) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const normalized = trimmed.startsWith('export ')
      ? trimmed.slice('export '.length).trim()
      : trimmed;
    const eq = normalized.indexOf('=');
    if (eq === -1) continue;
    const key = normalized.slice(0, eq).trim();
    if (!key) continue;
    out[key] = normalized
      .slice(eq + 1)
      .trim()
      .replace(/^["']|["']$/g, '');
  }
  return out;
}

function readTextIfExists(filePath: string): string {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch {
    return '';
  }
}

export interface LoadOctocodeEnvOptions {
  home?: string;
  cwd?: string;
  trusted?: boolean;
}

export interface LoadOctocodeEnvResult {
  map: Record<string, string>;
  sources: Record<string, 'global' | 'project'>;
}

/**
 * Load merged Octocode env from global then project (project wins).
 * Returns { map, sources } where sources[key] = 'global' | 'project' (names only, no values).
 * The project file is included only when trusted.
 */
export function loadOctocodeEnv({
  home,
  cwd,
  trusted = false,
}: LoadOctocodeEnvOptions = {}): LoadOctocodeEnvResult {
  const map: Record<string, string> = {};
  const sources: Record<string, 'global' | 'project'> = {};

  if (home) {
    for (const [k, v] of Object.entries(
      parseEnv(readTextIfExists(path.join(home, '.env')))
    )) {
      map[k] = v;
      sources[k] = 'global';
    }
  }
  if (cwd && trusted) {
    for (const [k, v] of Object.entries(
      parseEnv(readTextIfExists(path.join(cwd, '.octocode', '.env')))
    )) {
      map[k] = v;
      sources[k] = 'project';
    }
  }
  return { map, sources };
}

export interface ApplyOctocodeEnvOptions {
  env?: Record<string, string | undefined>;
  sources?: Record<string, 'global' | 'project'>;
}

export interface ApplyOctocodeEnvResult {
  applied: string[];
  skippedProtected: string[];
  skippedExisting: string[];
}

/**
 * Apply a parsed env map into `env`. Skips keys already set (env wins over files) and
 * protected keys. Returns names only — never values — for logging/status.
 */
export function applyOctocodeEnv(
  map: Record<string, string> | null | undefined,
  { env = process.env, sources = {} }: ApplyOctocodeEnvOptions = {}
): ApplyOctocodeEnvResult {
  const applied: string[] = [];
  const skippedProtected: string[] = [];
  const skippedExisting: string[] = [];

  for (const [key, value] of Object.entries(map ?? {})) {
    const trustedHomeKey =
      sources[key] === 'global' &&
      HOME_TRUSTED_ENV_KEYS.some(name => name === key);
    if (PROTECTED_KEYS.has(key) && !trustedHomeKey) {
      skippedProtected.push(key);
      continue;
    }
    const existing = env[key];
    if (existing !== undefined && existing !== '') {
      skippedExisting.push(key);
      continue;
    }
    env[key] = value;
    applied.push(key);
  }
  return { applied, skippedProtected, skippedExisting };
}

export interface PropagateOctocodeEnvOptions {
  home?: string;
  cwd?: string;
  trusted?: boolean;
  env?: Record<string, string | undefined>;
}

export interface PropagateOctocodeEnvResult extends ApplyOctocodeEnvResult {
  sources: Record<string, 'global' | 'project'>;
  keys: string[];
}

/** Convenience: load + apply in one call. Returns names-only metadata. */
export function propagateOctocodeEnv({
  home = getOctocodeHome(),
  cwd,
  trusted = false,
  env = process.env,
}: PropagateOctocodeEnvOptions = {}): PropagateOctocodeEnvResult {
  const { map, sources } = loadOctocodeEnv({ home, cwd, trusted });
  const result = applyOctocodeEnv(map, { env, sources });
  return { ...result, sources, keys: Object.keys(map) };
}

/**
 * True when stats.json writes are enabled via OCTOCODE_ENABLE_STATS=1|true.
 * Stats are always tracked in memory; this flag controls disk persistence only.
 * Keeping it off (the default) eliminates one write per 60-second flush cycle.
 */
export function isStatsEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  if (
    env['OCTOCODE_STORAGE_MODE']?.trim().toLowerCase() === 'memory' ||
    (env === process.env && !isPersistentStorageEnabled())
  )
    return false;
  return parseBooleanEnv(env['OCTOCODE_ENABLE_STATS']) ?? false;
}

/** True when Octocode may persist caches and runtime state on this machine. */
export function isPersistentStorageEnabled(): boolean {
  return getConfigSync().storage.mode === 'persistent';
}

/**
 * True when the Pi extension may persist Awareness state, SQLite extension
 * state, and session continuity on this machine.
 *
 * Reads `extension.storage.mode` from .octocoderc first (falling back to
 * the global `storage.mode`), then the `OCTOCODE_EXTENSION_STORAGE_MODE`
 * env var. This allows the researcher/CLI to run with `storage.mode=memory`
 * while the Pi extension uses `extension.storage.mode=persistent`.
 */
export function isPersistentStorageEnabledForExtension(): boolean {
  return getConfigSync().extension.storage.mode === 'persistent';
}

/**
 * Read and parse `<home>/.octocoderc`.
 * Delegates to the robust JSON5 loader (state-machine based, handles // inside strings).
 * Returns {} when absent or invalid. No secrets belong here.
 */
export function loadOctocoderc(
  home: string = getOctocodeHome()
): Record<string, unknown> {
  const result = loadConfigSync(home);
  if (result.success)
    return result.config ? (result.config as Record<string, unknown>) : {};
  // File absent is silent; any other failure (parse error etc.) is reported.
  if (result.error && result.error !== 'Config file does not exist') {
    process.stderr.write(
      `[octocode-config] Failed to parse .octocoderc: ${result.error}\n`
    );
  }
  return {};
}
