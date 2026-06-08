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

// Every runtime `dependency` MUST stay external — never inlined into the bundle.
// npm/yarn/pnpm install these into the consumer's node_modules, so the published
// dist/ should `require()` them at runtime rather than embed a copy. This is
// critical for packages that locate sibling files relative to their OWN install
// directory — most importantly `@vscode/ripgrep`, which does
// `require.resolve('@vscode/ripgrep-<os>-<arch>/bin/rg')` against a
// platform-specific optionalDependency. If esbuild inlines that resolver, the
// resolution origin shifts to dist/index.js and breaks under strict node_modules
// layouts (pnpm) where the binary package isn't hoisted next to us.
//
// Deriving this list from package.json `dependencies` (rather than hardcoding)
// guarantees it can never drift when deps are added or removed.
//
// NOTE: workspace packages (octocode-shared, octocode-security-utils) live in
// `devDependencies`, so they are deliberately NOT external — they get bundled,
// because consumers never install them.
export const runtimeExternals = Object.keys(pkg.dependencies ?? {});

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
