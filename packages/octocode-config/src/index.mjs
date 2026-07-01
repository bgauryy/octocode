// @octocodeai/config — Octocode unified env + config loader.
// THE single source shared by every Octocode surface:
//   MCP server · CLI · VS Code extension · Pi extension · agent · standalone skills
//
// Zero dependencies (Node builtins only). Cross-platform. Importable or runnable via npx.
//
// Unified home (matches octocode-tools-core/src/shared/paths.ts):
//   macOS:   ~/.octocode
//   Linux:   ${XDG_CONFIG_HOME:-~/.config}/.octocode
//   Windows: %APPDATA%\.octocode
//   override: OCTOCODE_HOME
//
// Precedence (matches docs/CONFIGURATION.md):
//   explicit process.env  >  <project>/.octocode/.env  >  <home>/.env  >  <home>/.octocoderc  >  defaults

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/** Resolve the single Octocode home directory, cross-platform, with OCTOCODE_HOME override. */
export function getOctocodeHome(env = process.env) {
  const override = env.OCTOCODE_HOME;
  if (override && override.trim()) return path.resolve(override.trim());
  const home = os.homedir();
  const platform = os.platform();
  if (platform === 'win32') {
    const appData = env.APPDATA || path.join(home, 'AppData', 'Roaming');
    return path.join(appData, '.octocode');
  }
  if (platform === 'darwin') return path.join(home, '.octocode');
  const xdg = env.XDG_CONFIG_HOME || path.join(home, '.config');
  return path.join(xdg, '.octocode');
}

// Keys a project/global .env must never override — infrastructure + auth.
export const PROTECTED_KEYS = new Set([
  'PATH', 'HOME', 'SHELL', 'USER', 'LOGNAME', 'PWD', 'TMPDIR', 'NODE_OPTIONS',
  'OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN', 'PYTHON',
]);

/**
 * Parse dotenv text into a { KEY: VALUE } map. Strict KEY=VALUE, `#` comments,
 * optional `export ` prefix, surrounding quotes stripped. No shell expansion.
 */
export function parseEnv(text) {
  const out = {};
  if (!text) return out;
  for (const rawLine of text.split('\n')) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const normalized = trimmed.startsWith('export ') ? trimmed.slice('export '.length).trim() : trimmed;
    const eq = normalized.indexOf('=');
    if (eq === -1) continue;
    const key = normalized.slice(0, eq).trim();
    if (!key) continue;
    out[key] = normalized.slice(eq + 1).trim().replace(/^["']|["']$/g, '');
  }
  return out;
}

function readTextIfExists(filePath) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch {
    return '';
  }
}

/**
 * Load merged Octocode env from global then project (project wins).
 * Returns { map, sources } where sources[key] = 'global' | 'project' (names only, no values).
 * The project file is included only when trusted.
 */
export function loadOctocodeEnv({ home, cwd, trusted = false } = {}) {
  const map = {};
  const sources = {};
  if (home) {
    for (const [k, v] of Object.entries(parseEnv(readTextIfExists(path.join(home, '.env'))))) {
      map[k] = v; sources[k] = 'global';
    }
  }
  if (cwd && trusted) {
    for (const [k, v] of Object.entries(parseEnv(readTextIfExists(path.join(cwd, '.octocode', '.env'))))) {
      map[k] = v; sources[k] = 'project';
    }
  }
  return { map, sources };
}

/**
 * Apply a parsed env map into `env`. Skips keys already set (env wins over files) and
 * protected keys. Returns names only — never values — for logging/status.
 */
export function applyOctocodeEnv(map, { env = process.env } = {}) {
  const applied = [];
  const skippedProtected = [];
  const skippedExisting = [];
  for (const [key, value] of Object.entries(map || {})) {
    if (PROTECTED_KEYS.has(key)) { skippedProtected.push(key); continue; }
    if (env[key] !== undefined && env[key] !== '') { skippedExisting.push(key); continue; }
    env[key] = value;
    applied.push(key);
  }
  return { applied, skippedProtected, skippedExisting };
}

/** Convenience: load + apply in one call. Returns names-only metadata. */
export function propagateOctocodeEnv({ home = getOctocodeHome(), cwd, trusted = false, env = process.env } = {}) {
  const { map, sources } = loadOctocodeEnv({ home, cwd, trusted });
  const result = applyOctocodeEnv(map, { env });
  return { ...result, sources, keys: Object.keys(map) };
}

/**
 * Read and parse `<home>/.octocoderc` (structured config; JSON with line and block comments
 * and trailing commas tolerated). Returns {} when absent or invalid. No secrets belong here.
 */
export function loadOctocoderc(home = getOctocodeHome()) {
  const raw = readTextIfExists(path.join(home, '.octocoderc'));
  if (!raw.trim()) return {};
  try {
    const stripped = raw
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/(^|[^:])\/\/.*$/gm, '$1')
      .replace(/,(\s*[}\]])/g, '$1');
    return JSON.parse(stripped);
  } catch (e) {
    process.stderr.write(`[octocode-config] Failed to parse .octocoderc: ${e.message}\n`);
    return {};
  }
}
