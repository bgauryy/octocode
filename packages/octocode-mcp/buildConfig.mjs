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

// Most runtime `dependency` entries MUST stay external — never inlined into the bundle.
// npm/yarn/pnpm install these into the consumer's node_modules, so the published
// dist/ should `require()` them at runtime rather than embed a copy. This is
// critical for SDKs and other normal runtime libraries that consumers should
// receive through package-manager dependency resolution.
//
// Deriving this list from package.json `dependencies` (rather than hardcoding)
// guarantees it can never drift when deps are added or removed.
//
//
// All runtime dependencies are external — installed via npm and loaded at runtime.
// octocode-security and @octocodeai/octocode-context-utils ship their own
// per-platform optionalDependencies; npm installs the right .node for each user.
export const bundledRuntimeDependencies = new Set([]);

export const runtimeExternals = Object.keys(pkg.dependencies ?? {}).filter(
  (dependencyName) => !bundledRuntimeDependencies.has(dependencyName)
);

export const external = [...nodeExternals, ...runtimeExternals];

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
