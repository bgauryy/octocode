import * as esbuild from 'esbuild';
import { chmodSync, readFileSync, writeFileSync } from 'fs';
import { rm } from 'fs/promises';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import {
  assertDeclaredRuntimeImports,
  packageExternals,
} from '../../skills-dev/octocode-dev/scripts/package-build.mjs';
import { stageSkills } from './scripts/stage-skills.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));
// Published runtime dependencies stay external: the CLI is an interface
// package, not a partial copy of the native and shared packages.
const external = packageExternals(pkg);

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
  define: {
    __OCTOCODE_BUNDLED__: 'true',
  },
  // The bundled native scripts are CommonJS: give them `require`, resolved
  // from this package, so they find the installed per-platform package.
  banner: {
    js: "import { createRequire as __octocodeRequire } from 'node:module';\nconst require = __octocodeRequire(import.meta.url);",
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
