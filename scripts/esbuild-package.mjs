import * as esbuild from 'esbuild';
import { readFile, rm } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { assertDeclaredRuntimeImports } from './runtime-import-contract.mjs';

const configUrl = pathToFileURL(resolve(process.cwd(), 'buildConfig.mjs')).href;
const { entryPoints, sharedBuildOptions, shimBanner } = await import(configUrl);
const pkg = JSON.parse(
  await readFile(resolve(process.cwd(), 'package.json'), 'utf8')
);

await rm('dist', { recursive: true, force: true });
const buildResults = await Promise.all(
  entryPoints.map(entry =>
    esbuild.build({
      ...sharedBuildOptions,
      ...entry,
      banner: { js: shimBanner },
      metafile: true,
    })
  )
);

assertDeclaredRuntimeImports({
  metafiles: buildResults.map(result => result.metafile),
  dependencies: pkg.dependencies,
  label: `${pkg.name} bundle`,
});

console.log('✓ esbuild complete');
