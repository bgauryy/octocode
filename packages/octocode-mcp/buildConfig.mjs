// Pure, side-effect-free build configuration consumed by the shared package runner.
// Importing this module must NOT trigger a build — it only computes config.
import { createRequire } from 'node:module';
import { packageExternals } from '../../skills-dev/octocode-dev/scripts/package-build.mjs';

const require = createRequire(import.meta.url);
const pkg = require('./package.json');

// Only the executable marker is needed; runtime dependencies stay external.
export const shimBanner = '#!/usr/bin/env node';

export const sharedBuildOptions = {
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
  logLevel: 'info',
};

export const entryPoints = [
  { entryPoints: ['src/index.ts'], outfile: 'dist/index.js' },
  { entryPoints: ['src/public.ts'], outfile: 'dist/public.js' },
];
