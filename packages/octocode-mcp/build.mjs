import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import { existsSync, copyFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
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

// Copy octocode-security native binaries next to the package root.
// The bundled loadNative() resolves binaries relative to dist/../ (= package root),
// so the .node files must live in packages/octocode-mcp/ for tests (and any direct
// dist/public.js import) to work without a compiled binary in node_modules.
const securityPkg = resolve(__dirname, '..', 'octocode-security');
const platforms = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64-gnu',
  'linux-x64-gnu',
  'win32-x64-msvc',
];
for (const platform of platforms) {
  const src = resolve(securityPkg, `octocode-security.${platform}.node`);
  if (existsSync(src)) {
    const dest = resolve(__dirname, `octocode-security.${platform}.node`);
    copyFileSync(src, dest);
    console.log(`✓ copied octocode-security.${platform}.node`);
  }
}
