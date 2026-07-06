#!/usr/bin/env node
/**
 * Build script for @octocodeai/octocode-awareness.
 * esbuild for JS output + tsc --emitDeclarationOnly for .d.ts files.
 *
 * Outputs:
 *   dist/index.js          — library entry (imported by pi-extension, etc.)
 *   dist/bin/awareness.js  — standalone CLI (called by hook scripts)
 *   dist/bin/extract-hook-files.js — hook file-path extractor
 *   dist/bin/hook-runner.js — shared hook implementation
 */

import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import { cpSync, copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const tscBin = resolve(__dirname, '../../node_modules/.bin/tsc');

await rm('dist', { recursive: true, force: true });

const shared = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  // Mark all Node built-ins as external — never bundle them.
  external: [
    'node:sqlite', 'node:fs', 'node:os', 'node:path', 'node:crypto',
    'node:child_process', 'node:url', 'node:module',
    'sqlite', 'fs', 'os', 'path', 'crypto', 'child_process',
  ],
  sourcemap: true,
};

// Library: imported by pi-extension and other consumers.
await esbuild.build({
  ...shared,
  entryPoints: ['src/index.ts'],
  outfile: 'dist/index.js',
});

// CLI entry: called by hook scripts as `node dist/bin/awareness.js <command>`.
await esbuild.build({
  ...shared,
  entryPoints: ['bin/awareness.ts'],
  outfile: 'dist/bin/awareness.js',
  banner: { js: '#!/usr/bin/env node' },
});

// Hook helper: `node dist/bin/extract-hook-files.js` reads JSON from stdin.
await esbuild.build({
  ...shared,
  entryPoints: ['bin/extract-hook-files.ts'],
  outfile: 'dist/bin/extract-hook-files.js',
  banner: { js: '#!/usr/bin/env node' },
});

// Hook runner: all lifecycle hook logic shared by thin shell wrappers.
await esbuild.build({
  ...shared,
  entryPoints: ['bin/hook-runner.ts'],
  outfile: 'dist/bin/hook-runner.js',
  banner: { js: '#!/usr/bin/env node' },
});

// Generate TypeScript declarations.
execSync(`${tscBin} --emitDeclarationOnly --outDir dist -p tsconfig.build.json`, {
  stdio: 'inherit',
  cwd: __dirname,
});

console.log('✓ @octocodeai/octocode-awareness built → dist/');

// ─── Sync octocode-awareness skill ─────────────────────────────────────────
// The complete octocode-awareness skill is owned by this package. The repo-root
// skills/octocode-awareness directory is generated from packages/octocode-awareness/skills/.

const repoRoot    = resolve(__dirname, '../..');
const skillDest   = join(repoRoot, 'skills', 'octocode-awareness');
const skillSrc    = join(__dirname, 'skills', 'octocode-awareness');
const scriptDest  = join(skillDest, 'scripts');
const packageScriptDest = join(skillSrc, 'scripts');
const distBin     = join(__dirname, 'dist', 'bin');

// Wipe and rebuild so removed files don't linger.
rmSync(skillDest, { recursive: true, force: true });
mkdirSync(skillDest, { recursive: true });

// 1. Canonical skill docs/references/scripts.
cpSync(skillSrc, skillDest, {
  recursive: true,
  filter: (src) => !src.includes('node_modules'),
});
mkdirSync(scriptDest, { recursive: true });
mkdirSync(packageScriptDest, { recursive: true });

// 2. Compiled CLI — the ONLY awareness binary all platforms share.
for (const dest of [scriptDest, packageScriptDest]) {
  copyFileSync(join(distBin, 'awareness.js'),          join(dest, 'awareness.mjs'));
  copyFileSync(join(distBin, 'extract-hook-files.js'), join(dest, 'extract-hook-files.mjs'));
  copyFileSync(join(distBin, 'hook-runner.js'),        join(dest, 'hook-runner.mjs'));
}

console.log('✓ skills/octocode-awareness/ synced from packages/octocode-awareness/skills/ + dist/bin/');
