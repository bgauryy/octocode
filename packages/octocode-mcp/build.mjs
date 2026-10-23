// Bundles the MCP server (dist/index.js) and its library entry (dist/public.js).
import * as esbuild from 'esbuild';
import { readFileSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import {
  assertDeclaredRuntimeImports,
  packageExternals,
} from '../../skills-dev/octocode-dev/scripts/package-build.mjs';

const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));

const sharedBuildOptions = {
  bundle: true,
  platform: 'node',
  target: 'node24',
  format: 'esm',
  minify: true,
  treeShaking: true,
  // Published runtime dependencies stay external; consumers install them from
  // this package's manifest, preserving the interface boundary.
  external: packageExternals(pkg),
  define: {
    __OCTOCODE_BUNDLED__: 'true',
  },
  // Only the executable marker is needed; runtime dependencies stay external.
  banner: { js: '#!/usr/bin/env node' },
  metafile: true,
  logLevel: 'info',
};

const entryPoints = [
  { entryPoints: ['src/index.ts'], outfile: 'dist/index.js' },
  { entryPoints: ['src/public.ts'], outfile: 'dist/public.js' },
];

await rm('dist', { recursive: true, force: true });
const buildResults = await Promise.all(
  entryPoints.map(entry => esbuild.build({ ...sharedBuildOptions, ...entry }))
);

assertDeclaredRuntimeImports({
  metafiles: buildResults.map(result => result.metafile),
  dependencies: pkg.dependencies,
  label: `${pkg.name} bundle`,
});

console.log('✓ esbuild complete');
