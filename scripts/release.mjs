#!/usr/bin/env node
/**
 * release.mjs — builds octocode-mcp / octocode CLI as single self-contained
 * executables using Node's Single Executable Application (SEA) support.
 *
 * The whole program (all JS dependencies, tools-core inlined FROM SOURCE like
 * the CLI build — no stale-dist trap) is bundled into one CJS file, and the
 * @octocodeai/octocode-engine native .node addon is embedded as a SEA asset.
 * At startup the generated wrapper extracts the addon to a versioned cache dir
 * (native code cannot be dlopen'd from inside an executable), pre-loads it,
 * and publishes it as globalThis.__OCTOCODE_ENGINE_BINDING__ — the fallback
 * every engine load site checks first.
 *
 * Usage:
 *   node scripts/release.mjs [--target mcp|cli] [--platform <key>]
 *                              [--node-bin <path>] [--keep-workdir]
 *
 * Output: releases/mcp/octocode-mcp-<platform>  ·  releases/cli/octocode-<platform>
 *
 * Platforms: darwin-arm64 | darwin-x64 | linux-arm64 | linux-x64 |
 *            linux-x64-musl | windows-x64   (default: current host)
 *
 * Cross-platform note: the SEA blob is injected into a *node binary for the
 * target platform*. By default the current `node` (process.execPath) is used,
 * which only produces a binary for the host platform. CI builds other targets
 * natively on a runner matrix (or pass --node-bin to a downloaded target node).
 */

import { execFileSync, spawnSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const repoRoot = join(__dirname, '..');
const enginePkgRoot = join(repoRoot, 'packages', 'octocode-engine');
const toolsCoreDir = join(repoRoot, 'packages', 'octocode-tools-core');

// SEA blob injection sentinel — fixed value defined by Node's SEA docs.
const NODE_SEA_FUSE = 'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2';

const PLATFORM_MAP = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-arm64': 'linux-arm64-gnu',
  'linux-x64': 'linux-x64-gnu',
  'linux-x64-musl': 'linux-x64-musl',
  'windows-x64': 'win32-x64-msvc',
};

const TARGETS = {
  mcp: {
    pkgDir: join(repoRoot, 'packages', 'octocode-mcp'),
    entry: join(repoRoot, 'packages', 'octocode-mcp', 'src', 'index.ts'),
    outName: 'octocode-mcp',
    releaseDir: join(repoRoot, 'releases', 'mcp'),
  },
  cli: {
    pkgDir: join(repoRoot, 'packages', 'octocode'),
    entry: join(repoRoot, 'packages', 'octocode', 'src', 'index.ts'),
    outName: 'octocode',
    releaseDir: join(repoRoot, 'releases', 'cli'),
  },
};

function hostPlatformKey() {
  const { platform, arch } = process;
  if (platform === 'darwin') return arch === 'arm64' ? 'darwin-arm64' : 'darwin-x64';
  if (platform === 'linux') return arch === 'arm64' ? 'linux-arm64' : 'linux-x64';
  if (platform === 'win32') return 'windows-x64';
  throw new Error(`Unsupported host platform: ${platform}-${arch}`);
}

function parseArgs(argv) {
  const args = {
    target: 'mcp',
    platform: hostPlatformKey(),
    nodeBin: process.execPath,
    keepWorkdir: false,
  };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--target') args.target = argv[++i];
    else if (argv[i] === '--platform') args.platform = argv[++i];
    else if (argv[i] === '--node-bin') args.nodeBin = argv[++i];
    else if (argv[i] === '--keep-workdir') args.keepWorkdir = true;
    else throw new Error(`Unknown argument: ${argv[i]}`);
  }
  return args;
}

const { target, platform, nodeBin, keepWorkdir } = parseArgs(process.argv.slice(2));

const targetConfig = TARGETS[target];
if (!targetConfig) {
  console.error(`Unknown target: ${target}. Valid targets: ${Object.keys(TARGETS).join(', ')}`);
  process.exit(1);
}
const triple = PLATFORM_MAP[platform];
if (!triple) {
  console.error(`Unknown platform: ${platform}`);
  console.error(`  Valid platforms: ${Object.keys(PLATFORM_MAP).join(', ')}`);
  process.exit(1);
}

const engineBinaryName = `octocode-engine.${triple}.node`;
const engineBinary = join(enginePkgRoot, 'npm', triple, engineBinaryName);
if (!existsSync(engineBinary)) {
  console.error(`⚠  engine binary not found: ${engineBinary}`);
  console.error('   Build it first: yarn workspace @octocodeai/octocode-engine build:all (or the matching build:<platform> script)');
  process.exit(1);
}
if (nodeBin !== process.execPath && !existsSync(nodeBin)) {
  console.error(`⚠  --node-bin not found: ${nodeBin}`);
  process.exit(1);
}

const engineVersion = JSON.parse(
  readFileSync(join(enginePkgRoot, 'package.json'), 'utf8')
).version;
const targetPkg = JSON.parse(
  readFileSync(join(targetConfig.pkgDir, 'package.json'), 'utf8')
);

const isWindowsTarget = platform === 'windows-x64';
const isDarwinTarget = platform.startsWith('darwin');
const workDir = join(targetConfig.pkgDir, 'dist', '.sea');
const outFile = join(
  targetConfig.releaseDir,
  `${targetConfig.outName}-${platform}${isWindowsTarget ? '.exe' : ''}`
);

rmSync(workDir, { recursive: true, force: true });
mkdirSync(workDir, { recursive: true });
mkdirSync(targetConfig.releaseDir, { recursive: true });

// 1. Generate the SEA entry wrapper for this target + platform.
const wrapper = `'use strict';
// Generated by scripts/release.mjs — Node SEA entry for ${target}/${platform}.
const { getAsset } = require('node:sea');
const { existsSync, mkdirSync, renameSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');
const { homedir, tmpdir } = require('node:os');
const { createRequire } = require('node:module');

const ENGINE_FILE = ${JSON.stringify(engineBinaryName)};
const ENGINE_VERSION = ${JSON.stringify(engineVersion)};

function ensureDir(dir) {
  mkdirSync(dir, { recursive: true });
  return dir;
}

function cacheDir() {
  try {
    return ensureDir(join(homedir(), '.octocode', 'native', ENGINE_VERSION));
  } catch {
    return ensureDir(join(tmpdir(), 'octocode-native', ENGINE_VERSION));
  }
}

// Native addons cannot be dlopen'd from inside the executable — extract once
// to a versioned cache dir. Write-to-temp + rename keeps concurrent starts
// from ever seeing a partially written addon.
const dest = join(cacheDir(), ENGINE_FILE);
if (!existsSync(dest)) {
  const tmp = dest + '.' + process.pid + '.tmp';
  writeFileSync(tmp, Buffer.from(getAsset('engine')), { mode: 0o755 });
  try {
    renameSync(tmp, dest);
  } catch (err) {
    if (!existsSync(dest)) throw err; // a concurrent start already won the race
  }
}

// SEA's own require() only loads node builtins; createRequire(__filename)
// gives a real require that can dlopen from disk.
globalThis.__OCTOCODE_ENGINE_BINDING__ = createRequire(__filename)(dest);

import(${JSON.stringify(targetConfig.entry)}).catch((err) => {
  console.error(err);
  process.exit(1);
});
`;
writeFileSync(join(workDir, 'wrapper.cjs'), wrapper);

// 2. Bundle wrapper + program + every JS dependency into a single CJS file.
//    tools-core is inlined FROM SOURCE (same derived specifier→src map as the
//    CLI build) so the bundle can never diverge from what is greppable.
const toolsCorePkg = JSON.parse(
  readFileSync(join(toolsCoreDir, 'package.json'), 'utf8')
);
const { entryPoints: toolsCoreEntryPoints } = await import(
  pathToFileURL(join(toolsCoreDir, 'buildConfig.mjs')).href
);
const distOutfileToSrcEntry = new Map(
  toolsCoreEntryPoints.map((entry) => {
    const srcEntry = Array.isArray(entry.entryPoints)
      ? entry.entryPoints[0]
      : entry.entryPoints;
    return [entry.outfile.replace(/^\.?\//, ''), srcEntry];
  })
);
const toolsCoreSpecifierToSrc = new Map();
for (const [subpath, exportTarget] of Object.entries(toolsCorePkg.exports ?? {})) {
  const importTarget =
    typeof exportTarget === 'string' ? exportTarget : exportTarget?.import;
  if (!importTarget) continue;
  const srcEntry = distOutfileToSrcEntry.get(importTarget.replace(/^\.?\//, ''));
  if (!srcEntry) continue;
  const specifier =
    subpath === '.'
      ? toolsCorePkg.name
      : `${toolsCorePkg.name}/${subpath.replace(/^\.\//, '')}`;
  toolsCoreSpecifierToSrc.set(specifier, resolve(toolsCoreDir, srcEntry));
}

const inlineToolsCoreFromSource = {
  name: 'inline-tools-core-from-source',
  setup(build) {
    const filter = /^@octocodeai\/octocode-tools-core(\/.*)?$/;
    build.onResolve({ filter }, (args) => {
      const src = toolsCoreSpecifierToSrc.get(args.path);
      if (!src) {
        return {
          errors: [
            {
              text: `Unmapped @octocodeai/octocode-tools-core import "${args.path}" — add its subpath to tools-core's package.json exports + buildConfig.entryPoints.`,
            },
          ],
        };
      }
      return { path: src };
    });
  },
};

const esbuildModule = await import(
  pathToFileURL(require.resolve('esbuild', { paths: [toolsCoreDir] })).href
);
const esbuild = esbuildModule.default ?? esbuildModule;
await esbuild.build({
  entryPoints: [join(workDir, 'wrapper.cjs')],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  minify: true,
  treeShaking: true,
  outfile: join(workDir, 'sea-bundle.cjs'),
  loader: { '.md': 'text' },
  plugins: [inlineToolsCoreFromSource],
  define: {
    'process.env.NODE_ENV': '"production"',
    __APP_VERSION__: JSON.stringify(targetPkg.version),
    // import.meta.url has no CJS equivalent; point createRequire(import.meta.url)
    // sites at the executable via the banner shim below.
    'import.meta.url': 'importMetaUrl',
  },
  banner: {
    js: "const importMetaUrl = require('node:url').pathToFileURL(__filename).href;",
  },
  logLevel: 'warning',
});

// 3. Build the SEA preparation blob (bundle + engine asset).
const seaConfig = {
  main: join(workDir, 'sea-bundle.cjs'),
  output: join(workDir, 'sea-prep.blob'),
  disableExperimentalSEAWarning: true,
  useCodeCache: false, // code cache is host-specific; keep blobs target-portable
  assets: { engine: engineBinary },
};
writeFileSync(join(workDir, 'sea-config.json'), JSON.stringify(seaConfig, null, 2));
execFileSync(
  process.execPath,
  ['--experimental-sea-config', join(workDir, 'sea-config.json')],
  { stdio: 'inherit' }
);

// 4. Copy the target node binary and inject the blob.
copyFileSync(nodeBin, outFile);
chmodSync(outFile, 0o755);

if (isDarwinTarget && process.platform === 'darwin') {
  execFileSync('codesign', ['--remove-signature', outFile], { stdio: 'inherit' });
}

const postjectArgs = [
  outFile,
  'NODE_SEA_BLOB',
  join(workDir, 'sea-prep.blob'),
  '--sentinel-fuse',
  NODE_SEA_FUSE,
  ...(isDarwinTarget ? ['--macho-segment-name', 'NODE_SEA'] : []),
];
let postjectCli;
try {
  postjectCli = require.resolve('postject/dist/cli.js', { paths: [repoRoot] });
} catch {
  postjectCli = null;
}
if (postjectCli) {
  execFileSync(process.execPath, [postjectCli, ...postjectArgs], { stdio: 'inherit' });
} else {
  const result = spawnSync('npx', ['-y', 'postject', ...postjectArgs], {
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) {
    console.error('⚠  postject failed (install it: yarn add -D postject)');
    process.exit(1);
  }
}

if (isDarwinTarget && process.platform === 'darwin') {
  execFileSync('codesign', ['-s', '-', outFile], { stdio: 'inherit' });
}

if (!keepWorkdir) rmSync(workDir, { recursive: true, force: true });

const sizeMb = (statSync(outFile).size / (1024 * 1024)).toFixed(0);
console.log(
  `✓ SEA executable: ${outFile} (${sizeMb}MB, ${target} ${targetPkg.version}, engine ${engineVersion}, ${platform})`
);
