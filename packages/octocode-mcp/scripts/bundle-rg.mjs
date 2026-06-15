#!/usr/bin/env node
/**
 * Downloads the rg binary for a given platform from the npm registry
 * and copies it to the output directory next to the compiled octocode-mcp binary.
 *
 * Fast path (native build): copies directly from the locally installed
 *   @vscode/ripgrep-<platform> optional package.
 * Slow path (cross-compile): downloads the tarball from the npm registry,
 *   verifies SHA-512 integrity against npm metadata, then extracts the binary.
 *
 * Usage:
 *   node scripts/bundle-rg.mjs <platform> <outDir> [--runtime-only]
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
  readFileSync,
} from 'node:fs';
import { rm, mkdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync, spawnSync } from 'node:child_process';
import { get } from 'node:https';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';

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

const DOWNLOAD_TIMEOUT_MS = 30_000;
const MAX_REDIRECT_DEPTH = 5;
const MAX_NETWORK_ATTEMPTS = 3;
const NETWORK_RETRY_DELAY_MS = 1_000;

/** @type {Record<string, { vscodeArch: string; binary: string }>} */
const PLATFORM_MAP = {
  'darwin-arm64': { vscodeArch: 'darwin-arm64', binary: 'rg' },
  'darwin-x64': { vscodeArch: 'darwin-x64', binary: 'rg' },
  'linux-arm64': { vscodeArch: 'linux-arm64', binary: 'rg' },
  'linux-x64': { vscodeArch: 'linux-x64', binary: 'rg' },
  // @vscode/ripgrep's linux-x64 rg is static-PIE linked (no glibc/musl
  // interpreter), so it runs on Alpine/musl too. verifyStaticBinary() confirms
  // this at build time so a future linkage change is caught immediately.
  'linux-x64-musl': { vscodeArch: 'linux-x64', binary: 'rg' },
  'windows-x64': { vscodeArch: 'win32-x64', binary: 'rg.exe' },
};

async function main() {
  const [platform, outDir, ...flags] = process.argv.slice(2);
  const runtimeOnly = flags.includes('--runtime-only');

  if (!platform || !outDir) {
    console.error(
      'Usage: node scripts/bundle-rg.mjs <platform> <outDir> [--runtime-only]'
    );
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
  const outFile = runtimeOnly
    ? join(outDir, 'runtime', 'rg', `rg-${platform}${outExt}`)
    : join(outDir, `rg-${platform}${outExt}`);

  await mkdir(dirname(outFile), { recursive: true });

  // Fast path: use locally installed optional package (native platform build).
  // npm/yarn already verified its integrity during install — no re-check needed.
  const localPkgName = `@vscode/ripgrep-${config.vscodeArch}`;
  const localPath = tryLocalPackage(localPkgName, config.binary);
  if (localPath) {
    console.log(`bundle-rg: copying from local ${localPkgName}`);
    copyFileSync(localPath, outFile);
    if (!isWindows) chmodSync(outFile, 0o755);
    verifyStaticBinary(outFile, platform);
    if (!runtimeOnly) {
      copyToRuntimeBundle(outFile, outDir, platform, outExt, isWindows);
    }
    console.log(`bundle-rg: ✓ ${outFile}`);
    return;
  }

  // Slow path: download from npm registry (cross-platform build).
  console.log(
    `bundle-rg: downloading ${localPkgName}@${RG_VERSION} from npm registry`
  );

  // Prefetch integrity from registry metadata so we can verify the tarball.
  const integrity = await fetchPackageIntegrity(config.vscodeArch, RG_VERSION);
  console.log(`bundle-rg: fetched integrity for ${localPkgName}@${RG_VERSION}`);

  const tmpDir = join(tmpdir(), `bundle-rg-${platform}-${Date.now()}`);
  try {
    await mkdir(tmpDir, { recursive: true });
    const tgz = join(tmpDir, 'pkg.tgz');
    const tgzUrl = npmTarballUrl(config.vscodeArch, RG_VERSION);
    await downloadWithRetry(tgzUrl, tgz);
    verifyIntegrity(tgz, integrity);
    extractBinaryFromTgz(tgz, `package/bin/${config.binary}`, outFile, tmpDir);
    if (!isWindows) chmodSync(outFile, 0o755);
    verifyStaticBinary(outFile, platform);
    if (!runtimeOnly) {
      copyToRuntimeBundle(outFile, outDir, platform, outExt, isWindows);
    }
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

/**
 * Fetch the SRI integrity string (sha512-<base64>) for a specific version of
 * @vscode/ripgrep-<arch> from the npm registry. This is mandatory for
 * cross-platform publish builds; do not use downloaded binaries unchecked.
 */
function fetchPackageIntegrity(vscodeArch, version) {
  const url = `https://registry.npmjs.org/@vscode/ripgrep-${vscodeArch}/${version}`;
  return withNetworkRetries(`fetch integrity metadata for ${url}`, () =>
    new Promise((resolve, reject) => {
      const req = get(url, res => {
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} for ${url}`));
          return;
        }
        let data = '';
        res.setEncoding('utf8');
        res.on('data', chunk => {
          data += chunk;
        });
        res.on('end', () => {
          try {
            const meta = JSON.parse(data);
            const integrity = meta.dist?.integrity;
            if (typeof integrity !== 'string' || integrity.length === 0) {
              reject(new Error(`Missing dist.integrity in ${url}`));
              return;
            }
            resolve(integrity);
          } catch (error) {
            reject(error);
          }
        });
      });
      req.setTimeout(DOWNLOAD_TIMEOUT_MS, () => {
        req.destroy(new Error(`Request timed out after ${DOWNLOAD_TIMEOUT_MS}ms`));
      });
      req.on('error', reject);
    })
  );
}

/**
 * Verify a downloaded file against an SRI integrity string (e.g. "sha512-abc…").
 * Throws if the digest does not match — never silently accepts a bad binary.
 */
function verifyIntegrity(filePath, integrity) {
  const dashIdx = integrity.indexOf('-');
  if (dashIdx === -1) {
    throw new Error(`Unsupported integrity format: ${integrity}`);
  }
  const algo = integrity.slice(0, dashIdx);
  const expectedB64 = integrity.slice(dashIdx + 1);
  const hash = createHash(algo);
  hash.update(readFileSync(filePath));
  const actualB64 = hash.digest('base64');
  if (actualB64 !== expectedB64) {
    throw new Error(
      `Integrity check FAILED for downloaded ripgrep tarball.\n` +
        `  Expected: ${algo}-${expectedB64}\n` +
        `  Got:      ${algo}-${actualB64}\n` +
        `Refusing to use a binary that did not pass integrity verification.`
    );
  }
  console.log(`bundle-rg: ✓ integrity verified (${algo})`);
}

/**
 * Verify the linux-x64-musl binary is statically linked at build time using
 * the system `file` command. Fails hard if the binary is dynamically linked —
 * a dynamic binary would segfault on Alpine and produce a silent runtime failure.
 *
 * This check is skipped on platforms where `file` is unavailable (non-fatal)
 * and on all non-musl targets (they can tolerate dynamic linking).
 */
function verifyStaticBinary(binaryPath, platform) {
  if (platform !== 'linux-x64-musl') return;

  const result = spawnSync('file', [binaryPath], {
    encoding: 'utf8',
    timeout: 5_000,
  });

  if (result.status !== 0 || !result.stdout) {
    throw new Error(
      `Could not verify static linkage for ${platform}.\n` +
        `  status: ${result.status ?? 'unknown'}\n` +
        `  stderr: ${result.stderr || '(none)'}`
    );
  }

  const output = result.stdout.toLowerCase();
  const isStatic =
    output.includes('statically linked') || output.includes('static-pie');

  if (!isStatic) {
    throw new Error(
      `linux-x64-musl rg binary is NOT statically linked.\n` +
        `  file output: ${result.stdout.trim()}\n` +
        `This binary will NOT work on Alpine/musl Linux. ` +
        `Check whether @vscode/ripgrep changed its linkage and update the build accordingly.`
    );
  }

  console.log(`bundle-rg: ✓ ${platform} binary is statically linked`);
}

/**
 * Download a URL to a local file with timeout and redirect-depth protection.
 * Throws on HTTP errors, timeout, or too many redirects.
 */
function downloadWithRetry(url, dest) {
  return withNetworkRetries(`download ${url}`, async () => {
    await rm(dest, { force: true });
    await download(url, dest);
  });
}

function download(url, dest, depth = 0) {
  if (depth > MAX_REDIRECT_DEPTH) {
    return Promise.reject(
      new Error(
        `Too many redirects (>${MAX_REDIRECT_DEPTH}) fetching ${url} — aborting`
      )
    );
  }

  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);

    const req = get(url, res => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        file.close();
        const location = res.headers.location;
        if (!location) {
          reject(new Error(`Redirect with no Location header from ${url}`));
          return;
        }
        // Clean up the partial file before retrying at the new location.
        rm(dest, { force: true })
          .then(() => download(location, dest, depth + 1))
          .then(resolve)
          .catch(reject);
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

    req.setTimeout(DOWNLOAD_TIMEOUT_MS, () => {
      req.destroy();
      file.close();
      reject(
        new Error(
          `Download timed out after ${DOWNLOAD_TIMEOUT_MS / 1000}s: ${url}`
        )
      );
    });

    req.on('error', reject);
  });
}

async function withNetworkRetries(label, operation) {
  let lastError;
  for (let attempt = 1; attempt <= MAX_NETWORK_ATTEMPTS; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      if (attempt === MAX_NETWORK_ATTEMPTS) break;
      const message = error instanceof Error ? error.message : String(error);
      console.warn(
        `bundle-rg: ${label} failed on attempt ${attempt}/${MAX_NETWORK_ATTEMPTS}: ${message}`
      );
      await delay(NETWORK_RETRY_DELAY_MS * attempt);
    }
  }

  const message =
    lastError instanceof Error ? lastError.message : String(lastError);
  throw new Error(`${label} failed after ${MAX_NETWORK_ATTEMPTS} attempts: ${message}`);
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/** Extract a single file from a .tgz archive using the system tar command. */
function extractBinaryFromTgz(tgzPath, entryPath, destPath, cwd) {
  execFileSync(
    'tar',
    ['-xzf', tgzPath, '--strip-components=2', '-C', cwd, entryPath],
    {
      cwd,
      stdio: 'inherit',
    }
  );
  const extracted = join(cwd, entryPath.split('/').pop());
  copyFileSync(extracted, destPath);
}

main().catch(err => {
  console.error('bundle-rg failed:', err.message);
  process.exit(1);
});
