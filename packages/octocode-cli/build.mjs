import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { readFileSync } from 'fs';
import { rm } from 'fs/promises';

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));

const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((m) => `node:${m}`),
];

// Every runtime `dependency` MUST stay external — never inlined into the bundle.
// They are installed in the consumer's node_modules, so dist/ should `require()`
// them at runtime. This is critical for `@vscode/ripgrep`, which resolves its
// platform binary via `require.resolve('@vscode/ripgrep-<os>-<arch>/bin/rg')`
// relative to its OWN install dir — inlining that resolver breaks it under pnpm.
// Workspace packages (octocode-mcp, octocode-shared) live in devDependencies and
// are deliberately bundled, because consumers never install them.
// Derived from package.json so it can never drift.
const runtimeExternals = Object.keys(pkg.dependencies ?? {});

const external = [...nodeExternals, ...runtimeExternals];

const shimBanner = [
  '#!/usr/bin/env node',
  "import { createRequire as __createRequire } from 'module';",
  "import { fileURLToPath as __fileURLToPath } from 'url';",
  "import { dirname as __dirname_fn } from 'path';",
  'const require = __createRequire(import.meta.url);',
  'const __filename = __fileURLToPath(import.meta.url);',
  'const __dirname = __dirname_fn(__filename);',
].join('\n');

await rm('out', { recursive: true, force: true });

await esbuild.build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'esm',
  outdir: 'out',
  entryNames: 'octocode-cli',
  chunkNames: 'chunks/[name]-[hash]',
  splitting: true,
  minify: true,
  treeShaking: true,
  banner: { js: shimBanner },
  external,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  logLevel: 'info',
});

console.log('✓ esbuild complete');
