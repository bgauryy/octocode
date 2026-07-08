import * as esbuild from 'esbuild';
import { builtinModules } from 'module';
import { chmodSync, readFileSync, writeFileSync } from 'fs';
import { cp, rm } from 'fs/promises';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { entryPoints as toolsCoreEntryPoints } from '../octocode-tools-core/buildConfig.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkg = JSON.parse(readFileSync('./package.json', 'utf-8'));
const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((m) => `node:${m}`),
];

// Every runtime `dependency` MUST stay external — never inlined into the bundle.
// @octocodeai/octocode-tools-core is deliberately NOT a runtime dependency: it
// is a build-time (dev) dependency that esbuild INLINES into out/octocode.js, so
// it is never published to npm. Its own runtime deps (engine native addon, core,
// octokit, …) stay external and are declared in this package's `dependencies`.
const runtimeExternals = Object.keys(pkg.dependencies ?? {});

// Subpath-export wildcards for the externalized packages tools-core pulls in
// (esbuild matches `*`). Base specifiers are covered by runtimeExternals; the
// native engine addon in particular can never be bundled.
const transitiveExternals = [
  '@octocodeai/octocode-engine',
  '@octocodeai/octocode-engine/*',
  '@octocodeai/octocode-core',
  '@octocodeai/octocode-core/*',
  '@modelcontextprotocol/sdk',
  '@modelcontextprotocol/sdk/*',
  '@octokit/*',
  'zod',
];

const external = [...nodeExternals, ...runtimeExternals, ...transitiveExternals];

// @octocodeai/octocode-tools-core is inlined from SOURCE, not from its published
// dist/. Resolving every specifier (base + subpaths) to the src entry files means
// esbuild's code-splitting shares tools-core's internal modules across the whole
// CLI bundle instead of pulling N pre-bundled dist entries that each re-inline the
// shared code. This kills the split-brain duplication AND the stale-dist trap:
// `yarn workspace octocode build` no longer depends on tools-core's dist being
// rebuilt first, so the shipped code path can never diverge from what's greppable.
//
// The specifier→src map is DERIVED from tools-core's own build config + exports map
// (never hardcoded), so it can't drift as subpaths are added/renamed there.
const toolsCoreDir = resolve(__dirname, '..', 'octocode-tools-core');
const toolsCorePkg = JSON.parse(
  readFileSync(resolve(toolsCoreDir, 'package.json'), 'utf-8')
);
const TOOLS_CORE = toolsCorePkg.name;

// dist outfile ('dist/direct.js') -> src entry ('src/direct.ts'), per tools-core's build.
const distOutfileToSrcEntry = new Map(
  toolsCoreEntryPoints.map((entry) => {
    const srcEntry = Array.isArray(entry.entryPoints)
      ? entry.entryPoints[0]
      : entry.entryPoints;
    return [entry.outfile.replace(/^\.?\//, ''), srcEntry];
  })
);

// import specifier ('@octocodeai/octocode-tools-core/direct') -> absolute src file.
const toolsCoreSpecifierToSrc = new Map();
for (const [subpath, target] of Object.entries(toolsCorePkg.exports ?? {})) {
  const importTarget = typeof target === 'string' ? target : target?.import;
  if (!importTarget) continue;
  const srcEntry = distOutfileToSrcEntry.get(importTarget.replace(/^\.?\//, ''));
  if (!srcEntry) continue;
  const specifier =
    subpath === '.'
      ? TOOLS_CORE
      : `${TOOLS_CORE}/${subpath.replace(/^\.\//, '')}`;
  toolsCoreSpecifierToSrc.set(specifier, resolve(toolsCoreDir, srcEntry));
}

const inlineToolsCoreFromSource = {
  name: 'inline-tools-core-from-source',
  setup(build) {
    const filter = /^@octocodeai\/octocode-tools-core(\/.*)?$/;
    build.onResolve({ filter }, (args) => {
      const target = toolsCoreSpecifierToSrc.get(args.path);
      if (!target) {
        return {
          errors: [
            {
              text: `Unmapped @octocodeai/octocode-tools-core import "${args.path}" — add its subpath to tools-core's package.json exports + buildConfig.entryPoints so the CLI build can resolve it from source.`,
            },
          ],
        };
      }
      return { path: target };
    });
  },
};

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
  entryNames: 'octocode',
  chunkNames: 'chunks/[name]-[hash]',
  splitting: true,
  minify: true,
  treeShaking: true,
  banner: { js: shimBanner },
  external,
  plugins: [inlineToolsCoreFromSource],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    // Match tools-core's own production build (buildConfig.mjs) now that we inline
    // its source directly: gates its test-only hooks off in the shipped CLI.
    'process.env.NODE_ENV': '"production"',
  },
  logLevel: 'info',
});

console.log('✓ esbuild complete');

// Copy bundled skills into packages/octocode/skills/ so they are available both
// during local dev (from out/../skills) and when installed from npm (node_modules/octocode/skills/).
const repoRoot = resolve(__dirname, '..', '..');
const skillsSource = resolve(repoRoot, 'skills');
const awarenessSkillSource = resolve(
  repoRoot,
  'packages',
  'octocode-awareness',
  'skills',
  'octocode-awareness'
);
const skillsDest = resolve(__dirname, 'skills');
await rm(skillsDest, { recursive: true, force: true });
await cp(skillsSource, skillsDest, { recursive: true });
await rm(resolve(skillsDest, 'octocode-awareness'), {
  recursive: true,
  force: true,
});
await cp(awarenessSkillSource, resolve(skillsDest, 'octocode-awareness'), {
  recursive: true,
});
console.log('✓ bundled skills copied');

const cliEntry = resolve(__dirname, 'out', 'octocode.js');
const cliSource = readFileSync(cliEntry, 'utf-8');
writeFileSync(
  cliEntry,
  cliSource.startsWith('#!') ? cliSource : `#!/usr/bin/env node\n${cliSource}`
);
chmodSync(cliEntry, 0o755);
