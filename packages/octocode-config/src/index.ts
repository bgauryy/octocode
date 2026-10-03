// @octocodeai/config — single source of truth for ALL Octocode config + token env logic.
//
// Surfaces: MCP server · CLI · VS Code extension · Pi extension · agent · standalone skills
//
// The `.` entry is zero-dependency (Node builtins only); the `./schema` and
// `./mcp` subpaths re-export `@octocodeai/octocode-core` (esbuild-external).
// Cross-platform.
//
// Precedence (per field):
//   explicit process.env  >  <project>/.octocode/.env  >  <home>/.env
//     >  <project>/.octocode/.octocoderc  >  <home>/.octocoderc  >  defaults

import fs from 'node:fs';
import path from 'node:path';
import { parseBooleanEnv } from './config/resolverSections.js';
import {
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
  HOME_TRUSTED_ENV_KEYS,
  PROTECTED_KEY_NAMES,
  DEFAULT_STORAGE_MODE,
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
  RUNTIME_SURFACES,
  INTERACTIVE_EXECUTION_TIMEOUT_SECS,
  setRuntimeSurface,
  getRuntimeSurface,
  _resetRuntimeSurface,
} from './config/runtimeSurface.js';
export {
  CONTRACT_DRIFT_OVERRIDE_ENV,
  type DevOverrideOptions,
  devOverridesAllowed,
  contractDriftAllowed,
  contractDriftMessage,
} from './config/devOverrides.js';
export { validateConfig } from './config/validator.js';
export {
  getConfigFilePath,
  getProjectConfigFilePath,
  configExists,
  loadConfigFileSync,
  loadConfigSync,
  loadProjectConfigSync,
  loadConfig,
} from './config/loader.js';
export {
  CONFIG_SOURCE_ENV_KEYS,
  type ConfigSourceEnvKey,
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  resolveConfigFields,
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
export type { TokenSource } from './tokens/types.js';
export {
  ENV_TOKEN_VARS,
  type EnvTokenVar,
  getTokenFromEnv,
  getEnvTokenSource,
  hasEnvToken,
  resolveEnvToken,
} from './tokens/envTokens.js';

// ─── Env loading (uses the loaders from config/loader.ts below) ───────────────

import {
  getConfigFilePath,
  getProjectConfigFilePath,
  loadConfigFileSync,
} from './config/loader.js';

/**
 * Env var names bound to a config field (e.g. `classification.api`), highest
 * priority first. Empty for an unknown path or an env-less field.
 */
export function configFieldEnvNames(fieldPath: string): readonly string[] {
  const field = CONFIG_FIELDS.find(candidate => candidate.path === fieldPath);
  return (field?.env ?? [])
    .slice()
    .sort((a, b) => a.priority - b.priority)
    .map(binding => binding.name);
}

/** Keys restricted by the shared dotenv policy (infrastructure and security controls). */
export const PROTECTED_KEYS: ReadonlySet<string> = new Set(PROTECTED_KEY_NAMES);

/** Upper-cased protected keys for case-insensitive matching on Windows. */
const PROTECTED_KEYS_CI: ReadonlySet<string> = new Set(
  PROTECTED_KEY_NAMES.map(name => name.toUpperCase())
);

/**
 * Windows environment variables are case-insensitive, so a `.env` line like
 * `Path=…` would dodge an exact-case protected check and then fold into
 * `PATH`. Match case-insensitively on win32; POSIX keeps exact-case semantics.
 */
export function isProtectedKey(key: string): boolean {
  if (PROTECTED_KEYS.has(key)) return true;
  return (
    process.platform === 'win32' && PROTECTED_KEYS_CI.has(key.toUpperCase())
  );
}

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
 * The workspace file is loaded by default; hosts can explicitly opt out with trusted:false.
 */
export function loadOctocodeEnv({
  home,
  cwd,
  trusted = true,
}: LoadOctocodeEnvOptions = {}): LoadOctocodeEnvResult {
  const map: Record<string, string> = {};
  const sources: Record<string, 'global' | 'project'> = {};

  if (home) {
    for (const [k, v] of Object.entries(
      parseEnv(readTextIfExists(path.join(home, '.env')))
    )) {
      if (!v.trim()) continue;
      map[k] = v;
      sources[k] = 'global';
    }
  }
  if (cwd && trusted) {
    for (const [k, v] of Object.entries(
      parseEnv(readTextIfExists(path.join(cwd, '.octocode', '.env')))
    )) {
      if (!v.trim()) continue;
      // A workspace value for a protected key is dropped later; it must not
      // also evict the trusted home value for that key.
      if (k in map && isProtectedKey(k) && !workspaceMayNarrow(k, v)) continue;
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

/** Present-but-blank in the process env disables every classification feature. */
export const CLASSIFICATION_KILL_SWITCH = 'OCTOCODE_CLASSIFICATION_API';

/**
 * Persistence switches a workspace `.env` may only turn off. They are
 * home-trusted (a checked-out repository must not widen where octocode
 * writes), but `memory` narrows what the trusted layers allow, so a project
 * that opts out of disk persistence keeps working. Mirrors the native resolver.
 */
export const WORKSPACE_NARROW_ONLY_ENV: Readonly<Record<string, string>> = {
  OCTOCODE_STORAGE_MODE: 'memory',
  OCTOCODE_EXTENSION_STORAGE_MODE: 'memory',
};

export function workspaceMayNarrow(key: string, value: string): boolean {
  return WORKSPACE_NARROW_ONLY_ENV[key] === value.trim().toLowerCase();
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

  // Resolve source precedence before alias preference. Otherwise a home
  // canonical key can hide a workspace alias for the same credential.
  const shadowed = new Set<string>();
  const groups: readonly (readonly string[])[] = [
    ENV_TOKEN_VARS,
    ...CONFIG_FIELDS.filter(
      field => field.credential && field.env.length > 1
    ).map(field => field.env.map(binding => binding.name)),
  ];
  for (const group of groups) {
    const processSelected = group.some(
      key =>
        Boolean(env[key]?.trim()) ||
        (key === CLASSIFICATION_KILL_SWITCH && env[key] !== undefined)
    );
    const workspaceSelected = group.some(
      key => sources[key] === 'project' && Boolean(map?.[key]?.trim())
    );
    for (const key of group) {
      if (processSelected || (workspaceSelected && sources[key] !== 'project'))
        shadowed.add(key);
    }
  }

  for (const [key, value] of Object.entries(map ?? {})) {
    const trustedHomeKey =
      sources[key] === 'global' &&
      (HOME_TRUSTED_ENV_KEYS as readonly string[]).includes(key);
    const narrowsPersistence =
      sources[key] === 'project' && workspaceMayNarrow(key, value);
    if (isProtectedKey(key) && !trustedHomeKey && !narrowsPersistence) {
      skippedProtected.push(key);
      continue;
    }
    const existing = env[key];
    // A present-but-blank classification key is an explicit opt-out.
    if (
      Boolean(existing?.trim()) ||
      shadowed.has(key) ||
      (key === CLASSIFICATION_KILL_SWITCH && existing !== undefined)
    ) {
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
  trusted = true,
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
  // Stats persistence requires persistent storage. Route through the same
  // storage-mode resolver for ANY env (it reads the given env then .octocoderc),
  // so `storage.mode=memory` in .octocoderc disables disk stats even when called
  // with a custom env object — previously that gate only applied to process.env.
  if (!isPersistentStorageEnabled(env)) return false;
  return parseBooleanEnv(env['OCTOCODE_ENABLE_STATS']) ?? false;
}

/**
 * Read storage.mode directly from env then raw .octocoderc layers — no
 * validator, no resolver pipeline. Only the two valid enum values are accepted.
 * Precedence: OCTOCODE_STORAGE_MODE env > workspace .octocoderc
 * > global .octocoderc > default.
 */
export function isPersistentStorageEnabled(
  env: NodeJS.ProcessEnv = process.env,
  cwd: string = process.cwd()
): boolean {
  const v = env['OCTOCODE_STORAGE_MODE']?.trim().toLowerCase();
  if (v === 'persistent' || v === 'memory') return v === 'persistent';
  for (const rc of loadOctocodercLayers({ env, cwd }) as {
    storage?: { mode?: unknown };
  }[]) {
    const m = rc.storage?.mode;
    if (m === 'persistent' || m === 'memory') return m === 'persistent';
  }
  return DEFAULT_STORAGE_MODE === 'persistent';
}

/**
 * True when the Pi extension may persist SQLite extension state and session
 * continuity on this machine.
 *
 * Precedence: OCTOCODE_EXTENSION_STORAGE_MODE env > OCTOCODE_STORAGE_MODE env
 * > extension.storage.mode in .octocoderc (workspace, then global)
 * > storage.mode in .octocoderc (workspace, then global) > default.
 * This allows the CLI to run with storage.mode=memory while the Pi extension
 * uses extension.storage.mode=persistent.
 */
export function isPersistentStorageEnabledForExtension(
  cwd: string = process.cwd()
): boolean {
  const env = process.env;
  const extVar = env['OCTOCODE_EXTENSION_STORAGE_MODE']?.trim().toLowerCase();
  if (extVar === 'persistent' || extVar === 'memory')
    return extVar === 'persistent';
  const storageVar = env['OCTOCODE_STORAGE_MODE']?.trim().toLowerCase();
  if (storageVar === 'persistent' || storageVar === 'memory')
    return storageVar === 'persistent';
  // Read the layers once for both extension and storage fallback.
  const layers = loadOctocodercLayers({ env, cwd }) as {
    storage?: { mode?: unknown };
    extension?: { storage?: { mode?: unknown } };
  }[];
  for (const rc of layers) {
    const extMode = rc.extension?.storage?.mode;
    if (extMode === 'persistent' || extMode === 'memory')
      return extMode === 'persistent';
  }
  for (const rc of layers) {
    const storageMode = rc.storage?.mode;
    if (storageMode === 'persistent' || storageMode === 'memory')
      return storageMode === 'persistent';
  }
  return DEFAULT_STORAGE_MODE === 'persistent';
}

/** Parse one `.octocoderc` file; {} when absent. Never throws: any other
 * failure is reported on stderr with the file path, and the file is ignored. */
function readOctocodercFile(filePath: string): Record<string, unknown> {
  const result = loadConfigFileSync(filePath);
  if (result.success)
    return result.config ? (result.config as Record<string, unknown>) : {};
  if (result.error && result.error !== 'Config file does not exist') {
    process.stderr.write(
      `[octocode-config] warning: ${filePath}: ${result.error}; the whole file is ignored\n`
    );
  }
  return {};
}

/**
 * Read and parse the global `<home>/.octocoderc`.
 * Delegates to the robust JSON5 loader (state-machine based, handles // inside strings).
 * Returns {} when absent or invalid. No secrets belong here.
 */
export function loadOctocoderc(
  home: string = getOctocodeHome()
): Record<string, unknown> {
  return readOctocodercFile(getConfigFilePath(home));
}

export interface LoadOctocodercLayersOptions {
  home?: string;
  cwd?: string;
  env?: Record<string, string | undefined>;
}

/**
 * `.octocoderc` layers in priority order: workspace
 * `<cwd>/.octocode/.octocoderc`, then global `<home>/.octocoderc`. Pass the
 * result to `resolveConfigFields` for per-field resolution. When the workspace
 * directory is the Octocode home, the file is returned once.
 */
export function loadOctocodercLayers({
  env = process.env,
  home = getOctocodeHome(env),
  cwd = process.cwd(),
}: LoadOctocodercLayersOptions = {}): Record<string, unknown>[] {
  const globalPath = getConfigFilePath(home);
  const projectPath = getProjectConfigFilePath(cwd);
  const layers = [readOctocodercFile(globalPath)];
  if (!sameFile(projectPath, globalPath))
    layers.unshift(stripWorkspaceProtected(readOctocodercFile(projectPath), projectPath));
  return layers;
}

/**
 * A workspace file may not set a field whose environment binding is
 * protected from a workspace `.env` (same trust boundary as native).
 */
function stripWorkspaceProtected(
  layer: Record<string, unknown>,
  filePath: string
): Record<string, unknown> {
  for (const field of CONFIG_FIELDS) {
    if (!field.file || !field.env.some(binding => PROTECTED_KEYS.has(binding.name)))
      continue;
    const parts = field.path.split('.');
    let parent: unknown = layer;
    for (const part of parts.slice(0, -1))
      parent =
        typeof parent === 'object' && parent !== null
          ? (parent as Record<string, unknown>)[part]
          : undefined;
    const key = parts.at(-1)!;
    if (
      typeof parent === 'object' &&
      parent !== null &&
      !Array.isArray(parent) &&
      key in parent
    ) {
      delete (parent as Record<string, unknown>)[key];
      process.stderr.write(
        `[octocode-config] warning: ${filePath}: ${field.path} is protected and ignored in a workspace config file; set it in the global config file or the process environment\n`
      );
    }
  }
  return layer;
}

function sameFile(a: string, b: string): boolean {
  if (path.resolve(a) === path.resolve(b)) return true;
  try {
    return fs.realpathSync(a) === fs.realpathSync(b);
  } catch {
    return false;
  }
}
