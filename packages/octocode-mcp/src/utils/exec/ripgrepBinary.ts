import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';

let cachedPath: string | null = null;

export function resolveRipgrepBinary(): string {
  if (cachedPath !== null) return cachedPath;
  cachedPath = computePath();
  return cachedPath;
}

function computePath(): string {
  // 1. Sibling probe: Bun-compiled binaries ship rg-<platform> next to the
  //    executable (placed there by build:bin:* scripts).
  //    Also covers Homebrew `depends_on "ripgrep"` — both binaries land in
  //    the same /opt/homebrew/bin/ directory.
  const sibling = resolveSiblingRg();
  if (sibling) return sibling;

  // 2. @vscode/ripgrep: npm / npx / npm install -g users.
  //    The package installs the platform binary into node_modules.
  const bundled = resolveVscodeRipgrep();
  if (bundled) return bundled;

  // 3. PATH probe: catches any rg installed outside node_modules —
  //    system packages (apt/brew/dnf), Nix, conda, custom Homebrew bottles,
  //    or any situation where the binary isn't next to process.execPath.
  const fromPath = resolveRgFromPath();
  if (fromPath) return fromPath;

  throw new Error(
    'ripgrep (rg) is unavailable. ' +
      'Install it via: npm i -g octocode-mcp  OR  brew install ripgrep  OR  apt install ripgrep'
  );
}

/**
 * Checks for an rg binary placed next to the running executable.
 * Only activates for Bun-compiled standalone binaries — skipped when
 * process.execPath points to a standard node / bun runtime binary so
 * we never accidentally pick up a system rg during npx runs.
 *
 * The build:bin:* scripts produce:
 *   rg-darwin-arm64, rg-linux-x64, rg-windows-x64.exe, … next to the binary.
 * A plain `rg` / `rg.exe` is also accepted (Homebrew dep, user-placed file).
 */
function resolveSiblingRg(): string | null {
  try {
    const execPath = process.execPath;
    // Skip for standard runtimes — the compiled binary name never ends with
    // node, node.exe, bun, or bun.exe.
    if (/[/\\](node|bun)(\.exe)?$/.test(execPath)) return null;

    const dir = dirname(execPath);
    const ext = process.platform === 'win32' ? '.exe' : '';

    // Plain name first (Homebrew dep, user shipped it this way).
    const plain = join(dir, `rg${ext}`);
    if (existsSync(plain)) return plain;

    // Platform-suffixed name (produced by our build:bin:* scripts).
    const key = platformKey();
    if (key) {
      const suffixed = join(dir, `rg-${key}${ext}`);
      if (existsSync(suffixed)) return suffixed;
    }
  } catch {
    // Never throw — fall through.
  }
  return null;
}

/** Resolve via the @vscode/ripgrep optional npm package. */
function resolveVscodeRipgrep(): string | null {
  try {
    const mod = require('@vscode/ripgrep') as { rgPath?: string };
    if (mod.rgPath && typeof mod.rgPath === 'string') {
      return mod.rgPath;
    }
  } catch {
    // Package not installed — fall through.
  }
  return null;
}

/**
 * Last-resort probe: finds rg on the system PATH.
 * Covers Homebrew bottles installed to a non-standard prefix, system
 * package managers (apt, dnf, pacman), Nix, conda, etc.
 * Exported for testing.
 */
export function resolveRgFromPath(): string | null {
  try {
    const isWin = process.platform === 'win32';
    const which = isWin ? 'where.exe' : 'which';
    const result = spawnSync(which, ['rg'], {
      encoding: 'utf8',
      timeout: 3000,
    });
    if (result.status === 0 && result.stdout) {
      // `where` may return multiple lines; take the first.
      const resolved = (result.stdout.trim().split('\n')[0] ?? '').trim();
      if (resolved && existsSync(resolved)) return resolved;
    }
  } catch {
    // which/where not available or rg not on PATH.
  }
  return null;
}

function platformKey(): string | null {
  const p = process.platform;
  const a = process.arch;
  if (p === 'darwin' && a === 'arm64') return 'darwin-arm64';
  if (p === 'darwin' && a === 'x64') return 'darwin-x64';
  if (p === 'linux' && a === 'arm64') return 'linux-arm64';
  if (p === 'linux' && a === 'x64') return 'linux-x64';
  if (p === 'win32' && a === 'x64') return 'windows-x64';
  return null;
}
