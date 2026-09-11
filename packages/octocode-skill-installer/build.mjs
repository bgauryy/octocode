#!/usr/bin/env node
import * as esbuild from 'esbuild';
import { execFileSync } from 'node:child_process';
import { rm } from 'node:fs/promises';
import { builtinModules, createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const tscBin = require.resolve('typescript/bin/tsc');

await rm(resolve(packageRoot, 'dist'), { recursive: true, force: true });

await esbuild.build({
  entryPoints: [resolve(packageRoot, 'src/index.ts')],
  outfile: resolve(packageRoot, 'dist/index.js'),
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  external: [
    '@octocodeai/config',
    ...builtinModules,
    ...builtinModules.map(name => `node:${name}`),
  ],
  sourcemap: true,
});

execFileSync(
  process.execPath,
  [tscBin, '--emitDeclarationOnly', '--outDir', resolve(packageRoot, 'dist'), '-p', 'tsconfig.build.json'],
  { cwd: packageRoot, stdio: 'inherit' },
);

console.log('✓ @octocodeai/octocode-skill-installer built → dist/');
