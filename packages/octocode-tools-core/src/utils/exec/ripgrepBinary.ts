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

export function allowRipgrepCommandName(binaryPath: string): void {
  const name = normalizeCommandName(binaryPath);
  if (name === 'rg' || !RG_BINARY_NAME.test(name)) return;
  try {
    securityRegistry.addAllowedCommands([name]);
  } catch {
    /* silently ignore */
  }
}

function computePath(): string {
  const explicit = resolveExplicitRg();
  if (explicit) return explicit;

  const runtimeAsset = resolveRuntimeRg();
  if (runtimeAsset) return runtimeAsset;

  const sibling = resolveSiblingRg();
  if (sibling) return sibling;

  const bundled = resolveVscodeRipgrep();
  if (bundled) return bundled;

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
      /* silently ignore */
    }
  }

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

function resolveSiblingRg(): string | null {
  try {
    const execPath = process.execPath;
    if (/[/\\](node|bun)(\.exe)?$/.test(execPath)) return null;

    const dir = dirname(execPath);
    const ext = process.platform === 'win32' ? '.exe' : '';

    const plain = join(dir, `rg${ext}`);
    if (existsSync(plain)) return plain;

    const key = platformKey();
    if (key) {
      const suffixed = join(dir, `rg-${key}${ext}`);
      if (existsSync(suffixed)) return suffixed;
    }
  } catch {
    /* silently ignore */
  }
  return null;
}

function resolveVscodeRipgrep(): string | null {
  if (process.env.OCTOCODE_DISABLE_VSCODE_RIPGREP === '1') return null;

  // 1. Official entry point. @vscode/ripgrep is ESM-only ("type":"module",
  //    no `require` export condition), so this only succeeds where require(esm)
  //    is available (Node >=20.19 / >=22.12); older Node throws ERR_REQUIRE_ESM.
  try {
    const mod = require('@vscode/ripgrep') as { rgPath?: string };
    if (mod.rgPath && typeof mod.rgPath === 'string' && existsSync(mod.rgPath)) {
      return mod.rgPath;
    }
  } catch {
    /* ESM-only require can throw on older Node — fall through to the binary */
  }

  // 2. Resolve the per-platform binary package directly. require.resolve on a
  //    file path never loads the ESM entry, so this works on every supported
  //    Node version. Mirrors @vscode/ripgrep/lib/index.js's own resolution
  //    (the platform packages ship the binary and declare no `exports`).
  try {
    const arch = process.env.npm_config_arch || process.arch;
    const binaryName = process.platform === 'win32' ? 'rg.exe' : 'rg';
    const platformPkg = `@vscode/ripgrep-${process.platform}-${arch}`;
    const resolved = require.resolve(`${platformPkg}/bin/${binaryName}`);
    if (existsSync(resolved)) return resolved;
  } catch {
    /* silently ignore */
  }
  return null;
}

export function resolveRgFromPath(): string | null {
  try {
    const isWin = process.platform === 'win32';
    const which = isWin ? 'where.exe' : 'which';
    const result = spawnSync(which, ['rg'], {
      encoding: 'utf8',
      timeout: 3000,
    });
    if (result.status === 0 && result.stdout) {
      const resolved = (result.stdout.trim().split('\n')[0] ?? '').trim();
      if (resolved && existsSync(resolved)) return resolved;
    }
  } catch {
    /* silently ignore */
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
