#!/usr/bin/env node
/**
 * Build script for @octocodeai/config.
 * Uses esbuild (fast) for JS output + tsc --emitDeclarationOnly for .d.ts files.
 *
 * The package has zero external dependencies; only Node built-ins are used.
 * esbuild produces a self-contained ESM file for each entry point, which is
 * then injected by package builds into each skill's scripts/ dir
 * as octocode-config.mjs for standalone use without npm.
 */

import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import { execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
// Resolve tsc from the workspace root node_modules, not global PATH.
const tscBin = resolve(__dirname, '../../node_modules/.bin/tsc');

await rm('dist', { recursive: true, force: true });

const shared = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node20',
  // Mark all Node built-ins as external — no need to bundle them.
  external: ['node:fs', 'node:os', 'node:path', 'node:process', 'fs', 'os', 'path'],
  sourcemap: true,
};

// Library entry: imported by all Octocode surfaces and injected into skills.
await esbuild.build({
  ...shared,
  entryPoints: ['src/index.ts'],
  outfile: 'dist/index.js',
});

// CLI entry: the `octocode-config` binary (npx @octocodeai/config).
await esbuild.build({
  ...shared,
  entryPoints: ['src/cli.ts'],
  outfile: 'dist/cli.js',
  banner: { js: '#!/usr/bin/env node' },
});

// Generate TypeScript declarations (uses workspace tsc, not global PATH).
execSync(`${tscBin} --emitDeclarationOnly --outDir dist -p tsconfig.build.json`, { stdio: 'inherit', cwd: __dirname });

console.log('✓ @octocodeai/config built → dist/');
