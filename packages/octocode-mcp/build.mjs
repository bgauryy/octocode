import * as esbuild from 'esbuild';
import { spawnSync } from 'node:child_process';
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

const assetsResult = spawnSync(process.execPath, ['scripts/bundle-runtime-assets.mjs'], {
  cwd: __dirname,
  stdio: 'inherit',
});

if (assetsResult.status !== 0) {
  process.exit(assetsResult.status ?? 1);
}
