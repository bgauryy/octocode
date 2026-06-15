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

// Runtime `dependency` entries stay external — installed by npm and loaded at
// runtime rather than inlined into the bundle. mcp's only runtime deps are
// @modelcontextprotocol/sdk and @octocodeai/octocode-tools-core.
//
// All other packages (octocode-security, octocode-shared, zod, …) are owned
// by tools-core and declared in transitiveExternals below.
export const bundledRuntimeDependencies = new Set([]);

export const runtimeExternals = Object.keys(pkg.dependencies ?? {}).filter(
  (dependencyName) => !bundledRuntimeDependencies.has(dependencyName)
);

// Packages directly imported in mcp/src that are NOT in mcp's package.json
// dependencies because ownership belongs to @octocodeai/octocode-tools-core.
// They install transitively when a consumer installs tools-core; we just need
// to tell esbuild not to bundle them (native .node modules cannot be bundled).
export const transitiveExternals = [
  // octocode-security ships native .node binaries
  'octocode-security',
  'octocode-security/mask',
  'octocode-security/withSecurityValidation',
  // octocode-shared is a plain TS library, but owned by tools-core
  'octocode-shared',
  // @octocodeai/octocode-core — schemas, types, extra-types, outputs subpaths
  '@octocodeai/octocode-core',
  '@octocodeai/octocode-core/schemas',
  '@octocodeai/octocode-core/schemas/outputs',
  '@octocodeai/octocode-core/types',
  '@octocodeai/octocode-core/extra-types',
  // zod — owned by tools-core; mcp uses it for MCP-layer schema fragments
  'zod',
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
