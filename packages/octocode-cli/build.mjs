import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { chmodSync, readFileSync, writeFileSync } from 'fs';
import { rm } from 'fs/promises';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));

const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((m) => `node:${m}`),
];

// Every runtime `dependency` MUST stay external — never inlined into the bundle.
// octocode-cli consumes octocode-mcp and octocode-shared through npm so their
// transitive optional native packages are installed by the user's package manager.
const runtimeExternals = Object.keys(pkg.dependencies ?? {});

const external = [...nodeExternals, ...runtimeExternals];

const shimBanner = [
  "import { createRequire as __createRequire } from 'module';",
  "import { fileURLToPath as __fileURLToPath } from 'url';",
  "import { dirname as __dirname_fn } from 'path';",
  'const require = __createRequire(import.meta.url);',
  'const __filename = __fileURLToPath(import.meta.url);',
  'const __dirname = __dirname_fn(__filename);',
].join('\n');

await rm('out', { recursive: true, force: true });

await esbuild.build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'esm',
  outdir: 'out',
  entryNames: 'octocode-cli',
  chunkNames: 'chunks/[name]-[hash]',
  splitting: true,
  minify: true,
  treeShaking: true,
  banner: { js: shimBanner },
  external,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  logLevel: 'info',
});

console.log('✓ esbuild complete');

const cliEntry = resolve(__dirname, 'out', 'octocode-cli.js');
const cliSource = readFileSync(cliEntry, 'utf-8');
writeFileSync(
  cliEntry,
  cliSource.startsWith('#!') ? cliSource : `#!/usr/bin/env node\n${cliSource}`
);
chmodSync(cliEntry, 0o755);
