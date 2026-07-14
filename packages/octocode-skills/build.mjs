#!/usr/bin/env node
/**
 * Build script for @octocodeai/skills
 *
 * 1. Clean out/
 * 2. Sync skills from ../../skills → skills/ (package root)
 * 3. Bundle CLI and library with esbuild
 */

import * as esbuild from 'esbuild';
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const pkg = JSON.parse(fs.readFileSync(path.join(__dirname, 'package.json'), 'utf-8'));

// ─── Clean ────────────────────────────────────────────────────────────────────

fs.rmSync(path.join(__dirname, 'out'), { recursive: true, force: true });

// ─── Sync skills ──────────────────────────────────────────────────────────────

const monorepoSkillsDir = path.resolve(__dirname, '..', '..', 'skills');
const packageSkillsDir = path.join(__dirname, 'skills');

if (!fs.existsSync(monorepoSkillsDir)) {
  console.warn(`⚠  Skills directory not found at ${monorepoSkillsDir} — skipping skills sync`);
} else {
  syncSkills(monorepoSkillsDir, packageSkillsDir);
  console.log(`✓  Skills synced → skills/`);
}

function syncSkills(src, dest) {
  fs.rmSync(dest, { recursive: true, force: true });
  copyDir(src, dest, { excludeDirs: ['scripts'] });
}

function copyDir(src, dest, opts = {}) {
  fs.mkdirSync(dest, { recursive: true });
  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (const entry of entries) {
    if (opts.excludeDirs?.includes(entry.name)) continue;
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyDir(srcPath, destPath, opts);
    } else if (entry.isFile()) {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

// ─── esbuild ──────────────────────────────────────────────────────────────────

const NODE_BUILTINS = [
  'node:fs', 'node:path', 'node:os', 'node:url', 'node:child_process',
  'node:process', 'node:util',
  'fs', 'path', 'os', 'url', 'child_process', 'process', 'util',
];

const shared = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node20',
  external: NODE_BUILTINS,
  sourcemap: false,
  minify: false,
};

await esbuild.build({
  ...shared,
  entryPoints: ['src/cli.ts'],
  outfile: 'out/cli.js',
  banner: { js: '#!/usr/bin/env node' },
  define: { __PKG_VERSION__: JSON.stringify(pkg.version) },
});

await esbuild.build({
  ...shared,
  entryPoints: ['src/index.ts'],
  outfile: 'out/index.js',
});

// Make CLI executable
fs.chmodSync(path.join(__dirname, 'out', 'cli.js'), 0o755);

// ─── TypeScript declarations ──────────────────────────────────────────────────
// esbuild handles JS; tsc adds .d.ts so TypeScript consumers get full types.

try {
  execSync('npx tsc --project tsconfig.json --emitDeclarationOnly', {
    cwd: __dirname,
    stdio: 'inherit',
  });
  console.log('✓  TypeScript declarations emitted → out/');
} catch {
  console.warn('⚠  tsc --emitDeclarationOnly failed — declarations skipped (publish will lack types)');
}

console.log('✓  @octocodeai/skills built → out/');
