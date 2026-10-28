// Bundles the launcher (dist/index.js): resolves the native binary and starts
// its MCP server.
import * as esbuild from 'esbuild';
import { chmodSync, readFileSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import {
  assertDeclaredRuntimeImports,
  launcherBuildOptions,
} from '../../skills-dev/octocode-dev/scripts/package-build.mjs';

const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));
const outfile = 'dist/index.js';

await rm('dist', { recursive: true, force: true });
const result = await esbuild.build({
  ...launcherBuildOptions(pkg),
  entryPoints: ['src/index.ts'],
  outfile,
});

assertDeclaredRuntimeImports({
  metafiles: [result.metafile],
  dependencies: pkg.dependencies,
  label: `${pkg.name} bundle`,
});
chmodSync(outfile, 0o755);
console.log('✓ esbuild complete');
