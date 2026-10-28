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
 */
'use strict';

const { spawn } = require('child_process');
const { mkdirSync } = require('fs');
const { availableParallelism, totalmem } = require('os');
const { parseArgs } = require('util');
const { join, relative } = require('path');
const { PLATFORMS, executableName, getPlatformSuffix } = require('../bin/platform.cjs');
const { stageFile, verifyBinaryRuns } = require('./stage-binary.cjs');

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
  return { release, check, targets, jobs: jobCount };
}

const hostPlatform = getPlatformSuffix();

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

/** Spawn with line-prefixed output so concurrent platform builds stay readable. */
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

/** The Cargo invocation for `platform` (null: the host) as `{ command, args }`. */
function hostsCommand(platform, { release = false, check = false }, dir) {
  const target = platform && PLATFORMS[platform];
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
    ...(release ? ['--release'] : ['--features', 'octocode-native/dev-unify']),
    ...(target ? ['--target', triple] : []),
    '--target-dir', dir,
  ];
  return { command, args };
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

async function buildPlatform(platform, explicitTarget, options) {
  const label = explicitTarget ? platform : 'host';
  const target = explicitTarget ? platform : null;
  const dir = targetDir(target);
  const { command, args } = hostsCommand(target, options, dir);
  await run(`${label}:${options.check ? 'check' : 'build'}`, command, args, process.env);
  if (options.check) return;

  const staged = stageBinary(platform, explicitTarget, options.release, dir);
  verifyStaged(platform, staged);
  for (const artifact of staged) console.log(`[${label}] staged ${relative(ROOT, artifact)}`);
}

/**
 * Concurrent platform builds. Each one ends in serial work (the engine's
 * codegen-units = 1, then thin LTO), so most cores idle under one build; memory
 * bounds them, at a few GB peak per release link. Measured 2026-10-25 on 12
 * cores / 32 GB, 8 platforms: 3 jobs 751 s (31% CPU), 4 jobs 521 s (45% CPU),
 * peak RSS 2.9-3.9 GB, no swap-outs.
 */
function defaultJobs(count) {
  const byCpu = Math.max(1, Math.floor(availableParallelism() / 2));
  const byMemory = Math.max(1, Math.floor(totalmem() / (8 * 1024 ** 3)));
  return Math.min(count, byCpu, byMemory);
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
else module.exports = { GLIBC_FLOOR, hostsCommand, targetDir };
