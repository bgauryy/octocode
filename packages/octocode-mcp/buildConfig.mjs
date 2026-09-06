// Pure, side-effect-free build configuration consumed by the shared package runner and tests.
// Importing this module must NOT trigger a build — it only computes config.
import { builtinModules } from 'node:module';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const pkg = require('./package.json');

// Node core modules are always external.
export const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map(m => `node:${m}`),
];

// Published runtime dependencies stay external, including tools-core. Consumers
// install them from this package's manifest, preserving the interface → core
// package boundary in the emitted bundle.
export const runtimeExternals = Object.keys(pkg.dependencies ?? {});

export const external = [...nodeExternals, ...runtimeExternals];

// Only the executable marker is needed; runtime dependencies stay external.
export const shimBanner = '#!/usr/bin/env node';

export const sharedBuildOptions = {
  bundle: true,
  platform: 'node',
  target: 'node18',
  format: 'esm',
  minify: true,
  treeShaking: true,
  external,
  loader: { '.md': 'text' },
  define: {
    'process.env.NODE_ENV': '"production"',
  },
  logLevel: 'info',
};

export const entryPoints = [
  { entryPoints: ['src/index.ts'], outfile: 'dist/index.js' },
  { entryPoints: ['src/public.ts'], outfile: 'dist/public.js' },
];
