import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  sharedBuildOptions,
  shimBanner,
  entryPoints,
} from './buildConfig.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));

await rm('dist', { recursive: true, force: true });

await Promise.all(
  entryPoints.map((entry) =>
    esbuild.build({
      ...sharedBuildOptions,
      ...entry,
      banner: { js: shimBanner },
    })
  )
);

console.log('✓ esbuild complete');
