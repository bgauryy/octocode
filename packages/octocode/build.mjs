import * as esbuild from 'esbuild';
import { chmodSync, readFileSync, writeFileSync } from 'fs';
import { rm } from 'fs/promises';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { assertDeclaredRuntimeImports } from '../../skills-dev/octocode-dev/scripts/runtime-import-contract.mjs';
import { packageExternals } from '../../skills-dev/octocode-dev/scripts/package-build.mjs';
import { stageSkills } from './scripts/stage-skills.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));
// Published runtime dependencies stay external: the CLI is an interface
// package, not a partial copy of the native and shared packages.
const external = packageExternals(pkg);

// Vite's `?raw` convention, so tests and the bundle share one asset import:
// config-view browser assets ship inlined as strings.
const rawText = {
  name: 'raw-text',
  setup(build) {
    build.onResolve({ filter: /\?raw$/ }, args => ({
      path: resolve(args.resolveDir, args.path.slice(0, -'?raw'.length)),
      namespace: 'raw-text',
    }));
    build.onLoad({ filter: /.*/, namespace: 'raw-text' }, args => ({
      contents: readFileSync(args.path, 'utf8'),
      loader: 'text',
    }));
  },
};

await rm('out', { recursive: true, force: true });

const monorepoSkillsDir = resolve(__dirname, '..', '..', 'skills');
const packageSkillsDir = resolve(__dirname, 'skills');
stageSkills(monorepoSkillsDir, packageSkillsDir);
console.log('✓ skills staged → skills/');

const buildResult = await esbuild.build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  platform: 'node',
  target: 'node24',
  format: 'esm',
  outdir: 'out',
  entryNames: 'octocode',
  chunkNames: 'chunks/[name]-[hash]',
  splitting: true,
  minify: true,
  treeShaking: true,
  metafile: true,
  external,
  plugins: [rawText],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __OCTOCODE_BUNDLED__: 'true',
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
