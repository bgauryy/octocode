#!/usr/bin/env node
/**
 * bundle-minifier.mjs — copies the octocode-minifier-utils native .node binary
 * for a given platform into the output directory, alongside a compiled Bun binary.
 *
 * Required because Bun --compile binaries are self-contained executables; native
 * .node files cannot be embedded and must be co-located at runtime.
 *
 * Usage:
 *   node scripts/bundle-minifier.mjs <platform> <outDir>
 *
 * Platforms (matching build:bin:* script names):
 *   darwin-arm64 | darwin-x64 | linux-arm64 | linux-x64 | linux-x64-musl | windows-x64
 */

import { chmodSync, copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Map build:bin platform names → octocode-minifier-utils binary triple names
const PLATFORM_MAP = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-arm64': 'linux-arm64-gnu',
  'linux-x64': 'linux-x64-gnu',
  'linux-x64-musl': 'linux-x64-musl',
  'windows-x64': 'win32-x64-msvc',
};

const [platform, outDir] = process.argv.slice(2);

if (!platform || !outDir) {
  console.error('Usage: node scripts/bundle-minifier.mjs <platform> <outDir>');
  console.error('  Platforms:', Object.keys(PLATFORM_MAP).join(', '));
  process.exit(1);
}

const triple = PLATFORM_MAP[platform];
if (!triple) {
  console.error(`Unknown platform: ${platform}`);
  console.error('  Valid platforms:', Object.keys(PLATFORM_MAP).join(', '));
  process.exit(1);
}

const minifierPkg = join(__dirname, '..', '..', 'octocode-minifier-utils');
const binaryName = `octocode-minifier-utils.${triple}.node`;
const src = join(minifierPkg, binaryName);
const dest = join(outDir, binaryName);
const runtimeDest = join(outDir, 'runtime', 'minifier', binaryName);

if (!existsSync(src)) {
  console.warn(`⚠  octocode-minifier-utils binary not found: ${src}`);
  console.warn(`   Run: cd packages/octocode-minifier-utils && yarn build`);
  process.exit(1);
}

copyFileSync(src, dest);
chmodSync(dest, 0o755);
mkdirSync(join(outDir, 'runtime', 'minifier'), { recursive: true });
copyFileSync(src, runtimeDest);
chmodSync(runtimeDest, 0o755);
console.log(`✓ copied ${binaryName} → ${dest}`);
