'use strict';

const assert = require('node:assert/strict');
const { spawn, spawnSync } = require('node:child_process');
const { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } = require('node:fs');
const { constants, tmpdir } = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const SCRIPT = path.join(__dirname, '..', 'scripts', 'build-slot.cjs');
const { COUNT_ENV, HELD_ENV, acquireSlot, slotCount, withSlot } = require(SCRIPT);

/** A private slot dir per test: no test ever takes or waits for the machine's real slots. */
function slotDir(t) {
  const root = mkdtempSync(path.join(tmpdir(), 'octocode-slot-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return path.join(root, 'octocode-build-slots');
}

const slotFiles = dir => (existsSync(dir) ? readdirSync(dir).filter(name => /^slot-\d+\.json$/.test(name)).sort() : []);
const record = (dir, n) => JSON.parse(readFileSync(path.join(dir, `slot-${n}.json`), 'utf8'));
const settledWithin = (promise, ms) => Promise.race([promise.then(() => true), new Promise(resolve => setTimeout(resolve, ms, false))]);

/** A child holding a slot under `tmp` (its os.tmpdir()), started by the CLI form. Resolves once the slot is taken. */
function cliHolder(tmp, script, extraEnv = {}) {
  const child = spawn(process.execPath, [SCRIPT, '--', process.execPath, '-e', script], {
    env: { ...process.env, TMPDIR: tmp, [HELD_ENV]: '', ...extraEnv },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const output = { stdout: '', stderr: '' };
  child.stdout.on('data', chunk => (output.stdout += chunk));
  child.stderr.on('data', chunk => (output.stderr += chunk));
  const exited = new Promise(resolve => child.on('close', (code, signal) => resolve({ code, signal, ...output })));
  const ready = new Promise(resolve => child.stdout.on('data', () => output.stdout.includes('held') && resolve()));
  return { child, exited, ready };
}

test('the count is 2 unless the one override sets it; a bad override is refused', () => {
  assert.equal(slotCount({}), 2);
  assert.equal(slotCount({ [COUNT_ENV]: '' }), 2);
  assert.equal(slotCount({ [COUNT_ENV]: '3' }), 3);
  for (const bad of ['0', '-1', '1.5', 'two']) assert.throws(() => slotCount({ [COUNT_ENV]: bad }), /positive integer/);
});

test('two holders at most; a third waits, names the holders, then takes the slot one releases', async t => {
  const dir = slotDir(t);
  const notices = [];
  const options = { dir, env: {}, pollMs: 10, noticeMs: 60_000, log: line => notices.push(line) };
  const first = await acquireSlot('first build', options);
  const second = await acquireSlot('second build', options);
  assert.deepEqual([first.slot, second.slot], [1, 2]);
  assert.deepEqual(slotFiles(dir), ['slot-1.json', 'slot-2.json']);
  const third = acquireSlot('third build', options);
  assert.equal(await settledWithin(third, 200), false, 'a third holder waits');
  // Twenty polls, one notice: it repeats at most once per noticeMs.
  assert.equal(notices.length, 1);
  assert.match(notices[0], /"third build" waits for one of 2 machine-wide Rust build slots/);
  assert.match(notices[0], new RegExp(`slot 1: pid ${process.pid} "first build" in .+, for \\d+s`));
  assert.match(notices[0], /slot 2: pid \d+ "second build"/);
  first.release();
  const taken = await third;
  assert.equal(taken.slot, 1);
  assert.equal(record(dir, 1).label, 'third build');
  assert.match(notices.at(-1), /"third build" took slot 1 after \d+s/);
  second.release();
  taken.release();
  taken.release();
  assert.deepEqual(slotFiles(dir), []);
});

test('a slot whose owner died is reclaimed', async t => {
  const dir = slotDir(t);
  const options = { dir, env: {}, pollMs: 10, log: () => {} };
  const live = await acquireSlot('live', options);
  const dead = spawnSync(process.execPath, ['-e', '']).pid;
  writeFileSync(path.join(dir, 'slot-2.json'), JSON.stringify({ pid: dead, label: 'killed build', cwd: '/', since: new Date().toISOString(), token: 'gone' }));
  const taken = await acquireSlot('next', options);
  assert.equal(taken.slot, 2);
  assert.equal(record(dir, 2).label, 'next');
  live.release();
  taken.release();
});

test('a pid now running another process does not keep a slot', { skip: process.platform === 'win32' }, async t => {
  const dir = slotDir(t);
  // Our own pid, recorded with another start time: the recorded owner is gone and its pid reused.
  const options = { dir, env: { [COUNT_ENV]: '1' }, pollMs: 10, log: () => {} };
  const holder = await acquireSlot('holder', options);
  const stale = { ...record(dir, 1), start: 'Thu Jan  1 00:00:00 1970', token: 'reused' };
  holder.release();
  writeFileSync(path.join(dir, 'slot-1.json'), JSON.stringify(stale));
  const taken = await acquireSlot('next', options);
  assert.equal(record(dir, 1).label, 'next');
  taken.release();
});

test('an unreadable record does not keep a slot', async t => {
  const dir = slotDir(t);
  const options = { dir, env: { [COUNT_ENV]: '1' }, pollMs: 10, log: () => {} };
  (await acquireSlot('make the dir', options)).release();
  writeFileSync(path.join(dir, 'slot-1.json'), '{"pid":0}');
  const taken = await acquireSlot('next', options);
  assert.equal(record(dir, 1).label, 'next');
  taken.release();
});

test('withSlot releases when the build throws', async t => {
  const dir = slotDir(t);
  await assert.rejects(
    withSlot('failing build', async () => {
      assert.deepEqual(slotFiles(dir), ['slot-1.json']);
      throw new Error('compile error');
    }, { dir, env: {}, log: () => {} }),
    /compile error/,
  );
  assert.deepEqual(slotFiles(dir), []);
});

test('a holder under an inherited slot takes no second slot, and its jobs run one at a time', async t => {
  const dir = slotDir(t);
  const options = { dir, env: { [COUNT_ENV]: '1' }, pollMs: 10, log: () => {} };
  const outer = await acquireSlot('outer', options);
  const inner = { ...options, env: outer.env };
  const first = await acquireSlot('job 1', inner);
  const second = acquireSlot('job 2', inner);
  assert.equal(first.slot, 1);
  assert.equal(await settledWithin(second, 100), false, 'job 2 waits for job 1');
  first.release();
  assert.equal((await second).slot, 1);
  (await second).release();
  assert.deepEqual(slotFiles(dir), ['slot-1.json'], 'the outer holder keeps its slot');
  outer.release();
  // A token whose holder released names no slot: the next holder takes one of its own.
  const after = await acquireSlot('after', { ...inner });
  assert.notEqual(record(dir, 1).token, outer.env[HELD_ENV]);
  after.release();
});

test('the CLI runs the command in a slot, passes its exit code, and a nested wrapper inherits the slot', async t => {
  const tmp = path.dirname(slotDir(t));
  const nested = `const r = require('node:child_process').spawnSync(process.execPath, [${JSON.stringify(SCRIPT)}, '--', process.execPath, '-e', 'process.exit(3)'], { stdio: 'inherit' }); console.log('held'); process.exit(r.status);`;
  const { exited } = cliHolder(tmp, nested, { [COUNT_ENV]: '1' });
  const result = await exited;
  assert.equal(result.code, 3, result.stderr);
  assert.equal(result.stderr, '', 'nothing waited');
  assert.deepEqual(slotFiles(path.join(tmp, 'octocode-build-slots')), []);
});

test('the CLI reports a command that cannot start and releases its slot', t => {
  const tmp = path.dirname(slotDir(t));
  const result = spawnSync(process.execPath, [SCRIPT, '--', path.join(tmp, 'no-such-command')], { env: { ...process.env, TMPDIR: tmp, [HELD_ENV]: '' }, encoding: 'utf8' });
  assert.equal(result.status, 127);
  assert.match(result.stderr, /build-slot: .*no-such-command/);
  assert.deepEqual(slotFiles(path.join(tmp, 'octocode-build-slots')), []);
  assert.equal(spawnSync(process.execPath, [SCRIPT, 'cargo']).status, 2, 'the command follows --');
});

test('SIGTERM stops the command and releases the slot; a waiter then proceeds', { skip: process.platform === 'win32' }, async t => {
  const tmp = path.dirname(slotDir(t));
  const dir = path.join(tmp, 'octocode-build-slots');
  const env = { [COUNT_ENV]: '1' };
  const holder = cliHolder(tmp, "console.log('held'); setInterval(() => {}, 1000);", env);
  await holder.ready;
  assert.equal(record(dir, 1).pid, holder.child.pid);
  const waiter = cliHolder(tmp, "console.log('held');", env);
  await new Promise(resolve => waiter.child.stderr.once('data', resolve));
  assert.equal(await settledWithin(waiter.exited, 300), false, 'the waiter waits');
  holder.child.kill('SIGTERM');
  const stopped = await holder.exited;
  assert.equal(stopped.code, 128 + constants.signals.SIGTERM);
  const after = await waiter.exited;
  assert.equal(after.code, 0);
  assert.match(after.stderr, new RegExp(`slot 1: pid ${holder.child.pid} "${process.execPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} -e`));
  assert.match(after.stderr, /took slot 1 after/);
  assert.deepEqual(slotFiles(dir), []);
});

test('SIGINT while waiting exits without taking a slot', { skip: process.platform === 'win32' }, async t => {
  const tmp = path.dirname(slotDir(t));
  const dir = path.join(tmp, 'octocode-build-slots');
  const env = { [COUNT_ENV]: '1' };
  const holder = cliHolder(tmp, "console.log('held'); setInterval(() => {}, 1000);", env);
  await holder.ready;
  const waiter = cliHolder(tmp, "console.log('held');", env);
  await new Promise(resolve => waiter.child.stderr.once('data', resolve));
  waiter.child.kill('SIGINT');
  assert.equal((await waiter.exited).code, 128 + constants.signals.SIGINT);
  assert.equal(record(dir, 1).pid, holder.child.pid);
  holder.child.kill('SIGTERM');
  await holder.exited;
  assert.deepEqual(slotFiles(dir), []);
});

test('a slot hands its children its share of the cores, unless the caller chose', () => {
  const { shareCores, SHARE_ENV } = require('../scripts/build-slot.cjs');
  const shared = shareCores({ PATH: '/bin' }, 2, 12);
  for (const name of SHARE_ENV) assert.equal(shared[name], '6', name);
  assert.equal(shared.PATH, '/bin');
  // The caller's own choice stands; an empty value counts as unset.
  const chosen = shareCores({ CARGO_BUILD_JOBS: '3', NEXTEST_TEST_THREADS: '' }, 2, 12);
  assert.equal(chosen.CARGO_BUILD_JOBS, '3');
  assert.equal(chosen.NEXTEST_TEST_THREADS, '6');
  // Never below one, however many slots share the machine.
  assert.equal(shareCores({}, 8, 4).RUST_TEST_THREADS, '1');
});
