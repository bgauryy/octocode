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
// install them from this package's manifest; native and shared-core dependencies
// are external for the same reason.
export const bundledRuntimeDependencies = new Set([]);

export const runtimeExternals = Object.keys(pkg.dependencies ?? {}).filter(
  dependencyName => !bundledRuntimeDependencies.has(dependencyName)
);

// Subpath-export wildcards for the externalized packages (esbuild matches `*`).
// The base specifiers are already covered by runtimeExternals above; these keep
// deep imports (e.g. `@octocodeai/octocode-core/types`) external too.
export const transitiveExternals = [
  '@octocodeai/octocode-engine/*',
  '@octocodeai/octocode-core/*',
  '@modelcontextprotocol/server/*',
  '@octokit/*',
];

export const external = [
  ...nodeExternals,
  ...runtimeExternals,
  ...transitiveExternals,
];

// ESM interop shim: provides require/__filename/__dirname inside the ESM bundle.
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
  { entryPoints: ['src/public.ts'], outfile: 'dist/public.js' },
];
