import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const configUrl = pathToFileURL(resolve(process.cwd(), 'buildConfig.mjs')).href;
const { entryPoints, sharedBuildOptions, shimBanner } = await import(configUrl);

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
