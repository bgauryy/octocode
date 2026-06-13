import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import {
  chmodSync,
  cpSync,
  existsSync,
  linkSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from 'fs';
import { rm } from 'fs/promises';
import { resolve, dirname, join } from 'path';

/**
 * Recursively copy a directory using hardlinks where possible.
 * Hardlinked files share one inode → zero extra disk usage in the monorepo.
 * Falls back to a regular copy on cross-device moves or other OS errors.
 */
function hardlinkDirSync(src, dest) {
  mkdirSync(dest, { recursive: true });
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const srcPath = join(src, entry.name);
    const destPath = join(dest, entry.name);
    if (entry.isDirectory()) {
      hardlinkDirSync(srcPath, destPath);
    } else {
      try {
        if (existsSync(destPath)) unlinkSync(destPath);
        linkSync(srcPath, destPath);
      } catch {
        cpSync(srcPath, destPath);
      }
    }
  }
}
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

hardlinkDirSync(mcpRuntime, resolve(__dirname, 'out', 'runtime'));

const mcpRuntimeManifest = resolve(mcpDist, 'runtime-assets.json');
if (existsSync(mcpRuntimeManifest)) {
  cpSync(mcpRuntimeManifest, resolve(__dirname, 'out', 'runtime-assets.json'));
}

console.log('✓ copied octocode-mcp runtime assets');
