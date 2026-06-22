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
  // Engine-free schema/help surface (P3): bundled independently so it carries no
  // @octocodeai/octocode-engine import and loads no native addon at eval.
  { entryPoints: ['src/schema.ts'], outfile: 'dist/schema.js' },
  { entryPoints: ['src/oql/index.ts'], outfile: 'dist/oql/index.js' },
  { entryPoints: ['src/zod.ts'], outfile: 'dist/zod.js' },
  { entryPoints: ['src/shared/credentials/index.ts'], outfile: 'dist/shared/credentials/index.js' },
  { entryPoints: ['src/shared/platform/index.ts'], outfile: 'dist/shared/platform/index.js' },
  { entryPoints: ['src/shared/session/index.ts'], outfile: 'dist/shared/session/index.js' },
  { entryPoints: ['src/shared/config/index.ts'], outfile: 'dist/shared/config/index.js' },
  { entryPoints: ['src/shared/paths.ts'], outfile: 'dist/shared/paths.js' },
  { entryPoints: ['src/shared/fs-utils.ts'], outfile: 'dist/shared/fs-utils.js' },
  { entryPoints: ['src/shared/credentials/testing.ts'], outfile: 'dist/shared/credentials/testing.js' },
];
