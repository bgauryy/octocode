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

// ─── Re-export getOctocodeHome (defined in home.ts to break circular deps) ───
export { getOctocodeHome } from './home.js';
import { getOctocodeHome } from './home.js';

// ─── Re-exports from config/ and tokens/ ─────────────────────────────────────
export * from './config/index.js';
export * from './tokens/index.js';

// ─── Env loading (uses loadConfigSync from config/loader.ts below) ────────────

import { loadConfigSync } from './config/loader.js';

/** Keys a project/global .env must never override — infrastructure + all auth tokens. */
export const PROTECTED_KEYS: ReadonlySet<string> = new Set([
  'PATH', 'HOME', 'SHELL', 'USER', 'LOGNAME', 'PWD', 'TMPDIR', 'NODE_OPTIONS',
  'OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN', 'PYTHON',
]);

/**
 * Parse dotenv text into a { KEY: VALUE } map. Strict KEY=VALUE, `#` comments,
 * optional `export ` prefix, surrounding quotes stripped. No shell expansion.
 */
export function parseEnv(text: string | null | undefined): Record<string, string> {
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
    out[key] = normalized.slice(eq + 1).trim().replace(/^["']|["']$/g, '');
  }
  return out;
}

function readTextIfExists(filePath: string): string {
  try { return fs.readFileSync(filePath, 'utf8'); } catch { return ''; }
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
    for (const [k, v] of Object.entries(parseEnv(readTextIfExists(path.join(home, '.env'))))) {
      map[k] = v;
      sources[k] = 'global';
    }
  }
  if (cwd && trusted) {
    for (const [k, v] of Object.entries(
      parseEnv(readTextIfExists(path.join(cwd, '.octocode', '.env'))),
    )) {
      map[k] = v;
      sources[k] = 'project';
    }
  }
  return { map, sources };
}

export interface ApplyOctocodeEnvOptions {
  env?: Record<string, string | undefined>;
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
  { env = process.env }: ApplyOctocodeEnvOptions = {},
): ApplyOctocodeEnvResult {
  const applied: string[] = [];
  const skippedProtected: string[] = [];
  const skippedExisting: string[] = [];

  for (const [key, value] of Object.entries(map ?? {})) {
    if (PROTECTED_KEYS.has(key)) { skippedProtected.push(key); continue; }
    const existing = env[key];
    if (existing !== undefined && existing !== '') { skippedExisting.push(key); continue; }
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
  const result = applyOctocodeEnv(map, { env });
  return { ...result, sources, keys: Object.keys(map) };
}

/**
 * True when stats.json writes are enabled via OCTOCODE_ENABLE_STATS=1|true.
 * Stats are always tracked in memory; this flag controls disk persistence only.
 * Keeping it off (the default) eliminates one write per 60-second flush cycle.
 */
export function isStatsEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  const v = env['OCTOCODE_ENABLE_STATS'];
  return v === '1' || v === 'true';
}

/**
 * Read and parse `<home>/.octocoderc`.
 * Delegates to the robust JSON5 loader (state-machine based, handles // inside strings).
 * Returns {} when absent or invalid. No secrets belong here.
 */
export function loadOctocoderc(home: string = getOctocodeHome()): Record<string, unknown> {
  const result = loadConfigSync(home);
  if (result.success) return result.config ? (result.config as Record<string, unknown>) : {};
  // File absent is silent; any other failure (parse error etc.) is reported.
  if (result.error && result.error !== 'Config file does not exist') {
    process.stderr.write(`[octocode-config] Failed to parse .octocoderc: ${result.error}\n`);
  }
  return {};
}
