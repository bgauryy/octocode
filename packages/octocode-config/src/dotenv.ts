// `.env` loading and propagation with the shared dotenv policy (the native
// resolver applies the same rules): parse, merge home then workspace, and
// apply into an environment without overriding what it already sets. A
// workspace value wins per key, except for API keys (`API_KEYS`), where the
// home value stands and a workspace only fills a key the home file lacks.

import fs from 'node:fs';
import path from 'node:path';
import {
  API_KEYS,
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
  HOME_TRUSTED_ENV_KEYS,
  LEGACY_ENV_NAMES,
  PROTECTED_KEY_NAMES,
  WORKSPACE_NARROW_ONLY_ENV,
} from './config/contract.generated.js';
import { getOctocodeHome } from './home.js';

/** API keys: the home `.env` value beats a workspace one (process > home > workspace). */
const API_KEY_NAMES: ReadonlySet<string> = new Set(API_KEYS.map(key => key.name));

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

export function sameFile(left: string, right: string): boolean {
  if (path.resolve(left) === path.resolve(right)) return true;
  try {
    return fs.realpathSync(left) === fs.realpathSync(right);
  } catch {
    return false;
  }
}

function readTextIfExists(filePath: string): string {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch {
    return '';
  }
}

/**
 * One `.env` file's nonblank entries, each legacy name moved to its current
 * name unless the file sets that one (as the native resolver reads them).
 */
export function fileEnv(filePath: string): Record<string, string> {
  const entries = Object.entries(parseEnv(readTextIfExists(filePath))).filter(([, v]) => v.trim());
  const map = Object.fromEntries(entries);
  for (const [legacy, current] of Object.entries(LEGACY_ENV_NAMES)) {
    const value = map[legacy];
    if (value === undefined) continue;
    delete map[legacy];
    if (!map[current]) map[current] = value;
  }
  return map;
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
 * Load merged Octocode env from global then project: project wins, except an
 * API key the global file sets, which stays global.
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
    for (const [k, v] of Object.entries(fileEnv(path.join(home, '.env')))) {
      map[k] = v;
      sources[k] = 'global';
    }
  }
  const workspaceEnv = cwd ? path.join(cwd, '.octocode', '.env') : undefined;
  // When cwd is the OS home, <cwd>/.octocode is the Octocode home: its .env is
  // the home layer only, never read a second time as the workspace layer.
  if (workspaceEnv && trusted && !(home && sameFile(workspaceEnv, path.join(home, '.env')))) {
    for (const [k, v] of Object.entries(fileEnv(workspaceEnv))) {
      // A workspace value for a protected key is dropped later; it must not
      // also evict the trusted home value for that key.
      if (k in map && isProtectedKey(k) && !workspaceMayNarrow(k, v)) continue;
      // A repository only fills an API key the user has not set globally.
      if (k in map && API_KEY_NAMES.has(k)) continue;
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

/**
 * The classification credential's variable (from the config contract):
 * present-but-blank in the process env, it disables every classification
 * feature and no file may refill it.
 */
export const CLASSIFICATION_KILL_SWITCH = CONFIG_FIELDS.find(
  field => field.path === 'classification.api'
)?.env?.name;

/**
 * Home-trusted switches a workspace `.env` may set only to their narrowing
 * value (generated from the config contract, shared with the native resolver).
 */
function workspaceMayNarrow(key: string, value: string): boolean {
  return WORKSPACE_NARROW_ONLY_ENV[key] === value.trim().toLowerCase();
}

/** Each variable's names: the current one, then its legacy ones (from the config contract). */
export function namesOf(key: string): string[] {
  const current = LEGACY_ENV_NAMES[key] ?? key;
  return [
    current,
    ...Object.keys(LEGACY_ENV_NAMES).filter(legacy => LEGACY_ENV_NAMES[legacy] === current),
  ];
}

/**
 * Whether `env` sets `key` under any of its names. The native runtime reads a
 * legacy name as its current one, so a legacy spelling in the environment
 * keeps a file from setting the current one.
 */
function setsAnyName(env: Record<string, string | undefined>, key: string): boolean {
  return namesOf(key).some(name => Boolean(env[name]?.trim()));
}

/** The GitHub token variables under every name, in the order they are read. */
const TOKEN_NAMES = ENV_TOKEN_VARS.flatMap(namesOf);

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

  // Resolve source precedence before token-name priority. Otherwise a
  // workspace `GH_TOKEN` could outrank a process or home `GITHUB_TOKEN`.
  const shadowed = new Set<string>();
  // What the environment set before any file, under any name of each variable.
  const before = { ...env };
  const processSelected = TOKEN_NAMES.some(key => Boolean(before[key]?.trim()));
  // Tokens are API keys: a home token shadows every workspace token.
  const homeSelected = TOKEN_NAMES.some(
    key => sources[key] === 'global' && Boolean(map?.[key]?.trim())
  );
  for (const key of TOKEN_NAMES) {
    if (processSelected || (homeSelected && sources[key] === 'project'))
      shadowed.add(key);
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
      setsAnyName(before, key) ||
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
