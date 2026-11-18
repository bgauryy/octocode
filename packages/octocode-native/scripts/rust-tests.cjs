#!/usr/bin/env node
'use strict';

/**
 * The Rust test selections (packages and features), the one source for them,
 * and their runner:
 *
 *   rust-tests.cjs                                `test:rust`: one build, then the selection
 *                                                 under libtest without PROCESS_TESTS, and
 *                                                 PROCESS_TESTS under nextest beside it
 *   rust-tests.cjs dev [--crate <name>]... [--lib] [--] [libtest args...]
 *                                                 `test:dev`: the same build, then only the named
 *                                                 crates' binaries (every crate without --crate;
 *                                                 only unit-test binaries with --lib), side by side
 *   rust-tests.cjs --coverage [report args...]    `coverage:rust`: the selections under
 *                                                 cargo-llvm-cov, then one merged report
 *                                                 (`--html`, `--lcov --output-path lcov.info`)
 *
 * `bacon.toml` test jobs and the `cargo dev-test` alias (`.cargo/config.toml`)
 * repeat these selections; tests/build-publication.test.cjs keeps them equal.
 *
 * Runner decision (2026-11-18, replacing the one-after-another `cargo test` run of 2026-11-17):
 * - libtest is the gate for every test that can share a process: it runs a test
 *   binary in one process, so the per-process start-up (the first GitHub client,
 *   contract parse, regex init) is paid once per binary instead of once per test.
 * - One build (`cargo test --no-run`, JSON messages) names every test executable;
 *   test-binaries.cjs runs them side by side, slowest first. `cargo test` ran them
 *   one after another, each waiting on its slowest tests while its other threads
 *   idled: 3.1 of 6 threads busy (910 CPU s in 296 s, 2026-11-18).
 *   Tests share no process-global state: a test hook is thread-local or a guard
 *   that restores it, and a test forgets only what its own calls kept (a workspace
 *   root of its own is a store key of its own). libtest has no per-test timeout, so
 *   each binary goes through scripts/test-watchdog.cjs, which fails a stuck test.
 * - nextest runs PROCESS_TESTS, a process per test, and only them: the tests that
 *   start real child processes (fake language servers, git, the daemon, the MCP
 *   server, the provider fixtures). On macOS a pipe is created without close-on-exec
 *   and marked a moment later, so a child another test of the same process spawns
 *   in that moment inherits its end and holds it open; a process each rules that
 *   out. nextest also kills such a test when it hangs (`.config/nextest.toml`
 *   slow-timeout), and its `process-heavy` group bounds how many start at once.
 */

const { spawn } = require('node:child_process');
const nodePath = require('node:path');
const { runForwardingSignals, takeSignals } = require('../bin/launch-native.cjs');
const { exitOnSignals, withSlot } = require('./build-slot.cjs');
const { finishCargoRun, parseCargoLine } = require('./target-hygiene.cjs');
const { testTmpdir } = require('./test-tmpdir.cjs');
const { crateNamed, runBinaries, testExecutables } = require('./test-binaries.cjs');

const SELECTIONS = [
  // Every workspace crate in one selection: `dev-unify` (crates/runtime) gives its
  // non-test units the features `build:dev` compiles, so the tests reuse that build
  // and no crate compiles a second copy for a narrower selection.
  ['--locked', '-p', 'octocode-native', '-p', 'octocode-runtime-base', '-p', 'octocode-tools-shared', '-p', 'octocode-tool-graph', '-p', 'octocode-tool-lsp', '-p', 'octocode-tool-github', '-p', 'octocode-tool-local', '-p', 'octocode-tool-ast', '-p', 'octocode-tool-packages', '-p', 'octocode-tool-clasify', '-p', 'octocode-github', '-p', 'octocode-io', '-p', 'octocode-cli', '-p', 'octocode-engine', '-p', 'octocode-contract-types', '-p', 'octocode-test-support', '--features', 'octocode-native/dev-unify'],
];

// What nextest runs, one process per test: each is a literal piece of a test's name. libtest's
// `--skip` and nextest's `test(/…/)` both match it anywhere in the name, so the two runs split
// the selection exactly, every test in one of them. A unit test whose name happens to hold one
// runs under nextest too, at a process's cost.
const PROCESS_TESTS = [
  'lsp_search::process_tests',
  'loading_server',
  'gh_clone',
  'warm_server::',
  'mcp_server::',
  'cli_exit_codes::providers::',
  // Tests whose children live past a quick exit, or whose result waits on a child's pipe or
  // clock. On macOS a pipe or socket is marked close-on-exec only after it is made, so a
  // child any sibling test spawns in that gap inherits it: a long-lived child then holds
  // another test's pipe open (a read to EOF hangs) or its listener answering. Short fixture
  // commands (git, mkfifo) stay in the unit run: they exit at once and hold nothing.
  // (2026-11-18: stderr_ring failed under the side-by-side unit run, 30 s initialize
  // timeout; one rustup test failed there, each passing alone.)
  // A node fake language server per test, through the client or the runtime.
  'lsp::client::tests::',
  'runtime_local::lsp_ast',
  'runtime_local::lsp_rows',
  'runtime_one_walk::channels',
  // Process supervision: children held, limited, killed and reaped.
  'process::tests::live',
  'lsp::spawn_limits::tests',
  'bounded_process::tests',
  // Fake servers, daemons and windows a test starts and signals (2026-11-18: the live checks'
  // fake servers missed their 30 s initialize and the stand-in browser its 15 s start under the
  // side-by-side unit run; each passes alone in about a second).
  'serve::unix::tests::startup',
  'serve_signal',
  'a_live_check_',
  'config_view::browser::tests::the_window_runs_clean',
  // Probes and credential helpers run under a deadline of their own.
  'lsp::config::tests::probe',
  'lsp::config::tests::rustup_memo',
  'lsp::config::tests::rustup_toolchain_path',
  'a_rustup_proxy_resolves_only_when_the_toolchain_rustup_selects_ships_the_component',
  'a_toolchain_matched_rustup_component_wins_over_the_managed_install',
  'auth::discovery::tests',
  'auth_discovery::',
  // Writers re-executed from this test binary, sharing one disk budget.
  'cache::tests::capacity::independent_processes_share_one_disk_budget',
  // Runs real git against a local HTTP server (2026-11-18: hung 600 s under the side-by-side
  // unit run; the server now stops waiting at git's deadline).
  'gh_git::process::tests',
];
const PROCESS_FILTER = `test(/${PROCESS_TESTS.join('|')}/)`;

// The libtest run: every binary of the selection without PROCESS_TESTS, all run (`--no-fail-fast`).
const UNIT = { name: 'unit', keep: 'octocode-native', selection: SELECTIONS[0], skip: PROCESS_TESTS };

// The nextest run beside it, on the same build. Its tests mostly wait on the child processes
// they start, so it takes 8 threads beside the unit binaries; the process-heavy group
// (.config/nextest.toml, 2 threads a test) lets 4 run at once. At 4 threads (2 at once) it
// was the run's longest part: 170-230 s, against 81 s at 8 (2026-11-18).
const RUNS = [
  { name: 'process', keep: 'octocode-native', selection: SELECTIONS[0], extra: ['-E', PROCESS_FILTER, '--test-threads', '8'] },
];

const NATIVE = nodePath.join(__dirname, '..');

/** The one build of `test:rust` and `test:dev`: every test executable of the selection, named in cargo's JSON messages. */
const buildArgs = ({ selection }) => ['test', '--no-run', '--message-format', 'json-render-diagnostics', ...selection];

/** The libtest arguments of the unit run: every test but PROCESS_TESTS. */
const unitTestArgs = ({ skip }) => skip.flatMap(name => ['--skip', name]);

/** How many test binaries run at once (OCTOCODE_TEST_BINARIES), and the threads each gets (the slot's RUST_TEST_THREADS). */
function budget(env) {
  const threads = Number(env.RUST_TEST_THREADS) || Math.max(1, Math.floor(require('node:os').availableParallelism() / 2));
  const concurrency = Number(env.OCTOCODE_TEST_BINARIES) || 3;
  return { threads, concurrency };
}

const DURATIONS = nodePath.join(NATIVE, 'target', '.keep', 'test-durations.json');

/** The cargo arguments of one nextest run; cargo's JSON messages feed target-hygiene.cjs. */
const runArgs = ({ selection, extra }) => ['nextest', 'run', '--cargo-message-format', 'json-render-diagnostics', '--no-fail-fast', ...selection, ...extra];

/** The cargo steps of `coverage:rust`: every selection under cargo-llvm-cov, then one merged report. */
const coverageSteps = reportArgs => [
  ['llvm-cov', 'clean', '--workspace'],
  ...SELECTIONS.map(selection => ['llvm-cov', 'nextest', '--no-report', ...selection]),
  ['llvm-cov', 'report', ...reportArgs],
];

/**
 * The build: human output passes through, cargo's JSON messages are kept, recorded for
 * target-hygiene.cjs, and returned with the status.
 */
function build(args, env, children) {
  const startedMs = Date.now();
  console.log(`$ cargo ${args.join(' ')}`);
  return new Promise(resolve => {
    const child = spawn('cargo', args, { cwd: NATIVE, env, stdio: ['ignore', 'pipe', 'inherit'] });
    children.add(child);
    const messages = [];
    let partial = '';
    const read = line => {
      const rest = line && parseCargoLine(line, messages);
      if (rest) console.log(rest);
    };
    child.stdout.on('data', chunk => {
      const lines = (partial + chunk.toString('utf8')).split('\n');
      partial = lines.pop();
      lines.forEach(read);
    });
    const done = status => {
      children.delete(child);
      read(partial);
      finishCargoRun({ selection: 'test-octocode-native', messages, success: status === 0, startedMs });
      resolve({ status, messages });
    };
    child.on('error', error => {
      console.error(error);
      done(1);
    });
    child.on('close', code => done(code ?? 1));
  });
}

/** Runs cargo steps in order with inherited output; resolves the first failing status, else 0. */
async function runSteps(steps, env) {
  for (const args of steps) {
    console.log(`$ cargo ${args.join(' ')}`);
    const status = await runForwardingSignals('cargo', args, { env });
    if (status !== 0) return status;
  }
  return 0;
}

/**
 * One run of `args`. Cargo's JSON messages (stdout) name every unit the selection uses, so
 * target-hygiene.cjs records them and sweeps what the run left stale. Output is collected
 * and printed whole when the run ends, so concurrent runs do not interleave.
 */
function runOne(name, keep, args, env, children) {
  const startedMs = Date.now();
  return new Promise(resolve => {
    const child = spawn('cargo', args, { cwd: NATIVE, env, stdio: ['ignore', 'pipe', 'pipe'] });
    children.add(child);
    const out = [];
    const err = [];
    child.stdout.on('data', chunk => out.push(chunk));
    child.stderr.on('data', chunk => err.push(chunk));
    child.on('error', error => {
      children.delete(child);
      console.error(error);
      resolve(1);
    });
    child.on('close', code => {
      children.delete(child);
      const messages = [];
      const lines = [`$ cargo ${args.join(' ')}`];
      for (const line of Buffer.concat(out).toString('utf8').split('\n')) {
        const rest = line && parseCargoLine(line, messages);
        if (rest) lines.push(rest);
      }
      console.log(`=== ${name} (${((Date.now() - startedMs) / 1000).toFixed(1)} s) ===\n${lines.join('\n')}`);
      process.stderr.write(Buffer.concat(err));
      const status = code ?? 1;
      finishCargoRun({ selection: `test-${keep}`, messages, success: status === 0, startedMs });
      resolve(status);
    });
  });
}

/** Signals reach every child, and each run still reports. */
async function withChildren(fn) {
  const children = new Set();
  const restore = takeSignals(signal => {
    for (const child of children) child.kill(signal);
  });
  try {
    return await fn(children);
  } finally {
    restore();
  }
}

async function runTests({ buildEnv, env }) {
  return withChildren(async children => {
    const built = await build(buildArgs(UNIT), buildEnv, children);
    if (built.status !== 0) return built.status;
    // Every run reports: a failure in one must not hide another's.
    const statuses = await Promise.all([
      runBinaries(testExecutables(built.messages), { args: unitTestArgs(UNIT), ...budget(env), env, children, durationsFile: DURATIONS }),
      ...RUNS.map(run => runOne(run.name, run.keep, runArgs(run), env, children)),
    ]);
    return statuses.find(status => status !== 0) ?? 0;
  });
}

/** `dev [--crate <name>]... [--lib] [--] [libtest args...]`: the crates named, whether only unit-test binaries run, and libtest's arguments. */
function devOptions(args) {
  const crates = [];
  let lib = false;
  const rest = [];
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--') {
      rest.push(...args.slice(index + 1));
      break;
    }
    if (arg === '--crate' && index + 1 < args.length) {
      index += 1;
      crates.push(args[index]);
    } else if (arg === '--lib') lib = true;
    else rest.push(arg);
  }
  return { crates, lib, rest };
}

/** The focused loop: the same build as test:rust, then only the binaries asked for, side by side. */
async function runDev(args, { buildEnv, env }) {
  const { crates, lib, rest } = devOptions(args);
  return withChildren(async children => {
    const built = await build(buildArgs(UNIT), buildEnv, children);
    if (built.status !== 0) return built.status;
    const binaries = testExecutables(built.messages);
    const known = new Set(binaries.map(binary => binary.crate));
    const wanted = new Set();
    for (const name of crates) {
      const crate = crateNamed(name, known);
      if (!crate) {
        console.error(`Unknown crate ${name}. Crates: ${[...known].sort().join(', ')}`);
        return 2;
      }
      wanted.add(crate);
    }
    const chosen = binaries.filter(binary => (wanted.size === 0 || wanted.has(binary.crate)) && (!lib || binary.kind === 'lib'));
    // All at once, so no order to keep; a filtered run's times are not the binaries' own, so
    // they never replace the full run's in DURATIONS, which orders test:rust.
    const { threads } = budget(env);
    return runBinaries(chosen, { args: rest, threads, concurrency: chosen.length, env, children });
  });
}

/**
 * The whole run holds a build slot (build-slot.cjs) and gives its test processes a private
 * TMPDIR (test-tmpdir.cjs), removed when the run ends. The build keeps the caller's TMPDIR:
 * a daemon a compile starts (an sccache server, through `build.rustc-wrapper`) outlives the
 * run, and with the private root as its temp dir every later compile through it fails once
 * the root is removed (2026-11-18: "Failed to create temp dir ... /tmp/octocode-rust-test-…").
 * The nextest run builds the same selection, already fresh, so it compiles nothing. The
 * coverage steps compile and test in one cargo-llvm-cov call each, so they keep the root.
 */
async function main(args) {
  const coverage = args[0] === '--coverage';
  const dev = args[0] === 'dev';
  if (args.length > 0 && !coverage && !dev) {
    // A filter matches nothing in one selection and nextest fails it: narrow with `dev.mjs test:dev -- <filter>`.
    console.error('Usage: rust-tests.cjs [dev [--crate <name>]... [--lib] [--] [libtest args...] | --coverage [cargo llvm-cov report args...]]');
    return 2;
  }
  process.chdir(NATIVE);
  exitOnSignals();
  return withSlot(coverage ? 'coverage:rust' : dev ? 'test:dev' : 'test:rust', async slot => {
    const tmp = testTmpdir();
    const envs = { buildEnv: slot.env, env: { ...slot.env, TMPDIR: tmp.dir } };
    try {
      return await (coverage ? runSteps(coverageSteps(args.slice(1)), envs.env) : dev ? runDev(args.slice(1), envs) : runTests(envs));
    } finally {
      tmp.remove();
    }
  });
}

if (require.main === module) {
  main(process.argv.slice(2)).then(
    code => process.exit(code),
    error => {
      console.error(error);
      process.exit(1);
    },
  );
}

module.exports = { SELECTIONS, PROCESS_TESTS, PROCESS_FILTER, UNIT, RUNS, buildArgs, coverageSteps, devOptions, runArgs, unitTestArgs };
