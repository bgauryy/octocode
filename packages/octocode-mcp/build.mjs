import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { rm } from 'fs/promises';

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

const shared = {
  bundle: true,
  platform: 'node',
  target: 'node18',
  format: 'esm',
  minify: true,
  treeShaking: true,
  external: nodeExternals,
  loader: { '.md': 'text' },
  define: {
    'process.env.NODE_ENV': '"production"',
  },
  logLevel: 'info',
};

await rm('dist', { recursive: true, force: true });

await Promise.all([
  esbuild.build({
    ...shared,
    entryPoints: ['src/index.ts'],
    outfile: 'dist/index.js',
    banner: { js: shimBanner },
  }),
  esbuild.build({
    ...shared,
    entryPoints: ['src/public.ts'],
    outfile: 'dist/public.js',
    banner: { js: shimBanner },
  }),
]);

console.log('✓ esbuild complete');
