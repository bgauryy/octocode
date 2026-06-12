#!/usr/bin/env node
/**
 * Downloads the rg binary for a given platform from the npm registry
 * and copies it to the output directory next to the compiled octocode-mcp binary.
 *
 * Fast path (native build): copies directly from the locally installed
 *   @vscode/ripgrep-<platform> optional package.
 * Slow path (cross-compile): downloads the tarball from the npm registry.
 *
 * Usage:
 *   node scripts/bundle-rg.mjs <platform> <outDir>
 *
 * Platforms:
 *   darwin-arm64 | darwin-x64 | linux-arm64 | linux-x64 | linux-x64-musl | windows-x64
 *
 * Output files:
 *   <outDir>/rg-darwin-arm64
 *   <outDir>/rg-linux-x64
 *   <outDir>/rg-windows-x64.exe
 *   …etc
 */

import {
  chmodSync,
  copyFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
} from 'node:fs';
import { rm, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { get } from 'node:https';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

// Read the version directly from package.json so it never drifts.
const pkg = require('../package.json');
const ripgrepVersion =
  pkg.dependencies?.['@vscode/ripgrep'] ??
  pkg.devDependencies?.['@vscode/ripgrep'];
if (!ripgrepVersion) {
  throw new Error('Missing @vscode/ripgrep version in package.json');
}
const RG_VERSION = ripgrepVersion.replace(/^[~^]/, '');

/** @type {Record<string, { vscodeArch: string; binary: string }>} */
const PLATFORM_MAP = {
  'darwin-arm64':   { vscodeArch: 'darwin-arm64',  binary: 'rg'     },
  'darwin-x64':     { vscodeArch: 'darwin-x64',    binary: 'rg'     },
  'linux-arm64':    { vscodeArch: 'linux-arm64',   binary: 'rg'     },
  'linux-x64':      { vscodeArch: 'linux-x64',     binary: 'rg'     },
  'linux-x64-musl': { vscodeArch: 'linux-x64',     binary: 'rg'     }, // @vscode/ripgrep's linux-x64 rg is static-pie linked (no glibc/musl interpreter), so it runs on Alpine/musl too
  'windows-x64':    { vscodeArch: 'win32-x64',     binary: 'rg.exe' },
};

async function main() {
  const [platform, outDir] = process.argv.slice(2);

  if (!platform || !outDir) {
    console.error('Usage: node scripts/bundle-rg.mjs <platform> <outDir>');
    console.error('  Platforms:', Object.keys(PLATFORM_MAP).join(', '));
    process.exit(1);
  }

  const config = PLATFORM_MAP[platform];
  if (!config) {
    console.error(`Unknown platform: ${platform}`);
    console.error('  Valid platforms:', Object.keys(PLATFORM_MAP).join(', '));
    process.exit(1);
  }

  const isWindows = platform === 'windows-x64';
  const outExt = isWindows ? '.exe' : '';
  const outFile = join(outDir, `rg-${platform}${outExt}`);

  await mkdir(outDir, { recursive: true });

  // Fast path: use locally installed optional package (native platform build).
  const localPkgName = `@vscode/ripgrep-${config.vscodeArch}`;
  const localPath = tryLocalPackage(localPkgName, config.binary);
  if (localPath) {
    console.log(`bundle-rg: copying from local ${localPkgName}`);
    copyFileSync(localPath, outFile);
    if (!isWindows) chmodSync(outFile, 0o755);
    copyToRuntimeBundle(outFile, outDir, platform, outExt, isWindows);
    console.log(`bundle-rg: ✓ ${outFile}`);
    return;
  }

  // Slow path: download from npm registry (cross-platform build).
  console.log(`bundle-rg: downloading ${localPkgName}@${RG_VERSION} from npm registry`);
  const tmpDir = join(tmpdir(), `bundle-rg-${platform}-${Date.now()}`);
  try {
    await mkdir(tmpDir, { recursive: true });
    const tgz = join(tmpDir, 'pkg.tgz');
    const tgzUrl = npmTarballUrl(config.vscodeArch, RG_VERSION);
    await download(tgzUrl, tgz);
    extractBinaryFromTgz(tgz, `package/bin/${config.binary}`, outFile, tmpDir);
    if (!isWindows) chmodSync(outFile, 0o755);
    copyToRuntimeBundle(outFile, outDir, platform, outExt, isWindows);
    console.log(`bundle-rg: ✓ ${outFile}`);
  } finally {
    await rm(tmpDir, { recursive: true, force: true });
  }
}

function copyToRuntimeBundle(outFile, outDir, platform, outExt, isWindows) {
  const runtimeDir = join(outDir, 'runtime', 'rg');
  const runtimeFile = join(runtimeDir, `rg-${platform}${outExt}`);
  mkdirSync(runtimeDir, { recursive: true });
  copyFileSync(outFile, runtimeFile);
  if (!isWindows) chmodSync(runtimeFile, 0o755);
}

/** Try resolving the binary from the already-installed optional package. */
function tryLocalPackage(pkgName, binaryName) {
  try {
    const pkgJson = require(`${pkgName}/package.json`);
    if (!pkgJson) return null;
    // Derive bin path from the package's installation directory.
    const pkgJsonPath = require.resolve(`${pkgName}/package.json`);
    const pkgDir = pkgJsonPath.replace(/[/\\]package\.json$/, '');
    const binPath = join(pkgDir, 'bin', binaryName);
    return existsSync(binPath) ? binPath : null;
  } catch {
    return null;
  }
}

/** npm tarball URL for a scoped @vscode/ripgrep-<arch> package. */
function npmTarballUrl(vscodeArch, version) {
  return `https://registry.npmjs.org/@vscode/ripgrep-${vscodeArch}/-/ripgrep-${vscodeArch}-${version}.tgz`;
}

/** Download a URL to a local file. */
function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);
    const req = get(url, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        file.close();
        download(res.headers.location, dest).then(resolve).catch(reject);
        return;
      }
      if (res.statusCode !== 200) {
        file.close();
        reject(new Error(`HTTP ${res.statusCode} for ${url}`));
        return;
      }
      res.pipe(file);
      file.on('finish', () => file.close(resolve));
    });
    req.on('error', reject);
  });
}

/** Extract a single file from a .tgz archive using the system tar command. */
function extractBinaryFromTgz(tgzPath, entryPath, destPath, cwd) {
  execFileSync('tar', ['-xzf', tgzPath, '--strip-components=2', '-C', cwd, entryPath], {
    cwd,
    stdio: 'inherit',
  });
  const extracted = join(cwd, entryPath.split('/').pop());
  copyFileSync(extracted, destPath);
}

main().catch((err) => {
  console.error('bundle-rg failed:', err.message);
  process.exit(1);
});
