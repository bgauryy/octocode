import { defineConfig } from 'tsdown';
import { builtinModules } from 'module';
import { readFileSync } from 'fs';

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));

export default defineConfig({
  entry: {
    server: 'src/server.ts',
    'server-init': 'src/server-init.ts',
  },
  format: ['esm'],
  outDir: 'scripts',
  clean: true,
  target: 'node20',
  platform: 'node',

  noExternal: [/.*/],

  external: [
    ...builtinModules,
    ...builtinModules.map((m) => `node:${m}`),
  ],

  splitting: false,

  treeshake: true,
  minify: true,
  shims: true,
  dts: true,
  sourcemap: false,

  outExtensions: () => ({ js: '.js' }),

  banner: '#!/usr/bin/env node',

  define: {
    'process.env.NODE_ENV': '"production"',
    '__PACKAGE_VERSION__': JSON.stringify(pkg.version),
  },
});
