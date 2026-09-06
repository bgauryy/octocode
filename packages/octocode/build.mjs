import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { chmodSync, readFileSync, writeFileSync } from 'fs';
import { rm } from 'fs/promises';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { assertDeclaredRuntimeImports } from '../../scripts/runtime-import-contract.mjs';
import { stageSkills } from './scripts/stage-skills.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));
const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map(m => `node:${m}`),
];

// Published runtime dependencies stay external. This keeps the CLI a true
// interface package and leaves tools-core responsible for its own dependency
// graph instead of partially inlining it into this bundle.
const runtimeExternals = Object.keys(pkg.dependencies ?? {});

const external = [...nodeExternals, ...runtimeExternals];

await rm('out', { recursive: true, force: true });

const monorepoSkillsDir = resolve(__dirname, '..', '..', 'skills');
const packageSkillsDir = resolve(__dirname, 'skills');
stageSkills(monorepoSkillsDir, packageSkillsDir);
console.log('✓ skills staged → skills/');

const buildResult = await esbuild.build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'esm',
  outdir: 'out',
  entryNames: 'octocode',
  chunkNames: 'chunks/[name]-[hash]',
  splitting: true,
  minify: true,
  treeShaking: true,
  metafile: true,
  external,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    'process.env.NODE_ENV': '"production"',
  },
  logLevel: 'info',
});

assertDeclaredRuntimeImports({
  metafiles: [buildResult.metafile],
  dependencies: pkg.dependencies,
  label: 'CLI bundle',
});

console.log('✓ esbuild complete');

const cliEntry = resolve(__dirname, 'out', 'octocode.js');
const cliSource = readFileSync(cliEntry, 'utf-8');
writeFileSync(
  cliEntry,
  cliSource.startsWith('#!') ? cliSource : `#!/usr/bin/env node\n${cliSource}`
);
chmodSync(cliEntry, 0o755);
