'use strict';

// rust-tests.cjs end to end with a fake `cargo` first on PATH: the run holds a build slot and
// gives every cargo child that runs tests one private TMPDIR, removed on success, failure and
// kill; the build keeps the caller's, so a daemon a compile starts never holds a removed root.
// The fake build names one test executable, as cargo's JSON messages do; the run starts it
// itself, under the watchdog, with a private TMPDIR of its own.

const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const SCRIPT = path.join(__dirname, '..', 'scripts', 'rust-tests.cjs');
const { HELD_ENV } = require('../scripts/build-slot.cjs');
const { BASE, PREFIX } = require('../scripts/test-tmpdir.cjs');

// Leaves what a killed test leaves (a file, and a directory a test locked), logs what it saw,
// then passes, fails the test runs, or hangs in them until a signal.
const FAKE_CARGO = `
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
const tmp = process.env.TMPDIR;
if (!args.includes('--no-run')) {
  fs.writeFileSync(path.join(tmp, 'left-' + process.pid), 'x');
  const locked = path.join(tmp, 'locked-' + process.pid);
  fs.mkdirSync(locked);
  fs.writeFileSync(path.join(locked, 'file'), 'x');
  fs.chmodSync(locked, 0o000);
}
const slots = fs.readdirSync(process.env.FAKE_SLOT_DIR).filter(name => name.endsWith('.json'))
  .map(name => JSON.parse(fs.readFileSync(path.join(process.env.FAKE_SLOT_DIR, name), 'utf8')).label);
fs.appendFileSync(process.env.FAKE_LOG, JSON.stringify({ pid: process.pid, args, tmp, slots }) + '\\n');
const testing = (args[0] === 'test' || args[1] === 'run') && !args.includes('--no-run');
if (testing && process.env.FAKE_MODE === 'fail') process.exit(1);
if (testing && process.env.FAKE_MODE === 'hang') setInterval(() => {}, 1000);
if (args[0] === 'test' && args.includes('--no-run')) {
  const dir = path.dirname(process.env.FAKE_BINARY);
  console.log(JSON.stringify({ reason: 'compiler-artifact', package_id: 'path+file://' + dir + '#octocode-fake@1.0.0', manifest_path: path.join(dir, 'Cargo.toml'), target: { kind: ['lib'], name: 'octocode_fake' }, profile: { test: true }, executable: process.env.FAKE_BINARY, filenames: [], fresh: true }));
}
`;

// A test binary: logs what it saw, then passes, fails or hangs as FAKE_MODE says.
const FAKE_BINARY = `
const fs = require('node:fs');
fs.appendFileSync(process.env.FAKE_LOG, JSON.stringify({ binary: true, pid: process.pid, args: process.argv.slice(2), tmp: process.env.TMPDIR, cwd: process.cwd(), manifest: process.env.CARGO_MANIFEST_DIR }) + '\\n');
if (process.env.FAKE_MODE === 'fail') process.exit(1);
if (process.env.FAKE_MODE === 'hang') setInterval(() => {}, 1000);
else console.log('test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s');
`;

function setup(t, mode) {
  const root = mkdtempSync(path.join(tmpdir(), 'octocode-rust-tests-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = path.join(root, 'bin');
  mkdirSync(bin);
  writeFileSync(path.join(bin, 'cargo'), `#!${process.execPath}\n${FAKE_CARGO}`, { mode: 0o755 });
  const binary = path.join(root, 'crate', 'fake-binary');
  mkdirSync(path.dirname(binary));
  writeFileSync(binary, `#!${process.execPath}\n${FAKE_BINARY}`, { mode: 0o755 });
  const slotDir = path.join(root, 'octocode-build-slots');
  const log = path.join(root, 'cargo.log');
  writeFileSync(log, '');
  const env = { ...process.env, PATH: `${bin}${path.delimiter}${process.env.PATH}`, TMPDIR: root, [HELD_ENV]: '', FAKE_MODE: mode, FAKE_LOG: log, FAKE_SLOT_DIR: slotDir, FAKE_BINARY: binary };
  const records = () => readFileSync(log, 'utf8').split('\n').filter(Boolean).map(line => JSON.parse(line));
  const calls = () => records().filter(record => !record.binary);
  const binaries = () => records().filter(record => record.binary);
  const slots = () => (existsSync(slotDir) ? readdirSync(slotDir).filter(name => name.endsWith('.json')) : []);
  return { env, calls, binaries, slots, binary };
}

function run(env, args = []) {
  const child = spawn(process.execPath, [SCRIPT, ...args], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '';
  for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => (output += chunk));
  const exited = new Promise(resolve => child.on('close', code => resolve({ code, output })));
  return { child, exited };
}

/** The build saw the caller's TMPDIR inside a held slot; the calls after it are the test runs. */
function testRuns(calls, env, label) {
  const [built, ...runs] = calls;
  assert.ok(built.args.includes('--no-run'), built.args.join(' '));
  assert.equal(built.tmp, env.TMPDIR, 'the build keeps the caller TMPDIR');
  assert.deepEqual(built.slots, [label]);
  return runs;
}

/** Every call saw the same fresh root under BASE, inside a held slot; the root is gone now. */
function assertOneRemovedRoot(calls, label) {
  const roots = new Set(calls.map(call => call.tmp));
  assert.equal(roots.size, 1, [...roots].join(', '));
  const [root] = roots;
  assert.ok(root.startsWith(path.join(BASE, PREFIX)), root);
  assert.equal(existsSync(root), false, `${root} removed`);
  for (const call of calls) assert.deepEqual(call.slots, [label]);
}

const unix = { skip: process.platform === 'win32' };

test('test:rust passes one temp root to every test run, not the build, and removes it after a pass', unix, async t => {
  const { env, calls, binaries, slots, binary } = setup(t, 'pass');
  const { code, output } = await run(env).exited;
  assert.equal(code, 0, output);
  const seen = calls();
  // The build first, then the nextest run beside the test binaries the build named.
  assert.deepEqual(seen[0].args.slice(0, 4), ['test', '--no-run', '--message-format', 'json-render-diagnostics']);
  assert.deepEqual(seen.slice(1).map(call => call.args.slice(0, 3)), [['nextest', 'run', '--cargo-message-format']]);
  assertOneRemovedRoot(testRuns(seen, env, 'test:rust'), 'test:rust');
  // The binary ran as cargo runs it, from its package, skipping the process tests, under a temp root of its own.
  const [ran] = binaries();
  assert.equal(ran.cwd, require('node:fs').realpathSync(path.dirname(binary)));
  assert.equal(ran.manifest, path.dirname(binary));
  const { PROCESS_TESTS } = require('../scripts/rust-tests.cjs');
  assert.deepEqual(ran.args.slice(0, PROCESS_TESTS.length * 2), PROCESS_TESTS.flatMap(name => ['--skip', name]));
  assert.ok(ran.args.includes('--test-threads'), ran.args.join(' '));
  assert.ok(ran.tmp.startsWith(path.join(BASE, PREFIX)), ran.tmp);
  assert.equal(existsSync(ran.tmp), false, `${ran.tmp} removed`);
  assert.match(output, /binaries: 1 in [\d.]+ s .*2 passed, 0 failed/);
  assert.deepEqual(slots(), [], 'the slot is released');
});

test('test:dev runs only the crates it names, with the libtest arguments it was given', unix, async t => {
  const { env, calls, binaries, slots } = setup(t, 'pass');
  const durations = path.join(__dirname, '..', 'target', '.keep', 'test-durations.json');
  const recorded = () => (existsSync(durations) ? readFileSync(durations, 'utf8') : null);
  const before = recorded();
  const named = await run(env, ['dev', '--crate', 'fake', '--', 'a_filter']).exited;
  assert.equal(named.code, 0, named.output);
  assert.equal(recorded(), before, 'a filtered run leaves the full run durations as they were');
  assert.deepEqual(calls().map(call => call.args.slice(0, 2)), [['test', '--no-run']]);
  testRuns(calls(), env, 'test:dev');
  assert.deepEqual(binaries().map(record => record.args[0]), ['a_filter']);
  const unknown = await run(env, ['dev', '--crate', 'no-such-crate']).exited;
  assert.equal(unknown.code, 2, unknown.output);
  assert.match(unknown.output, /Unknown crate no-such-crate\. Crates: octocode-fake/);
  assert.equal(binaries().length, 1, 'an unknown crate runs nothing');
  assert.deepEqual(slots(), []);
});

test('a failing run removes the temp root too, locked directories included', unix, async t => {
  const { env, calls, slots } = setup(t, 'fail');
  const { code, output } = await run(env).exited;
  assert.notEqual(code, 0, output);
  assertOneRemovedRoot(testRuns(calls(), env, 'test:rust'), 'test:rust');
  assert.deepEqual(slots(), []);
});

test('a killed run stops its cargo children and test binaries and removes the temp roots', unix, async t => {
  const { env, calls, binaries, slots } = setup(t, 'hang');
  const { child, exited } = run(env);
  while (calls().length < 2 || binaries().length < 1) await new Promise(resolve => setTimeout(resolve, 20));
  child.kill('SIGTERM');
  const { code, output } = await exited;
  assert.notEqual(code, 0, output);
  const seen = calls();
  assertOneRemovedRoot(testRuns(seen, env, 'test:rust'), 'test:rust');
  for (const { pid } of seen) assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, `cargo ${pid} stopped`);
  for (const { pid, tmp } of binaries()) {
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, `test binary ${pid} stopped`);
    assert.equal(existsSync(tmp), false, `${tmp} removed`);
  }
  assert.deepEqual(slots(), []);
});

test('coverage:rust runs every step with the temp root and removes it', unix, async t => {
  const { env, calls, slots } = setup(t, 'pass');
  const { code, output } = await run(env, ['--coverage', '--html']).exited;
  assert.equal(code, 0, output);
  const seen = calls();
  assert.deepEqual(seen.map(call => call.args.slice(0, 2)), [['llvm-cov', 'clean'], ['llvm-cov', 'nextest'], ['llvm-cov', 'report']]);
  assert.deepEqual(seen.at(-1).args, ['llvm-cov', 'report', '--html']);
  assertOneRemovedRoot(seen, 'coverage:rust');
  assert.deepEqual(slots(), []);
});

test('removing a test temp root stops the processes the run left working inside it', { skip: process.platform === 'win32' }, async () => {
  const { testTmpdir } = require('../scripts/test-tmpdir.cjs');
  const root = testTmpdir();
  const work = require('node:path').join(root.dir, 'work');
  require('node:fs').mkdirSync(work);
  // A detached leftover, as a test's daemon would be: it survives its parent.
  const leftover = spawn('sleep', ['60'], { cwd: work, detached: true, stdio: 'ignore' });
  leftover.unref();
  const exited = new Promise(resolve => leftover.once('exit', (code, signal) => resolve(signal)));
  // An unrelated process outside the root keeps running.
  const outside = spawn('sleep', ['60'], { stdio: 'ignore' });
  try {
    root.remove();
    assert.equal(await exited, 'SIGTERM');
    assert.equal(require('node:fs').existsSync(root.dir), false);
    assert.equal(outside.exitCode, null, 'a process outside the root is left alone');
  } finally {
    outside.kill();
  }
});
