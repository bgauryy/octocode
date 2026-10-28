// Bundles the launcher (out/octocode.js), which starts the native binary, and
// stages the public skills the package ships.
import * as esbuild from 'esbuild';
import { chmodSync, readFileSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  assertDeclaredRuntimeImports,
  launcherBuildOptions,
} from '../../skills-dev/octocode-dev/scripts/package-build.mjs';
import { stageSkills } from './scripts/stage-skills.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));
const outfile = 'out/octocode.js';

await rm('out', { recursive: true, force: true });

stageSkills(resolve(__dirname, '..', '..', 'skills'), resolve(__dirname, 'skills'));
console.log('✓ skills staged → skills/');

const result = await esbuild.build({
  ...launcherBuildOptions(pkg),
  entryPoints: ['src/index.ts'],
  outfile,
});

assertDeclaredRuntimeImports({
  metafiles: [result.metafile],
  dependencies: pkg.dependencies,
  label: `${pkg.name} bundle`,
});
chmodSync(outfile, 0o755);
console.log('✓ esbuild complete');
