import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { cpSync, existsSync, readFileSync } from 'fs';
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
// Workspace packages (octocode-mcp, octocode-shared) live in devDependencies and
// are deliberately bundled, because consumers never install them.
// Derived from package.json so it can never drift.
const runtimeExternals = Object.keys(pkg.dependencies ?? {});

const external = [...nodeExternals, ...runtimeExternals];

const shimBanner = [
  '#!/usr/bin/env node',
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

// octocode-cli bundles octocode-mcp's JS, then copies MCP's runtime assets as a
// single unit. The CLI should not know about octocode-security or rg internals.
const mcpDist = resolve(__dirname, '..', 'octocode-mcp', 'dist');
const mcpRuntime = resolve(mcpDist, 'runtime');
if (!existsSync(mcpRuntime)) {
  throw new Error(
    `Missing octocode-mcp runtime assets at ${mcpRuntime}. ` +
      'Build octocode-mcp before octocode-cli.'
  );
}

cpSync(mcpRuntime, resolve(__dirname, 'out', 'runtime'), {
  recursive: true,
});

const mcpRuntimeManifest = resolve(mcpDist, 'runtime-assets.json');
if (existsSync(mcpRuntimeManifest)) {
  cpSync(mcpRuntimeManifest, resolve(__dirname, 'out', 'runtime-assets.json'));
}

console.log('✓ copied octocode-mcp runtime assets');
