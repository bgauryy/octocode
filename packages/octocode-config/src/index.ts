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
import { resolveConfigFields } from './config/resolverSections.js';
import type { OctocodeConfig } from './config/types.js';
import {
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
  HOME_TRUSTED_ENV_KEYS,
  PROTECTED_KEY_NAMES,
  DEFAULT_STORAGE_MODE,
  WORKSPACE_NARROW_ONLY_ENV,
} from './config/contract.generated.js';

// Read-only editor metadata comes from the same contract as both resolvers.
export { CONFIG_FIELDS };
export type {
  ConfigFieldSpec,
  ConfigEnvBinding,
  ConfigFieldKind,
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
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredToolsConfig,
  RequiredNetworkConfig,
  RequiredLspConfig,
  RequiredOutputConfig,
  RequiredOutputPaginationConfig,
  RequiredStorageConfig,
  MinifyMode,
} from './config/types.js';
export { CONFIG_SCHEMA_VERSION, CONFIG_FILE_NAME } from './config/types.js';
export {
  DEFAULT_CONFIG,
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
} from './config/loader.js';
export {
  CONFIG_SOURCE_ENV_KEYS,
  type ConfigSourceEnvKey,
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  resolveConfigFields,
  resolveGitHub,
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveLsp,
  resolveOutput,
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
 * Home-trusted switches a workspace `.env` may set only to their narrowing
 * value (generated from the config contract, shared with the native resolver).
 */
export { WORKSPACE_NARROW_ONLY_ENV };

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
 * True when stats.json writes are enabled (`storage.stats`, env
 * OCTOCODE_ENABLE_STATS). Stats are always tracked in memory; this flag
 * controls disk persistence only, and needs persistent storage.
 */
export function isStatsEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  if (!isPersistentStorageEnabled(env)) return false;
  const cwd = process.cwd();
  return resolveConfigFields(
    loadOctocodercLayers({ env, cwd }) as OctocodeConfig[],
    effectiveEnv(env, cwd)
  ).storage.stats;
}

/** `env` with the workspace and home `.env` layers applied (native rules). */
function effectiveEnv(
  env: NodeJS.ProcessEnv,
  cwd: string
): Record<string, string | undefined> {
  const effective: Record<string, string | undefined> = { ...env };
  const { map, sources } = loadOctocodeEnv({ home: getOctocodeHome(env), cwd });
  applyOctocodeEnv(map, { env: effective, sources });
  return effective;
}

/** `persistent` or `memory`, else undefined. */
function storageModeOf(value: unknown): 'persistent' | 'memory' | undefined {
  const mode = typeof value === 'string' ? value.trim().toLowerCase() : '';
  return mode === 'persistent' || mode === 'memory' ? mode : undefined;
}

/**
 * Whether runtime state may be written to disk, with the native resolver's
 * layers and trust rules: process env > workspace `.env` > home `.env` >
 * workspace `.octocoderc` > home `.octocoderc` > default. A workspace layer
 * may only narrow to `memory`.
 */
export function isPersistentStorageEnabled(
  env: NodeJS.ProcessEnv = process.env,
  cwd: string = process.cwd()
): boolean {
  const home = getOctocodeHome(env);
  const fromEnv = storageModeOf(
    effectiveEnv(env, cwd)['OCTOCODE_STORAGE_MODE']
  );
  if (fromEnv) return fromEnv === 'persistent';
  const globalPath = getConfigFilePath(home);
  const projectPath = getProjectConfigFilePath(cwd);
  if (
    !sameFile(projectPath, globalPath) &&
    storageModeOf(
      (readOctocodercFile(projectPath) as { storage?: { mode?: unknown } })
        .storage?.mode
    ) === 'memory'
  )
    return false;
  const fromHome = storageModeOf(
    (readOctocodercFile(globalPath) as { storage?: { mode?: unknown } }).storage
      ?.mode
  );
  return (fromHome ?? DEFAULT_STORAGE_MODE) === 'persistent';
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
    layers.unshift(
      stripWorkspaceProtected(readOctocodercFile(projectPath), projectPath)
    );
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
    if (
      !field.file ||
      !field.env.some(binding => PROTECTED_KEYS.has(binding.name))
    )
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
