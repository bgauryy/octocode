import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { readFileSync } from 'fs';
import { rm } from 'fs/promises';

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));

const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((m) => `node:${m}`),
];

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
  external: nodeExternals,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  logLevel: 'info',
});

console.log('✓ esbuild complete');
