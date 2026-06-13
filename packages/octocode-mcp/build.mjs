import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import {
  sharedBuildOptions,
  shimBanner,
  entryPoints,
} from './buildConfig.mjs';

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

await import('./scripts/bundle-runtime-assets.mjs');
