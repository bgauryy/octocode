import { existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';

const DIR_MODE = 0o700;

/**
 * Octocode's home directory. Set OCTOCODE_HOME to isolate caches in tests/CI.
 * Default per platform:
 *   - macOS:   `~/.octocode`
 *   - Linux:   `${XDG_CONFIG_HOME:-~/.config}/.octocode`
 *   - Windows: `%APPDATA%\.octocode`
 *
 * Implementation delegated to @octocodeai/config — single source of truth.
 */
export function getDefaultOctocodeHome(): string {
  return getOctocodeHome(process.env);
}

/** Octocode home directory — same as OCTOCODE_HOME, exposed for tool/cache use. */
export function getOctocodeDir(): string {
  return OCTOCODE_HOME;
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
  repos: join(OCTOCODE_HOME, 'tmp', 'clone'),
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
