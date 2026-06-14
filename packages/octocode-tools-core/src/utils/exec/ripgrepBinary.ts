import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { securityRegistry } from 'octocode-security/registry';
import { normalizeCommandName } from 'octocode-security/commandValidator';

const moduleDir = dirname(fileURLToPath(import.meta.url));

let cachedPath: string | null = null;

export function resolveRipgrepBinary(): string {
  if (cachedPath !== null) return cachedPath;
  cachedPath = computePath();
  allowRipgrepCommandName(cachedPath);
  return cachedPath;
}

const RG_BINARY_NAME = /^rg(-[a-z0-9-]+)?$/i;

/**
 * Bundled/sibling binaries ship platform-suffixed names (rg-darwin-arm64)
 * that are not in octocode-security's builtin command allowlist. Register
 * the exact resolved basename; anything not shaped like an rg binary
 * (plain `rg`, `rg-<platform>`, optional `.exe`) stays blocked.
 */
export function allowRipgrepCommandName(binaryPath: string): void {
  const name = normalizeCommandName(binaryPath);
  if (name === 'rg' || !RG_BINARY_NAME.test(name)) return;
  try {
    securityRegistry.addAllowedCommands([name]);
  } catch {
    // Frozen registry — validateCommand will reject and name the binary.
  }
}

function computePath(): string {
  // 1. Explicit override: useful for tests, packagers, and distro builds.
  const explicit = resolveExplicitRg();
  if (explicit) return explicit;

  // 2. MCP runtime bundle: npm/CLI builds copy rg into dist/runtime.
  const runtimeAsset = resolveRuntimeRg();
  if (runtimeAsset) return runtimeAsset;

  // 3. Sibling probe: Bun-compiled binaries ship rg-<platform> next to the
  //    executable (placed there by build:bin:* scripts).
  //    Also covers Homebrew `depends_on "ripgrep"` — both binaries land in
  //    the same /opt/homebrew/bin/ directory.
  const sibling = resolveSiblingRg();
  if (sibling) return sibling;

  // 4. @vscode/ripgrep: npm / npx / npm install -g users.
  //    The package installs the platform binary into node_modules.
  const bundled = resolveVscodeRipgrep();
  if (bundled) return bundled;

  // 5. PATH probe: catches any rg installed outside node_modules —
  //    system packages (apt/brew/dnf), Nix, conda, custom Homebrew bottles,
  //    or any situation where the binary isn't next to process.execPath.
  const fromPath = resolveRgFromPath();
  if (fromPath) return fromPath;

  throw new Error(
    'ripgrep (rg) is unavailable. ' +
      'Install it via: npm i -g octocode-mcp  OR  brew install ripgrep  OR  apt install ripgrep'
  );
}

function resolveExplicitRg(): string | null {
  const explicitPath = process.env.OCTOCODE_RG_PATH;
  if (explicitPath && existsSync(explicitPath)) return explicitPath;
  return null;
}

function resolveRuntimeRg(): string | null {
  const key = platformKey();
  if (!key) return null;

  // Fast path: baked manifest written at build time by bundle-runtime-assets.mjs.
  // Avoids probing multiple parent directories — the manifest stores the exact
  // relative path from the dist root, so one existsSync check is enough.
  const manifestPath = findRuntimeAssetsManifest();
  if (manifestPath) {
    try {
      const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
        rg?: Array<{ platform: string; file: string }>;
      };
      const entry = manifest.rg?.find(r => r.platform === key);
      if (entry) {
        const resolved = join(dirname(manifestPath), entry.file);
        if (existsSync(resolved)) return resolved;
      }
    } catch {
      // Corrupted or missing manifest — fall through to directory probe.
    }
  }

  // Fallback: scan well-known relative positions for the rg runtime directory.
  // Covers unusual bundle layouts or stripped manifest files.
  const ext = process.platform === 'win32' ? '.exe' : '';
  const names = [`rg-${key}${ext}`, `rg${ext}`];
  const dirs = [
    join(moduleDir, 'runtime', 'rg'),
    join(moduleDir, '..', 'runtime', 'rg'),
    join(moduleDir, '..', '..', 'runtime', 'rg'),
  ];

  for (const dir of dirs) {
    for (const name of names) {
      const candidate = join(dir, name);
      if (existsSync(candidate)) return candidate;
    }
  }

  return null;
}

/** Locate the runtime-assets.json manifest written by bundle-runtime-assets.mjs. */
function findRuntimeAssetsManifest(): string | null {
  const candidates = [
    join(moduleDir, 'runtime-assets.json'),
    join(moduleDir, '..', 'runtime-assets.json'),
  ];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  return null;
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
  if (process.env.OCTOCODE_DISABLE_VSCODE_RIPGREP === '1') return null;

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
