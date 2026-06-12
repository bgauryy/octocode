#!/usr/bin/env node
/**
 * Copies native runtime assets owned by octocode-mcp into dist/runtime.
 *
 * octocode-security builds the Rust .node binary; octocode-mcp packages it
 * alongside the rg binary so downstream wrappers such as octocode-cli can copy
 * one MCP runtime bundle without knowing each dependency's internals.
 */
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(__dirname, '..');
const distDir = join(packageRoot, 'dist');
const runtimeDir = join(distDir, 'runtime');

const platform = process.platform;
const arch = process.arch;

const securityTripleMap = {
  darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
  linux: { arm64: 'linux-arm64-gnu', x64: 'linux-x64-gnu' },
  win32: { x64: 'win32-x64-msvc' },
};

const rgPlatformMap = {
  darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
  linux: { arm64: 'linux-arm64', x64: 'linux-x64' },
  win32: { x64: 'windows-x64' },
};

const securityTriple = securityTripleMap[platform]?.[arch];
const rgPlatform = rgPlatformMap[platform]?.[arch];

if (!securityTriple || !rgPlatform) {
  throw new Error(`Unsupported runtime asset platform: ${platform}-${arch}`);
}

const copiedAssets = {
  platform,
  arch,
  security: copySecurityNative(securityTriple),
  rg: copyRipgrep(rgPlatform),
};

writeFileSync(
  join(distDir, 'runtime-assets.json'),
  `${JSON.stringify(copiedAssets, null, 2)}\n`
);

console.log('✓ bundled octocode-mcp runtime assets');

function copySecurityNative(triple) {
  const binaryName = `octocode-security.${triple}.node`;
  const source = join(
    packageRoot,
    '..',
    'octocode-security',
    binaryName
  );
  const destination = join(runtimeDir, 'security', binaryName);

  if (!existsSync(source)) {
    throw new Error(
      `Missing octocode-security native binary: ${source}. ` +
        'Build packages/octocode-security before packages/octocode-mcp.'
    );
  }

  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  chmodSync(destination, 0o755);

  return relative(distDir, destination);
}

function copyRipgrep(rgPlatformName) {
  const source = resolveRipgrepSource();
  const binaryName = `rg-${rgPlatformName}${platform === 'win32' ? '.exe' : ''}`;
  const destination = join(runtimeDir, 'rg', binaryName);

  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  if (platform !== 'win32') chmodSync(destination, 0o755);

  return relative(distDir, destination);
}

function resolveRipgrepSource() {
  try {
    const mod = require('@vscode/ripgrep');
    if (
      mod &&
      typeof mod.rgPath === 'string' &&
      existsSync(mod.rgPath)
    ) {
      return mod.rgPath;
    }
  } catch {
    // Fall through to a clearer error below.
  }

  throw new Error(
    'Missing @vscode/ripgrep binary. Run yarn install before building octocode-mcp.'
  );
}
