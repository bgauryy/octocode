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

const { spawn, spawnSync } = require('node:child_process');
const nodePath = require('node:path');
const { finishCargoRun, parseCargoLine } = require('./target-hygiene.cjs');

const SELECTIONS = [
  // Dev builds share these compiled crates with `build:dev` (crates/runtime `dev-unify`).
  ['--locked', '-p', 'octocode-native', '-p', 'octocode-github', '-p', 'octocode-io', '-p', 'octocode-cli', '--features', 'octocode-native/dev-unify'],
  ['--locked', '-p', 'octocode-engine', '--all-features'],
];

// The CLI tests that start the real daemon, the MCP server or the provider fixtures run
// under `--profile e2e` (`.config/nextest.toml`, whose default-filter is this set); the
// first selection's unit run skips them. Both runs are required.
const E2E_FILTER = 'binary(integration) & (test(/^warm_server::/) | test(/^mcp_server::/) | test(/^cli_exit_codes::providers::/))';

// What `main` runs under nextest: the unit run of each selection and the e2e run of the
// first (same packages and features, so one compiled copy). Engine and e2e take fewer
// threads: they finish well inside the first run and overlap its slow tail.
const RUNS = [
  { name: 'native', keep: 'octocode-native', selection: SELECTIONS[0], extra: ['-E', `not (${E2E_FILTER})`] },
  { name: 'engine', keep: 'octocode-engine', selection: SELECTIONS[1], extra: ['--test-threads', '4'] },
  { name: 'e2e', keep: 'octocode-native', selection: SELECTIONS[0], extra: ['--profile', 'e2e', '--test-threads', '4'] },
];

const NATIVE = nodePath.join(__dirname, '..');

function runCargo(args) {
  console.log(`$ cargo ${args.join(' ')}`);
  const { status, error } = spawnSync('cargo', args, { cwd: NATIVE, stdio: 'inherit' });
  if (error) throw error;
  return status ?? 1;
}

/** Compile every selection once, one after the other, so the concurrent runs find a finished build. */
function buildAll() {
  for (const selection of SELECTIONS) {
    const status = runCargo(['nextest', 'run', '--no-run', ...selection]);
    if (status !== 0) process.exit(status);
  }
}

/**
 * One run under nextest. Cargo's JSON messages (stdout) name every unit the selection
 * uses, so target-hygiene.cjs records them and sweeps what the run left stale. Output is
 * collected and printed whole when the run ends, so concurrent runs do not interleave.
 */
function runOne({ name, keep, selection, extra }) {
  const args = ['nextest', 'run', '--cargo-message-format', 'json-render-diagnostics', ...selection, ...extra];
  const startedMs = Date.now();
  return new Promise(resolve => {
    const child = spawn('cargo', args, { cwd: NATIVE, stdio: ['ignore', 'pipe', 'pipe'] });
    const out = [];
    const err = [];
    child.stdout.on('data', chunk => out.push(chunk));
    child.stderr.on('data', chunk => err.push(chunk));
    child.on('error', error => {
      console.error(error);
      resolve(1);
    });
    child.on('close', code => {
      const messages = [];
      const lines = [`$ cargo ${args.join(' ')}`];
      for (const line of Buffer.concat(out).toString('utf8').split('\n')) {
        const rest = line && parseCargoLine(line, messages);
        if (rest) lines.push(rest);
      }
      console.log(`=== ${name} ===\n${lines.join('\n')}`);
      process.stderr.write(Buffer.concat(err));
      const status = code ?? 1;
      finishCargoRun({ selection: `test-${keep}`, messages, success: status === 0, startedMs });
      resolve(status);
    });
  });
}

function cargo(args) {
  const status = runCargo(args);
  if (status !== 0) process.exit(status);
}

async function main() {
  const args = process.argv.slice(2);
  if (args[0] === '--coverage') {
    cargo(['llvm-cov', 'clean', '--workspace']);
    for (const selection of SELECTIONS) cargo(['llvm-cov', 'nextest', '--no-report', ...selection]);
    cargo(['llvm-cov', 'report', ...args.slice(1)]);
  } else if (args.length === 0) {
    // Every run reports: a failure in one must not hide another's.
    buildAll();
    const failed = (await Promise.all(RUNS.map(runOne))).find(status => status !== 0);
    if (failed !== undefined) process.exit(failed);
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

module.exports = { SELECTIONS, E2E_FILTER, RUNS };
