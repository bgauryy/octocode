// Pure, side-effect-free build configuration shared by build.mjs and its tests.
import { builtinModules } from 'node:module';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const pkg = require('./package.json');

export const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((m) => `node:${m}`),
];

export const bundledRuntimeDependencies = new Set([]);

export const runtimeExternals = Object.keys(pkg.dependencies ?? {}).filter(
  (dependencyName) => !bundledRuntimeDependencies.has(dependencyName)
);

export const external = [...nodeExternals, ...runtimeExternals];

export const shimBanner = [
  '#!/usr/bin/env node',
  "import { createRequire as __createRequire } from 'module';",
  "import { fileURLToPath as __fileURLToPath } from 'url';",
  "import { dirname as __dirname_fn } from 'path';",
  'const require = __createRequire(import.meta.url);',
  'const __filename = __fileURLToPath(import.meta.url);',
  'const __dirname = __dirname_fn(__filename);',
].join('\n');

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
  { entryPoints: ['src/direct.ts'], outfile: 'dist/direct.js' },
  { entryPoints: ['src/zod.ts'], outfile: 'dist/zod.js' },
];
