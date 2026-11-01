#!/usr/bin/env node
'use strict';

/**
 * The Rust test selections (packages and features), the one source for them,
 * and their runner:
 *
 *   rust-tests.cjs                                `test:rust`: each selection under nextest
 *   rust-tests.cjs --coverage [report args...]    `coverage:rust`: each selection under
 *                                                 cargo-llvm-cov, then one merged report
 *                                                 (`--html`, `--lcov --output-path lcov.info`)
 *
 * `bacon.toml` test jobs and the `cargo dev-test` alias (`.cargo/config.toml`)
 * repeat these selections; tests/build-publication.test.cjs keeps them equal.
 *
 * Runner decision (audit 2026-10-21, F2): `test:rust` and CI stay on nextest,
 * and the local loop runs libtest through `cargo dev-test`, which carries the
 * first selection, so both share one build.
 * - libtest runs a test binary in one process: about 65 s for the selection
 *   against 266 s under nextest, whose 3,600 processes each repeat the
 *   per-process start-up (the first GitHub client, contract parse, regex init).
 * - nextest stays the gate: a process per test isolates process-global state
 *   (environment, `GitHubBudget::global`, thread-locals) and kills a hung test
 *   (`.config/nextest.toml` slow-timeout). In one shared process two
 *   timing-sensitive tests failed in 1 of 2 libtest runs, so a libtest gate
 *   would be flaky until those are fixed and the suite re-measured.
 */

const { spawnSync } = require('node:child_process');
const nodePath = require('node:path');

const SELECTIONS = [
  // Dev builds share these compiled crates with `build:dev` (crates/runtime `dev-unify`).
  ['--locked', '-p', 'octocode-native', '-p', 'octocode-github', '-p', 'octocode-io', '-p', 'octocode-cli', '--features', 'octocode-native/dev-unify'],
  ['--locked', '-p', 'octocode-engine', '--all-features'],
];

function cargo(args) {
  console.log(`$ cargo ${args.join(' ')}`);
  const { status, error } = spawnSync('cargo', args, { cwd: nodePath.join(__dirname, '..'), stdio: 'inherit' });
  if (error) throw error;
  if (status !== 0) process.exit(status ?? 1);
}

// Stale copies of our crates pile up in target/debug; agents run tests more often than builds.
async function cleanOversizedCache() {
  const { exceedsSize, NATIVE_DEBUG_LIMIT_BYTES, nativeDebugDir } = await import(
    '../../../skills-dev/octocode-dev/scripts/lib/native-target.mjs'
  );
  if (!exceedsSize(nativeDebugDir(), NATIVE_DEBUG_LIMIT_BYTES)) return;
  console.log('rust-tests: native target/debug is over 50 GiB, running clean:cache first');
  const { status, error } = spawnSync('yarn', ['clean:cache'], { cwd: nodePath.join(__dirname, '..'), stdio: 'inherit' });
  if (error) throw error;
  if (status !== 0) process.exit(status ?? 1);
}

async function main() {
  await cleanOversizedCache();
  const args = process.argv.slice(2);
  if (args[0] === '--coverage') {
    cargo(['llvm-cov', 'clean', '--workspace']);
    for (const selection of SELECTIONS) cargo(['llvm-cov', 'nextest', '--no-report', ...selection]);
    cargo(['llvm-cov', 'report', ...args.slice(1)]);
  } else if (args.length === 0) {
    for (const selection of SELECTIONS) cargo(['nextest', 'run', ...selection]);
  } else {
    // A filter matches nothing in one selection and nextest fails it: narrow with `bacon test -- <filter>` or `cargo dev-test -- <filter>`.
    console.error('Usage: rust-tests.cjs [--coverage [cargo llvm-cov report args...]]');
    process.exit(2);
  }
}

if (require.main === module) {
  main().catch(error => {
    console.error(error);
    process.exit(1);
  });
}

module.exports = { SELECTIONS };
