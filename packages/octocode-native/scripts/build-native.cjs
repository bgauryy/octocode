#!/usr/bin/env node
/**
 * Builds and stages the native artifacts.
 *
 *   hosts  = `octocode` + `octocode-regex-worker` binaries and the runtime addon,
 *            one Cargo invocation (shared engine feature resolution).
 *   engine = the engine addon via `napi build` (`portable-default,napi-addon`).
 *
 * The two resolve octocode-engine with different features, so they cannot share
 * one Cargo invocation, and Cargo holds one lock per target dir. Each therefore
 * gets its own target dir and they build concurrently: the engine is compiled at
 * `codegen-units = 1` in release, so the two serial engine compiles are the
 * critical path. `--serial` keeps the old one-dir sequential flow for comparison.
 *
 * Usage:
 *   build-native.cjs [--release] [--only hosts|engine] [--serial]
 *   build-native.cjs --release --target <platform> [--target <platform>...]
 *   build-native.cjs --release --all [--jobs N]
 *
 * No --target builds for the host and stages binaries into npm/<host>/ and both
 * addons into the package root (what the local launcher and MCP load). A
 * --target build stages all four artifacts into npm/<platform>/ (and the root
 * addons too when the platform is the host). Cross targets use cargo-zigbuild
 * (Linux) or cargo-xwin (Windows), as napi's --cross-compile does.
 */
'use strict';

const { spawn } = require('child_process');
const { mkdirSync } = require('fs');
const { availableParallelism, totalmem } = require('os');
const { join, relative } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');
const { stageFile, verifyAddonLoads, verifyBinaryRuns } = require('./native-addon-utils.cjs');

const ROOT = join(__dirname, '..');
const PLATFORMS = {
  'darwin-arm64': { triple: 'aarch64-apple-darwin', os: 'darwin', arch: 'arm64', libc: null },
  'darwin-x64': { triple: 'x86_64-apple-darwin', os: 'darwin', arch: 'x64', libc: null },
  'linux-arm64-gnu': { triple: 'aarch64-unknown-linux-gnu', os: 'linux', arch: 'arm64', libc: 'gnu' },
  'linux-x64-gnu': { triple: 'x86_64-unknown-linux-gnu', os: 'linux', arch: 'x64', libc: 'gnu' },
  'linux-x64-musl': { triple: 'x86_64-unknown-linux-musl', os: 'linux', arch: 'x64', libc: 'musl' },
  'win32-x64-msvc': { triple: 'x86_64-pc-windows-msvc', os: 'win32', arch: 'x64', libc: null },
};
const ENGINE_FEATURES = 'portable-default,napi-addon';

function usage(message) {
  if (message) console.error(`build-native: ${message}`);
  console.error(
    'Usage: build-native.cjs [--release] [--only hosts|engine] [--serial] [--target <platform>... | --all] [--jobs N]\n' +
      `Platforms: ${Object.keys(PLATFORMS).join(', ')}`,
  );
  process.exit(2);
}

function parseArgs(argv) {
  const options = { release: false, only: null, serial: false, targets: [], all: false, jobs: null };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const value = () => (i + 1 < argv.length ? argv[++i] : usage(`${arg} needs a value`));
    if (arg === '--release') options.release = true;
    else if (arg === '--serial') options.serial = true;
    else if (arg === '--all') options.all = true;
    else if (arg === '--only') options.only = value();
    else if (arg === '--target') options.targets.push(value());
    else if (arg === '--jobs') options.jobs = Number(value());
    else usage(`unknown argument ${arg}`);
  }
  if (options.only && !['hosts', 'engine'].includes(options.only)) usage(`--only must be hosts or engine`);
  if (options.all && options.targets.length) usage('--all and --target are exclusive');
  if (options.all) options.targets = Object.keys(PLATFORMS);
  for (const target of options.targets) if (!PLATFORMS[target]) usage(`unknown platform ${target}`);
  if (options.targets.length && options.only) usage('--only applies to host builds only');
  if (options.jobs !== null && !(Number.isInteger(options.jobs) && options.jobs > 0)) usage('--jobs must be a positive integer');
  return options;
}

const hostPlatform = getPlatformSuffix();

function isCross(platform) {
  const host = PLATFORMS[hostPlatform];
  const target = PLATFORMS[platform];
  return !host || host.os !== target.os || host.arch !== target.arch || host.libc !== target.libc;
}

/** A libc-only difference (musl on a glibc x64 runner) links with the host toolchain CI installs. */
function needsCrossLinker(platform) {
  const host = PLATFORMS[hostPlatform];
  const target = PLATFORMS[platform];
  return !host || host.os !== target.os || host.arch !== target.arch;
}

function formatSeconds(ms) {
  return `${(ms / 1000).toFixed(1)}s`;
}

const running = new Set();

/** Spawn with line-prefixed output so concurrent builds stay readable. */
function run(label, command, args, env) {
  const started = Date.now();
  console.log(`[${label}] $ ${command} ${args.join(' ')}`);
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: ROOT, env, stdio: ['ignore', 'pipe', 'pipe'] });
    running.add(child);
    for (const stream of [child.stdout, child.stderr]) {
      let pending = '';
      stream.setEncoding('utf8');
      stream.on('data', chunk => {
        const lines = (pending + chunk).split('\n');
        pending = lines.pop();
        for (const line of lines) process.stdout.write(`[${label}] ${line}\n`);
      });
      stream.on('end', () => pending && process.stdout.write(`[${label}] ${pending}\n`));
    }
    child.on('error', error => {
      running.delete(child);
      reject(new Error(`[${label}] could not start ${command}: ${error.message}`));
    });
    child.on('close', (code, signal) => {
      running.delete(child);
      if (code === 0) {
        console.log(`[${label}] done in ${formatSeconds(Date.now() - started)}`);
        resolve();
      } else {
        reject(new Error(`[${label}] ${command} exited ${code ?? signal}`));
      }
    });
  });
}

function napiCli() {
  const manifest = require.resolve('@napi-rs/cli/package.json', { paths: [ROOT] });
  const { bin } = require(manifest);
  return join(manifest, '..', typeof bin === 'string' ? bin : bin.napi);
}

/**
 * Target dirs. Host builds keep `target/` for hosts (warm with test/lint
 * artifacts) and `target/napi-engine` for the engine. Each explicit platform
 * gets its own pair so platforms never contend on one Cargo lock.
 */
function targetDirs(platform, serial) {
  if (serial) return { hosts: join(ROOT, 'target'), engine: join(ROOT, 'target') };
  if (!platform) return { hosts: join(ROOT, 'target'), engine: join(ROOT, 'target', 'napi-engine') };
  const base = join(ROOT, 'target', 'platforms', platform);
  return { hosts: join(base, 'hosts'), engine: join(base, 'engine') };
}

function crossEnv(platform) {
  const env = { ...process.env };
  const target = platform && PLATFORMS[platform];
  // A static-CRT musl target cannot emit the cdylib addons; CI builds musl natively and is unaffected.
  if (target?.libc === 'musl' && isCross(platform)) {
    const key = `CARGO_TARGET_${target.triple.toUpperCase().replace(/-/g, '_')}_RUSTFLAGS`;
    env[key] = `${env[key] ?? ''} -C target-feature=-crt-static`.trim();
  }
  return env;
}

function hostsCommand(platform, release, dir) {
  const target = platform && PLATFORMS[platform];
  let cargo = ['build'];
  if (target && needsCrossLinker(platform)) {
    if (target.os === 'linux') cargo = ['zigbuild'];
    else if (target.os === 'win32') cargo = ['xwin', 'build'];
  }
  return [
    ...cargo,
    '--locked',
    '-p', 'octocode-cli',
    '-p', 'octocode-runtime-napi',
    '--bins', '--lib', '--no-default-features',
    ...(release ? ['--release'] : []),
    ...(target ? ['--target', target.triple] : []),
    '--target-dir', dir,
  ];
}

function engineCommand(platform, release, dir, outDir) {
  const target = platform && PLATFORMS[platform];
  return [
    napiCli(),
    'build',
    '--manifest-path', 'crates/engine/Cargo.toml',
    '--package-json-path', 'package.json',
    '--output-dir', outDir,
    '--target-dir', dir,
    '--platform',
    ...(release ? ['--release'] : []),
    ...(target ? ['--target', target.triple] : []),
    ...(target && target.os !== 'darwin' && isCross(platform) ? ['--cross-compile'] : []),
    '--no-default-features', '--features', ENGINE_FEATURES,
    '--js', 'engine-generated.cjs',
    '--dts', 'engine-generated.d.ts',
    '--', '--locked',
  ];
}

function artifactNames(platform) {
  const os = PLATFORMS[platform].os;
  const exe = os === 'win32' ? '.exe' : '';
  return {
    binaries: ['octocode', 'octocode-regex-worker'].map(name => `${name}${exe}`),
    library:
      os === 'win32' ? 'octocode_runtime_napi.dll'
        : os === 'darwin' ? 'liboctocode_runtime_napi.dylib'
          : 'liboctocode_runtime_napi.so',
  };
}

function stageHosts(platform, explicitTarget, release, dir) {
  const { binaries, library } = artifactNames(platform);
  const profile = release ? 'release' : 'debug';
  const built = explicitTarget ? join(dir, PLATFORMS[platform].triple, profile) : join(dir, profile);
  const packageDir = join(ROOT, 'npm', platform);
  mkdirSync(packageDir, { recursive: true });
  const staged = [];
  for (const binary of binaries) {
    const destination = join(packageDir, binary);
    stageFile(join(built, binary), destination, { executable: true });
    staged.push(destination);
  }
  const addon = `octocode-native.${platform}.node`;
  const destinations = explicitTarget ? [join(packageDir, addon)] : [];
  if (platform === hostPlatform) destinations.push(join(ROOT, addon));
  for (const destination of destinations) {
    stageFile(join(built, library), destination, { platform });
    staged.push(destination);
  }
  return staged;
}

function stageEngine(platform, explicitTarget, outDir) {
  const addon = `octocode-engine.${platform}.node`;
  // napi emits the real ABI declarations; the hand-written loader stays
  // canonical and check-engine-napi-abi diffs the two.
  stageFile(join(outDir, 'engine-generated.d.ts'), join(ROOT, '.napi-abi-snapshot.d.ts'));
  const destinations = explicitTarget ? [join(ROOT, 'npm', platform, addon)] : [];
  if (platform === hostPlatform) destinations.push(join(ROOT, addon));
  for (const destination of destinations) {
    mkdirSync(join(destination, '..'), { recursive: true });
    stageFile(join(outDir, addon), destination, { platform });
  }
  return destinations;
}

function verifyStaged(platform, staged) {
  if (platform !== hostPlatform) return;
  for (const artifact of staged) {
    if (artifact.endsWith('.node')) verifyAddonLoads(artifact);
    else if (/[\\/]octocode(\.exe)?$/.test(artifact)) verifyBinaryRuns(artifact);
  }
}

async function buildPlatform(platform, explicitTarget, options) {
  const label = explicitTarget ? platform : 'host';
  const dirs = targetDirs(explicitTarget ? platform : null, options.serial);
  const outDir = join(dirs.engine, 'napi-out', platform);
  const env = crossEnv(explicitTarget ? platform : null);
  const hosts = options.only !== 'engine';
  const engine = options.only !== 'hosts';
  const buildHosts = () =>
    run(`${label}:hosts`, 'cargo', hostsCommand(explicitTarget ? platform : null, options.release, dirs.hosts), env);
  const buildEngine = () =>
    run(`${label}:engine`, process.execPath, engineCommand(explicitTarget ? platform : null, options.release, dirs.engine, outDir), env);

  if (options.serial) {
    if (hosts) await buildHosts();
    if (engine) await buildEngine();
  } else {
    await Promise.all([hosts && buildHosts(), engine && buildEngine()]);
  }

  const staged = [
    ...(hosts ? stageHosts(platform, explicitTarget, options.release, dirs.hosts) : []),
    ...(engine ? stageEngine(platform, explicitTarget, outDir) : []),
  ];
  verifyStaged(platform, staged);
  for (const artifact of staged) console.log(`[${label}] staged ${relative(ROOT, artifact)}`);
}

/** Two concurrent release links per platform peak at a few GB each. */
function defaultJobs(count) {
  const byCpu = Math.max(1, Math.floor(availableParallelism() / 4));
  const byMemory = Math.max(1, Math.floor(totalmem() / (8 * 1024 ** 3)));
  return Math.min(count, byCpu, byMemory);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!hostPlatform && !options.targets.length) usage(`unsupported host ${process.platform}-${process.arch}`);
  const started = Date.now();
  const onSignal = signal => {
    for (const child of running) child.kill(signal);
    process.exit(130);
  };
  process.on('SIGINT', onSignal);
  process.on('SIGTERM', onSignal);

  try {
    if (!options.targets.length) {
      await buildPlatform(hostPlatform, false, options);
    } else {
      const queue = [...options.targets];
      const jobs = options.jobs ?? defaultJobs(queue.length);
      console.log(`building ${queue.join(', ')} with ${jobs} concurrent platform job(s)`);
      await Promise.all(
        Array.from({ length: jobs }, async () => {
          while (queue.length) await buildPlatform(queue.shift(), true, options);
        }),
      );
    }
  } catch (error) {
    for (const child of running) child.kill('SIGTERM');
    console.error(error.message);
    process.exit(1);
  }
  console.log(`build-native finished in ${formatSeconds(Date.now() - started)}`);
}

if (require.main === module) main();
else module.exports = { PLATFORMS, engineCommand, hostsCommand, stageFile, targetDirs };
