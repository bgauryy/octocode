import { existsSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { HOME, getAppDataPath, isMac, isWindows } from './platform/index.js';

const DIR_NAME = '.octocode';
const DIR_MODE = 0o700;

function readNonEmptyEnv(name: string): string | undefined {
  const value = process.env[name];
  return value && value.trim().length > 0 ? value : undefined;
}

/**
 * Octocode's home directory. Set OCTOCODE_HOME to isolate caches in tests/CI.
 * Default per platform:
 *   - macOS:   `~/.octocode`
 *   - Linux:   `${XDG_CONFIG_HOME:-~/.config}/.octocode`
 *   - Windows: `%APPDATA%\.octocode`
 */
export function getDefaultOctocodeHome(): string {
  const override = readNonEmptyEnv('OCTOCODE_HOME');
  if (override) return resolve(override);

  if (isWindows) {
    return join(getAppDataPath(), DIR_NAME);
  }

  if (isMac) {
    return join(HOME, DIR_NAME);
  }

  return join(
    readNonEmptyEnv('XDG_CONFIG_HOME') ?? join(HOME, '.config'),
    DIR_NAME
  );
}

export const OCTOCODE_HOME = getDefaultOctocodeHome();

export const paths = {
  home: OCTOCODE_HOME,
  config: join(OCTOCODE_HOME, '.octocoderc'),
  credentials: join(OCTOCODE_HOME, 'credentials.json'),
  key: join(OCTOCODE_HOME, '.key'),
  session: join(OCTOCODE_HOME, 'session.json'),
  stats: join(OCTOCODE_HOME, 'stats.json'),
  tmp: join(OCTOCODE_HOME, 'tmp'),
  clone: join(OCTOCODE_HOME, 'tmp', 'clone'),
  tree: join(OCTOCODE_HOME, 'tmp', 'tree'),
  binary: join(OCTOCODE_HOME, 'tmp', 'binary'),
  repos: join(OCTOCODE_HOME, 'tmp', 'clone'),
  unzip: join(OCTOCODE_HOME, 'tmp', 'unzip'),
  cliConfig: join(OCTOCODE_HOME, 'config.json'),
  lspConfig: join(OCTOCODE_HOME, 'lsp-servers.json'),
} as const;

export function ensureHome(): void {
  if (!existsSync(paths.home)) {
    mkdirSync(paths.home, { recursive: true, mode: DIR_MODE });
  }
}

export function ensureRepos(): void {
  ensureClone();
}

export function ensureTmp(): void {
  ensureHome();
  if (!existsSync(paths.tmp)) {
    mkdirSync(paths.tmp, { recursive: true, mode: DIR_MODE });
  }
}

export function ensureClone(): void {
  ensureTmp();
  if (!existsSync(paths.clone)) {
    mkdirSync(paths.clone, { recursive: true, mode: DIR_MODE });
  }
}

export function ensureTree(): void {
  ensureTmp();
  if (!existsSync(paths.tree)) {
    mkdirSync(paths.tree, { recursive: true, mode: DIR_MODE });
  }
}

export function ensureBinary(): void {
  ensureTmp();
  if (!existsSync(paths.binary)) {
    mkdirSync(paths.binary, { recursive: true, mode: DIR_MODE });
  }
}

export function ensureUnzip(): void {
  ensureTmp();
  if (!existsSync(paths.unzip)) {
    mkdirSync(paths.unzip, { recursive: true, mode: DIR_MODE });
  }
}
