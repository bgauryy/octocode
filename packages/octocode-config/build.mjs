#!/usr/bin/env node
/**
 * Build script for @octocodeai/config.
 * Uses esbuild (fast) for JS output + tsc --emitDeclarationOnly for .d.ts files.
 *
 * The published package has zero runtime dependencies; build-only validation
 * is completed before esbuild emits the self-contained runtime.
 * esbuild produces a self-contained ESM file for each entry point.
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
  target: 'node24',
  // Mark all Node built-ins as external — no need to bundle them.
  external: ['node:fs', 'node:os', 'node:path', 'node:process', 'fs', 'os', 'path'],
  sourcemap: true,
};

// Library entry: imported by Octocode packages.
await esbuild.build({
  ...shared,
  entryPoints: ['src/index.ts'],
  outfile: 'dist/index.js',
});

// Contract hub entries: thin re-exports of @octocodeai/octocode-core so every
// surface imports contracts from "@octocodeai/config/{schema,mcp}". Core (and
// its Zod dependency) are marked external — never bundled here — so the source
// of truth stays in octocode-core and the zero-dep dist/index.js is untouched.
await esbuild.build({
  ...shared,
  external: [...shared.external, '@octocodeai/octocode-core', '@octocodeai/octocode-core/*', 'zod'],
  entryPoints: ['src/contracts/schema.ts', 'src/contracts/mcp.ts'],
  outdir: 'dist/contracts',
});

// Generate TypeScript declarations (uses workspace tsc, not global PATH).
execSync(`${tscBin} --emitDeclarationOnly --outDir dist -p tsconfig.build.json`, { stdio: 'inherit', cwd: __dirname });

console.log('✓ @octocodeai/config built → dist/');
