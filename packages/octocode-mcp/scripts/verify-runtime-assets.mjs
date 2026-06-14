#!/usr/bin/env node
/**
 * Verifies that all ripgrep binaries are present in dist/runtime/rg.
 *
 * octocode-security and @octocodeai/octocode-context-utils are no longer
 * verified here — they are installed as npm runtime dependencies with
 * per-platform optionalDependencies.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(__dirname, '..');

const EXPECTED_PLATFORMS = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64',
  'linux-x64',
  'linux-x64-musl',
  'windows-x64',
];

const EXPECTED_FILES = {
  rg: {
    'darwin-arm64': 'runtime/rg/rg-darwin-arm64',
    'darwin-x64': 'runtime/rg/rg-darwin-x64',
    'linux-arm64': 'runtime/rg/rg-linux-arm64',
    'linux-x64': 'runtime/rg/rg-linux-x64',
    'linux-x64-musl': 'runtime/rg/rg-linux-x64-musl',
    'windows-x64': 'runtime/rg/rg-windows-x64.exe',
  },
};

const distDir = resolveDistDir();
const manifestPath = join(distDir, 'runtime-assets.json');
const failures = [];

if (!existsSync(manifestPath)) {
  fail(`Missing runtime asset manifest: ${manifestPath}`);
} else {
  verifyManifest(readManifest(manifestPath));
}

if (failures.length > 0) {
  console.error('Runtime asset verification failed:');
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Runtime asset verification passed for ${EXPECTED_PLATFORMS.length} platforms in ${distDir}.`
);

function resolveDistDir() {
  const distArgIndex = process.argv.indexOf('--dist');
  if (distArgIndex !== -1) {
    const value = process.argv[distArgIndex + 1];
    if (!value) {
      throw new Error('Usage: verify-runtime-assets.mjs [--dist <dir>]');
    }
    return isAbsolute(value) ? value : resolve(process.cwd(), value);
  }
  return join(packageRoot, 'dist');
}

function readManifest(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    fail(`Could not parse ${path}: ${message}`);
    return {};
  }
}

function verifyManifest(manifest) {
  if (manifest.mode !== 'all') {
    fail(
      `runtime-assets.json mode must be "all" before publishing, got ${JSON.stringify(
        manifest.mode
      )}`
    );
  }

  for (const platform of EXPECTED_PLATFORMS) {
    for (const kind of Object.keys(EXPECTED_FILES)) {
      verifyEntry(manifest, kind, platform);
    }
  }

  for (const kind of Object.keys(EXPECTED_FILES)) {
    const entries = Array.isArray(manifest[kind]) ? manifest[kind] : [];
    const unexpected = entries
      .map(entry => entry?.platform)
      .filter(platform => platform && !EXPECTED_PLATFORMS.includes(platform));
    if (unexpected.length > 0) {
      fail(`${kind} has unexpected platforms: ${unexpected.join(', ')}`);
    }
  }
}

function verifyEntry(manifest, kind, platform) {
  const entries = Array.isArray(manifest[kind]) ? manifest[kind] : [];
  const entry = entries.find(item => item?.platform === platform);
  const expectedFile = EXPECTED_FILES[kind][platform];

  if (!entry) {
    fail(`${kind} is missing manifest entry for ${platform}`);
    return;
  }

  if (entry.file !== expectedFile) {
    fail(
      `${kind}/${platform} manifest file mismatch: expected ${expectedFile}, got ${entry.file}`
    );
  }

  const resolvedFile = join(distDir, expectedFile);
  if (!existsSync(resolvedFile)) {
    fail(`${kind}/${platform} file is missing: ${resolvedFile}`);
  }
}

function fail(message) {
  failures.push(message);
}
