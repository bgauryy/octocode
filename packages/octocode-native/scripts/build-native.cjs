#!/usr/bin/env node
/**
 * Builds and stages the native `octocode` binary (CLI and MCP server).
 *
 * Usage:
 *   build-native.cjs [--release]
 *   build-native.cjs --release --target <platform> [--target <platform>...]
 *   build-native.cjs --release --all [--jobs N]
 *   build-native.cjs --check [--target <platform>... | --all] [--jobs N]
 *
 * No --target builds for the host and stages the binary into npm/<host>/,
 * where the launchers (`octocode`, `octocode-mcp`) find it. A --target build
 * stages it into npm/<platform>/. Cross targets use cargo-zigbuild (Linux)
 * or cargo-xwin (Windows).
 *
 * --check type-checks the same selection for each platform through the same
 * cross tool (its C build scripts included) without linking or staging: the
 * fast loop for platform-specific code (`cfg(windows)`, `cfg(target_os)`).
 *
 * Each platform's cargo job holds a machine-wide build slot (build-slot.cjs).
 */
'use strict';

const { spawn } = require('child_process');
const { mkdirSync, rmSync } = require('fs');
const { availableParallelism, homedir, totalmem } = require('os');
const { parseArgs } = require('util');
const { join, relative } = require('path');
const { PLATFORMS, executableName, getPlatformSuffix } = require('../bin/platform.cjs');
const { slotCount, withSlot } = require('./build-slot.cjs');
const { stageFile, verifyBinaryRuns } = require('./stage-binary.cjs');
const { finishCargoRun, isCargoProgress, parseCargoLine } = require('./target-hygiene.cjs');

const ROOT = join(__dirname, '..');

function usage(message) {
  if (message) console.error(`build-native: ${message}`);
  console.error(
    'Usage: build-native.cjs [--release | --check] [--target <platform>... | --all] [--jobs N]\n' +
      `Platforms: ${Object.keys(PLATFORMS).join(', ')}`,
  );
  process.exit(2);
}

function parseOptions(argv) {
  let values;
  try {
    ({ values } = parseArgs({
      args: argv,
      options: {
        release: { type: 'boolean', default: false },
        check: { type: 'boolean', default: false },
        all: { type: 'boolean', default: false },
        target: { type: 'string', multiple: true, default: [] },
        jobs: { type: 'string' },
      },
    }));
  } catch (error) {
    usage(error.message);
  }
  const { release, check, all, target, jobs } = values;
  if (all && target.length) usage('--all and --target are exclusive');
  if (check && release) usage('--check and --release are exclusive');
  const targets = all ? Object.keys(PLATFORMS) : target;
  for (const platform of targets) if (!PLATFORMS[platform]) usage(`unknown platform ${platform}`);
  const jobCount = jobs === undefined ? null : Number(jobs);
  if (jobCount !== null && !(Number.isInteger(jobCount) && jobCount > 0)) usage('--jobs must be a positive integer');
  return { release, check, all, targets, jobs: jobCount };
}

const hostPlatform = getPlatformSuffix();

/**
 * A target of another OS or CPU than the host needs a cross linker. A
 * libc-only difference (musl on a glibc host of the same CPU) links with the
 * host toolchain plus musl-tools.
 */
function needsCrossLinker(platform) {
  const host = PLATFORMS[hostPlatform];
  const target = PLATFORMS[platform];
  return !host || host.os !== target.os || host.arch !== target.arch;
}

function formatSeconds(ms) {
  return `${(ms / 1000).toFixed(1)}s`;
}

const running = new Set();

/**
 * Spawn with line-prefixed output so concurrent platform builds stay readable.
 * Cargo JSON messages on stdout go to `messages` instead of the terminal, and
 * its per-crate progress lines are dropped.
 */
function run(label, command, args, env, messages = []) {
  const started = Date.now();
  console.log(`[${label}] $ ${command} ${args.join(' ')}`);
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: ROOT, env, stdio: ['ignore', 'pipe', 'pipe'] });
    running.add(child);
    for (const stream of [child.stdout, child.stderr]) {
      let pending = '';
      const print = line => {
        const rest = stream === child.stdout ? parseCargoLine(line, messages) : line;
        if (rest !== null && !isCargoProgress(rest)) process.stdout.write(`[${label}] ${rest}\n`);
      };
      stream.setEncoding('utf8');
      stream.on('data', chunk => {
        const lines = (pending + chunk).split('\n');
        pending = lines.pop();
        for (const line of lines) print(line);
      });
      stream.on('end', () => pending && print(pending));
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

/**
 * Host builds keep `target/` (warm with test/lint artifacts). Each explicit
 * platform gets its own dir so platforms never contend on one Cargo lock.
 */
function targetDir(platform) {
  return platform ? join(ROOT, 'target', 'platforms', platform) : join(ROOT, 'target');
}

/**
 * The oldest glibc a cross-linked gnu artifact runs on: Node 24's own floor
 * (nodejs/node BUILDING.md, platform list), so the binary runs wherever Node
 * does. cargo-zigbuild reads it from the target suffix; without it zig links
 * hard GLIBC_2.29/2.30 version requirements (libm `pow`/`log`, and std's
 * weak `gettid`). A native Linux build links the host's glibc instead.
 */
const GLIBC_FLOOR = '2.28';

/**
 * The oldest macOS a darwin artifact runs on, for rustc and every C build
 * script; Rust's own default for Apple Silicon.
 */
const MACOS_FLOOR = '11.0';

/**
 * Release artifacts name no path of the machine that built them. Rust paths
 * map through `--remap-path-prefix` (the last matching prefix wins, so the
 * most specific comes last); C build scripts map through `-ffile-prefix-map`,
 * except clang-cl on Windows, which takes no such flag.
 */
function pathRemaps(os) {
  const repo = join(ROOT, '..', '..');
  const cargoHome = process.env.CARGO_HOME || join(homedir(), '.cargo');
  const maps = [[homedir(), '/home'], [cargoHome, '/cargo'], [repo, '/octocode']];
  const rustflags = maps.map(([from, to]) => `--remap-path-prefix=${from}=${to}`);
  const cflags = os === 'win32' ? null : [process.env.CFLAGS, ...maps.map(([from, to]) => `-ffile-prefix-map=${from}=${to}`)].filter(Boolean).join(' ');
  return { rustflags, cflags };
}

/**
 * The Cargo invocation for `platform` (null: the host) as `{ command, args,
 * env }`; env holds only the variables it adds.
 */
function hostsCommand(platform, { release = false, check = false }, dir) {
  const target = platform && PLATFORMS[platform];
  const shipped = target || PLATFORMS[hostPlatform];
  const env = {};
  // `--config` arrays join the config file's: the Windows crt-static flags stay.
  const releaseConfig = [];
  if (release) {
    const { rustflags, cflags } = pathRemaps(shipped.os);
    releaseConfig.push('--config', `target.${shipped.triple}.rustflags=${JSON.stringify(rustflags)}`);
    if (cflags) env.CFLAGS = env.CXXFLAGS = cflags;
    if (shipped.os === 'darwin') env.MACOSX_DEPLOYMENT_TARGET = MACOS_FLOOR;
  }
  const subcommand = check ? 'check' : 'build';
  let command = 'cargo';
  let cargo = [subcommand];
  let triple = target?.triple;
  if (target && needsCrossLinker(platform)) {
    // `cargo zigbuild` is only the build subcommand; check is `cargo-zigbuild check`.
    if (target.os === 'linux') {
      [command, cargo] = ['cargo-zigbuild', [check ? 'check' : 'zigbuild']];
      if (target.libc === 'gnu') triple = `${target.triple}.${GLIBC_FLOOR}`;
    } else if (target.os === 'win32') cargo = ['xwin', subcommand];
  }
  const args = [
    ...cargo,
    '--locked',
    '-p', 'octocode-cli',
    '--bins',
    // Dev builds share compiled crates with `test:rust` (see crates/runtime `dev-unify`).
    ...(release ? ['--release', ...releaseConfig] : ['--features', 'octocode-native/dev-unify']),
    ...(target ? ['--target', triple] : []),
    '--target-dir', dir,
    // Lists every unit's outputs, fresh ones included: target-hygiene.cjs records them.
    '--message-format=json-render-diagnostics',
  ];
  return { command, args, env };
}

function stageBinary(platform, explicitTarget, release, dir) {
  const profile = release ? 'release' : 'debug';
  const built = explicitTarget ? join(dir, PLATFORMS[platform].triple, profile) : join(dir, profile);
  const packageDir = join(ROOT, 'npm', platform);
  mkdirSync(packageDir, { recursive: true });
  const file = executableName('octocode', PLATFORMS[platform].os);
  const destination = join(packageDir, file);
  stageFile(join(built, file), destination, { executable: true });
  return [destination];
}

function verifyStaged(platform, staged) {
  if (platform !== hostPlatform) return;
  const { version } = require(join(ROOT, 'npm', platform, 'package.json'));
  for (const artifact of staged) verifyBinaryRuns(artifact, version);
}

/**
 * The build dir a staged platform build leaves for deletion, else null. A
 * release `--all` build is the release line's one-off: its per-platform dirs
 * (0.5-0.7 GB each) are not reused before the next release, while `target/`
 * holds the host's warm dev, test and lint builds and stays.
 */
function buildDirToDrop(platform, explicitTarget, options) {
  return explicitTarget && options.all && options.release && !options.check ? targetDir(platform) : null;
}

async function buildPlatform(platform, explicitTarget, options) {
  const label = explicitTarget ? platform : 'host';
  const target = explicitTarget ? platform : null;
  const dir = targetDir(target);
  const { command, args, env } = hostsCommand(target, options, dir);
  const mode = options.check ? 'check' : options.release ? 'build-release' : 'build-dev';
  const selection = `${mode}-${target ?? 'host'}`;
  await withSlot(`build-native ${selection}`, async slot => {
    const messages = [];
    const startedMs = Date.now();
    let success = false;
    try {
      await run(`${label}:${options.check ? 'check' : 'build'}`, command, args, { ...slot.env, ...env }, messages);
      success = true;
    } finally {
      finishCargoRun({ selection, messages, success, startedMs, targetRoot: targetDir(null), log: line => console.log(`[${label}] ${line}`) });
    }
  });
  if (options.check) return;

  const staged = stageBinary(platform, explicitTarget, options.release, dir);
  verifyStaged(platform, staged);
  for (const artifact of staged) console.log(`[${label}] staged ${relative(ROOT, artifact)}`);
  const drop = buildDirToDrop(platform, explicitTarget, options);
  if (drop) {
    rmSync(drop, { recursive: true, force: true });
    console.log(`[${label}] removed ${relative(ROOT, drop)}`);
  }
}

/**
 * Concurrent platform builds. Each one ends in serial work (the engine's
 * codegen-units = 1, then thin LTO), so most cores idle under one build; memory
 * bounds them, at a few GB peak per release link. Measured 2026-10-25 on 12
 * cores / 32 GB, 8 platforms: 3 jobs 751 s (31% CPU), 4 jobs 521 s (45% CPU),
 * peak RSS 2.9-3.9 GB, no swap-outs. A job past the build slot count only waits.
 */
function defaultJobs(count) {
  const byCpu = Math.max(1, Math.floor(availableParallelism() / 2));
  const byMemory = Math.max(1, Math.floor(totalmem() / (8 * 1024 ** 3)));
  return Math.min(count, byCpu, byMemory, slotCount());
}

async function main() {
  const options = parseOptions(process.argv.slice(2));
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
      console.log(`${options.check ? 'checking' : 'building'} ${queue.join(', ')} with ${jobs} concurrent platform job(s)`);
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
else module.exports = { GLIBC_FLOOR, MACOS_FLOOR, buildDirToDrop, hostsCommand, targetDir };
