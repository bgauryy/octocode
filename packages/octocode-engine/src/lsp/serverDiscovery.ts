import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';

import { detectIdeContext } from './ideContext.js';
import { firstExecutableIn } from './platform.js';

/**
 * Platformized language-server discovery.
 *
 * The native layer (`config.rs`) already resolves explicit overrides
 * (`OCTOCODE_*_SERVER_PATH`, `.octocode/lsp-servers.json`) and anything on
 * `PATH` via `which`. This module covers the gap that bites in production: a
 * server IS installed, but in a per-ecosystem directory that a GUI-launched
 * IDE's (or a bare `spawn`'s) impoverished `PATH` never sees — `~/.cargo/bin`,
 * `~/go/bin`, `~/.local/bin`, the npm/pnpm global prefixes, the Neovim `mason`
 * dir, the Homebrew prefix, version-manager shims, and the per-OS Windows
 * shim dirs.
 *
 * ## Performance (CLI-critical)
 *
 * `lsp-server list` calls `discoverServer` for every known server with the
 * same workspaceRoot, so cold-start cost matters. Three caches work together:
 *
 *   1. `_discoveryCache`     — result per `(command, workspaceRoot)`. After the
 *      first call, every subsequent call for the same pair is O(1): zero fs ops.
 *
 *   2. `_existingEcoDirs`    — ecosystem dirs filtered to only those that
 *      exist on disk. Computed once per process (or after `clearDiscoveryCache`).
 *      Turns ~15 dirs × N servers into ~15 stats once + only real-dir probes.
 *
 *   3. `_projectLocalDirs`   — resolved project-local path lists per workspace
 *      root (avoids re-walking the directory tree).
 *
 * Call `clearDiscoveryCache()` after installing a server mid-session.
 *
 * Search order (first hit wins): project-local → known ecosystem dirs →
 * IDE-bundled extension servers. Returns an absolute path plus a provenance
 * label, or `null` if nothing on the machine provides the command.
 */
export type DiscoverySource =
  | 'project-local'
  | `ecosystem:${string}`
  | 'ide-bundled';

export interface DiscoveredServer {
  command: string;
  source: DiscoverySource;
}

/** Per-command-per-workspace result. `undefined` = not yet looked up. */
const _discoveryCache = new Map<string, DiscoveredServer | null>();

/** Ecosystem dirs that actually exist; `null` = not yet computed. */
let _existingEcoDirs: Array<{ dir: string; label: string }> | null = null;

/** Project-local dir lists per resolved workspace root. */
const _projectLocalDirs = new Map<string, string[]>();

/**
 * Invalidate all discovery caches. Call after programmatically installing a
 * server (e.g. `provisionServer`) or in tests between scenarios.
 */
export function clearDiscoveryCache(): void {
  _discoveryCache.clear();
  _existingEcoDirs = null;
  _projectLocalDirs.clear();
}

const HOME = homedir();
const join = path.join;

function envPath(...segments: string[]): string {
  return join(...segments);
}

/**
 * Every ecosystem `bin` directory worth probing on this OS, paired with a
 * label for provenance. Computed from env vars + canonical defaults — no
 * subprocess spawning, so it stays cheap to call per resolution.
 */
function ecosystemBinDirs(): Array<{ dir: string; label: string }> {
  const env = process.env;
  const out: Array<{ dir: string; label: string }> = [];
  const add = (dir: string | undefined, label: string) => {
    if (dir) out.push({ dir, label });
  };

  if (process.platform === 'win32') {
    const appData = env.APPDATA;
    const localAppData = env.LOCALAPPDATA;
    add(env.USERPROFILE && envPath(env.USERPROFILE, '.cargo', 'bin'), 'cargo');
    add(env.USERPROFILE && envPath(env.USERPROFILE, 'go', 'bin'), 'go');
    // Go Windows installer default: C:\Go\bin (distinct from %USERPROFILE%\go\bin)
    add('C:\\Go\\bin', 'go');
    // LLVM/clangd: official Windows installer defaults
    add('C:\\Program Files\\LLVM\\bin', 'llvm');
    add('C:\\Program Files (x86)\\LLVM\\bin', 'llvm');
    // .NET global tools (csharp-ls, OmniSharp, etc.)
    add(env.USERPROFILE && envPath(env.USERPROFILE, '.dotnet', 'tools'), 'dotnet');
    add(appData && envPath(appData, 'npm'), 'npm-global');
    add(localAppData && envPath(localAppData, 'pnpm'), 'pnpm');
    add(env.USERPROFILE && envPath(env.USERPROFILE, 'scoop', 'shims'), 'scoop');
    add(env.ProgramData && envPath(env.ProgramData, 'chocolatey', 'bin'), 'choco');
    add(localAppData && envPath(localAppData, 'Microsoft', 'WinGet', 'Links'), 'winget');
    add(localAppData && envPath(localAppData, 'nvim-data', 'mason', 'bin'), 'mason');
    return out;
  }

  // Rust
  add(envPath(HOME, '.cargo', 'bin'), 'cargo');
  // Go
  add(env.GOBIN, 'go');
  add(env.GOPATH ? envPath(env.GOPATH, 'bin') : undefined, 'go');
  add(envPath(HOME, 'go', 'bin'), 'go');
  // Python user installs
  add(envPath(HOME, '.local', 'bin'), 'python-user');
  // Node global (env-driven prefixes + canonical defaults)
  add(env.npm_config_prefix ? envPath(env.npm_config_prefix, 'bin') : undefined, 'npm-global');
  add(env.PNPM_HOME, 'pnpm');
  add(envPath(HOME, '.volta', 'bin'), 'volta');
  add(envPath(HOME, '.asdf', 'shims'), 'asdf');
  add(envPath(HOME, '.local', 'share', 'mise', 'shims'), 'mise');
  // Neovim mason (high-value on dev machines)
  add(envPath(HOME, '.local', 'share', 'nvim', 'mason', 'bin'), 'mason');
  // Homebrew (Apple Silicon first — the #1 "installed but not found" cause)
  add('/opt/homebrew/bin', 'homebrew');
  add('/usr/local/bin', 'homebrew');
  add('/home/linuxbrew/.linuxbrew/bin', 'homebrew');
  // System binary dirs: clangd ships in /usr/bin on macOS (Xcode CLI tools) and
  // on Linux distros that install it via the package manager. Check after homebrew
  // so a homebrew-managed version takes precedence over the system one.
  add('/usr/bin', 'system');
  // macOS framework Python: ~/Library/Python/<X.Y>/bin
  if (process.platform === 'darwin') {
    add(envPath(HOME, 'Library', 'Python'), 'python-framework');
  }
  // .NET global tools (csharp-ls, OmniSharp, etc.)
  add(envPath(HOME, '.dotnet', 'tools'), 'dotnet');
  // Active virtualenv / conda
  add(env.VIRTUAL_ENV ? envPath(env.VIRTUAL_ENV, 'bin') : undefined, 'venv');
  add(env.CONDA_PREFIX ? envPath(env.CONDA_PREFIX, 'bin') : undefined, 'conda');
  return out;
}

/**
 * Ecosystem dirs that exist on this machine, cached for the process lifetime.
 * Pre-filtering eliminates stat calls for non-existent dirs on every lookup —
 * on a typical dev machine this shrinks the probe list from ~15 to ~5.
 */
function existingEcoDirs(): Array<{ dir: string; label: string }> {
  if (!_existingEcoDirs) {
    _existingEcoDirs = ecosystemBinDirs().filter(({ dir }) => existsSync(dir));
  }
  return _existingEcoDirs;
}

/** Project-local server dirs, walking up from the workspace root. */
function buildProjectLocalDirs(workspaceRoot: string): string[] {
  const dirs: string[] = [];
  let current = workspaceRoot; // already resolved by callers
  for (;;) {
    dirs.push(join(current, 'node_modules', '.bin'));
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  const venvBin = process.platform === 'win32' ? 'Scripts' : 'bin';
  dirs.push(join(workspaceRoot, '.venv', venvBin));
  if (process.env.VIRTUAL_ENV) dirs.push(join(process.env.VIRTUAL_ENV, venvBin));
  return dirs;
}

function cachedProjectLocalDirs(resolvedRoot: string): string[] {
  let dirs = _projectLocalDirs.get(resolvedRoot);
  if (!dirs) {
    dirs = buildProjectLocalDirs(resolvedRoot);
    _projectLocalDirs.set(resolvedRoot, dirs);
  }
  return dirs;
}

/**
 * VS Code (and fork) extension roots whose `<ext>/server/` dirs may hold a
 * reusable server. Skipped unless we detect an IDE host.
 */
function ideExtensionRoots(): string[] {
  const { isIde, host } = detectIdeContext();
  if (!isIde) return [];
  const roots: Record<string, string> = {
    vscode: join(HOME, '.vscode', 'extensions'),
    cursor: join(HOME, '.cursor', 'extensions'),
    windsurf: join(HOME, '.windsurf', 'extensions'),
  };
  const candidates = [roots[host], join(HOME, '.vscode-server', 'extensions')].filter(
    (dir): dir is string => Boolean(dir)
  );
  return candidates.filter(existsSync);
}

function resolveCommand(command: string): string {
  return path.basename(command);
}

function resolveRoot(workspaceRoot: string): string {
  return path.resolve(workspaceRoot);
}

function cacheKey(command: string, resolvedRoot: string): string {
  return `${command}\0${resolvedRoot}`;
}

/**
 * Core lookup — runs only on cache miss. Assumes `command` is a basename and
 * `resolvedRoot` is an absolute path.
 */
function scan(command: string, resolvedRoot: string): DiscoveredServer | null {
  if (!command) return null;

  // 1) Project-local — matches the project's pinned toolchain.
  for (const dir of cachedProjectLocalDirs(resolvedRoot)) {
    const hit = firstExecutableIn(dir, command, join);
    if (hit) return { command: hit, source: 'project-local' };
  }

  // 2) Known ecosystem dirs (pre-filtered to existing).
  for (const { dir, label } of existingEcoDirs()) {
    const hit = firstExecutableIn(dir, command, join);
    if (hit) return { command: hit, source: `ecosystem:${label}` };
  }

  // 3) IDE-bundled extension servers (only when hosted in an IDE).
  for (const root of ideExtensionRoots()) {
    if (existsSync(root)) {
      // No safe generic launch — see LSP_GUIDE.md "IDE reuse".
      break;
    }
  }

  return null;
}

/**
 * Locate `command` on this machine outside of `PATH`. `command` is the bare
 * server name (e.g. `rust-analyzer`, `gopls`); absolute commands are returned
 * by the caller before this is reached.
 *
 * Results are memoised for the process lifetime. Call `clearDiscoveryCache()`
 * after installing a server mid-session.
 */
export function discoverServer(
  command: string,
  workspaceRoot: string
): DiscoveredServer | null {
  const base = resolveCommand(command);
  const resolved = resolveRoot(workspaceRoot);
  const key = cacheKey(base, resolved);

  if (_discoveryCache.has(key)) {
    return _discoveryCache.get(key) ?? null;
  }

  const result = scan(base, resolved);
  _discoveryCache.set(key, result);
  return result;
}

/**
 * Discover multiple servers in one call, sharing the ecosystem-dir pre-filter
 * and project-local dir computation across all commands. Results are written
 * into the shared cache, so subsequent `discoverServer` calls for the same
 * `(command, workspaceRoot)` pair are O(1).
 *
 * Equivalent to calling `discoverServer` for each command individually, but
 * avoids redundant work when the caller needs results for many commands against
 * the same workspace (e.g. the `lsp-server list` CLI command).
 */
export function discoverServerBatch(
  commands: string[],
  workspaceRoot: string
): Record<string, DiscoveredServer | null> {
  const resolved = resolveRoot(workspaceRoot);
  const results: Record<string, DiscoveredServer | null> = {};

  for (const command of commands) {
    const base = resolveCommand(command);
    const key = cacheKey(base, resolved);

    if (_discoveryCache.has(key)) {
      results[command] = _discoveryCache.get(key) ?? null;
      continue;
    }

    const result = scan(base, resolved);
    _discoveryCache.set(key, result);
    results[command] = result;
  }

  return results;
}
