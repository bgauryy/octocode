// Pure, side-effect-free build configuration shared by build.mjs and its tests.
// Importing this module must NOT trigger a build — it only computes config.
import { builtinModules } from 'node:module';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const pkg = require('./package.json');

// Node core modules are always external.
export const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((m) => `node:${m}`),
];

// @octocodeai/octocode-tools-core is INLINED into this bundle — it is a
// build-time (dev) dependency, never published to npm. esbuild bundles its
// first-party code precisely because it is NOT listed in `external`.
//
// tools-core's own runtime deps cannot be inlined and must stay external:
//   • @octocodeai/octocode-engine ships a native .node addon (unbundlable)
//   • the rest are registry packages resolved by npm at install time
// All of them are declared in octocode-mcp's own `dependencies`, so a consumer
// `npm install octocode-mcp` pulls them in directly (no tools-core hop). Every
// runtime `dependency` therefore stays external.
export const bundledRuntimeDependencies = new Set([]);

export const runtimeExternals = Object.keys(pkg.dependencies ?? {}).filter(
  (dependencyName) => !bundledRuntimeDependencies.has(dependencyName)
);

// Subpath-export wildcards for the externalized packages (esbuild matches `*`).
// The base specifiers are already covered by runtimeExternals above; these keep
// deep imports (e.g. `@octocodeai/octocode-core/schemas`) external too.
export const transitiveExternals = [
  '@octocodeai/octocode-engine/*',
  '@octocodeai/octocode-core/*',
  '@modelcontextprotocol/sdk/*',
  '@octokit/*',
];

export const external = [...nodeExternals, ...runtimeExternals, ...transitiveExternals];

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
