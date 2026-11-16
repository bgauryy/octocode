// Bundles the launcher (out/octocode.js), which starts the native binary, and
// the postinstall installer (out/install.js); writes the bin stub that starts
// the launcher; stages the public skills the package ships.
import * as esbuild from 'esbuild';
import { chmodSync, copyFileSync, readFileSync } from 'node:fs';
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

const bundles = await Promise.all(
  [
    ['src/index.ts', outfile],
    ['src/install.ts', 'out/install.js'],
  ].map(([entry, out]) =>
    esbuild.build({ ...launcherBuildOptions(pkg), entryPoints: [entry], outfile: out })
  )
);

assertDeclaredRuntimeImports({
  metafiles: bundles.map(result => result.metafile),
  dependencies: pkg.dependencies,
  label: `${pkg.name} bundle`,
});
chmodSync(outfile, 0o755);
// The bin: a stub that imports the launcher, until postinstall replaces it.
copyFileSync('src/bin.mjs', pkg.bin);
chmodSync(pkg.bin, 0o755);
console.log('✓ esbuild complete');
