// `.env` loading and propagation with the shared dotenv policy (the native
// resolver applies the same rules): parse, merge home then workspace, and
// apply into an environment without overriding what it already sets.

import fs from 'node:fs';
import path from 'node:path';
import {
  ENV_TOKEN_VARS,
  HOME_TRUSTED_ENV_KEYS,
  PROTECTED_KEY_NAMES,
  WORKSPACE_NARROW_ONLY_ENV,
} from './config/contract.generated.js';
import { getOctocodeHome } from './home.js';

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
const CLASSIFICATION_KILL_SWITCH = 'OCTOCODE_CLASSIFICATION_API';

/**
 * Home-trusted switches a workspace `.env` may set only to their narrowing
 * value (generated from the config contract, shared with the native resolver).
 */
function workspaceMayNarrow(key: string, value: string): boolean {
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

  // Resolve source precedence before token-name priority. Otherwise a home
  // `GH_TOKEN` could outrank a process or workspace `GITHUB_TOKEN`.
  const shadowed = new Set<string>();
  const processSelected = ENV_TOKEN_VARS.some(key => Boolean(env[key]?.trim()));
  const workspaceSelected = ENV_TOKEN_VARS.some(
    key => sources[key] === 'project' && Boolean(map?.[key]?.trim())
  );
  for (const key of ENV_TOKEN_VARS) {
    if (processSelected || (workspaceSelected && sources[key] !== 'project'))
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
