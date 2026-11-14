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
 *   would be flaky until those are fixed and the suite re-measured. `cargo dev-test`
 *   runs under scripts/test-watchdog.cjs, so a hung test fails there too.
 */

const { spawnSync } = require('node:child_process');
const nodePath = require('node:path');
const { finishCargoRun, parseCargoLine } = require('./target-hygiene.cjs');

const SELECTIONS = [
  // Dev builds share these compiled crates with `build:dev` (crates/runtime `dev-unify`).
  ['--locked', '-p', 'octocode-native', '-p', 'octocode-github', '-p', 'octocode-io', '-p', 'octocode-cli', '--features', 'octocode-native/dev-unify'],
  ['--locked', '-p', 'octocode-engine', '--all-features'],
];

const NATIVE = nodePath.join(__dirname, '..');

function runCargo(args) {
  console.log(`$ cargo ${args.join(' ')}`);
  const { status, error } = spawnSync('cargo', args, { cwd: NATIVE, stdio: 'inherit' });
  if (error) throw error;
  return status ?? 1;
}

/**
 * One selection under nextest. Cargo's JSON messages (stdout) name every unit the
 * selection uses, so target-hygiene.cjs records them and sweeps what the run left stale.
 */
function runSelection(selection) {
  const args = ['nextest', 'run', '--cargo-message-format', 'json-render-diagnostics', ...selection];
  console.log(`$ cargo ${args.join(' ')}`);
  const startedMs = Date.now();
  const { status, error, stdout } = spawnSync('cargo', args, {
    cwd: NATIVE,
    stdio: ['inherit', 'pipe', 'inherit'],
    encoding: 'utf8',
    maxBuffer: 2 ** 31,
  });
  if (error) throw error;
  const messages = [];
  for (const line of (stdout ?? '').split('\n')) {
    const rest = line && parseCargoLine(line, messages);
    if (rest) console.log(rest);
  }
  const packageName = selection[selection.indexOf('-p') + 1];
  finishCargoRun({ selection: `test-${packageName}`, messages, success: status === 0, startedMs });
  return status ?? 1;
}

function cargo(args) {
  const status = runCargo(args);
  if (status !== 0) process.exit(status);
}

function main() {
  const args = process.argv.slice(2);
  if (args[0] === '--coverage') {
    cargo(['llvm-cov', 'clean', '--workspace']);
    for (const selection of SELECTIONS) cargo(['llvm-cov', 'nextest', '--no-report', ...selection]);
    cargo(['llvm-cov', 'report', ...args.slice(1)]);
  } else if (args.length === 0) {
    // Every selection reports: a failure in the first must not hide the second's.
    const failed = SELECTIONS.map(runSelection).find(status => status !== 0);
    if (failed !== undefined) process.exit(failed);
  } else {
    // A filter matches nothing in one selection and nextest fails it: narrow with `bacon test -- <filter>` or `cargo dev-test -- <filter>`.
    console.error('Usage: rust-tests.cjs [--coverage [cargo llvm-cov report args...]]');
    process.exit(2);
  }
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error);
    process.exit(1);
  }
}

module.exports = { SELECTIONS };
