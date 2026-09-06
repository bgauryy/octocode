#!/usr/bin/env node
/**
 * @octocodeai/agent-contracts build script.
 * esbuild bundles JS → out/; tsc emits .d.ts → out/ (same directory, matches exports).
 */
import { build } from 'esbuild';
import { execSync } from 'node:child_process';
import { rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { baseOptions } from '../../../build.config.mjs';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outDir      = join(packageRoot, 'out');
const repoRoot    = resolve(packageRoot, '../..');
const tscBin      = resolve(repoRoot, 'node_modules/.bin/tsc');

await rm(outDir, { recursive: true, force: true });

await build({
  ...baseOptions,
  // node:sqlite is a native built-in; must stay external.
  external: [...baseOptions.external, 'node:sqlite', 'yaml'],
  entryPoints: [
    'src/index.ts',
    'src/paths.ts',
    'src/db.ts',
    'src/schema.ts',
    'src/mcp-state.ts',
    'src/sqlite.ts',
    'src/sqlite-version.ts',
    'src/embed.ts',
    'src/entities.ts',
    'src/permissions.ts',
    'src/protocols.ts',
    'src/agent-skills.ts',
    'src/prompts/index.ts',
  ].map((p) => join(packageRoot, p)),
  outdir: outDir,
  minify: false,
});

// Type declarations: emitted alongside the JS so package.json exports resolve.
execSync(
  `${JSON.stringify(tscBin)} -p ${JSON.stringify(join(packageRoot, 'tsconfig.json'))} --emitDeclarationOnly`,
  { cwd: packageRoot, stdio: 'inherit' },
);
