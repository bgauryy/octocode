import * as esbuild from 'esbuild';
import { readFileSync } from 'fs';
import { rm } from 'fs/promises';

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));

await rm('dist', { recursive: true, force: true });

await esbuild.build({
  entryPoints: {
    index: 'src/index.ts',
    'credentials/index': 'src/credentials/index.ts',
    'platform/index': 'src/platform/index.ts',
    'session/index': 'src/session/index.ts',
    'config/index': 'src/config/index.ts',
    'credentials/testing': 'src/credentials/testing.ts',
    paths: 'src/paths.ts',
  },
  bundle: true,
  platform: 'node',
  target: 'node18',
  format: 'esm',
  outdir: 'dist',
  packages: 'external',
  treeShaking: true,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  logLevel: 'info',
});

console.log('✓ esbuild complete');
