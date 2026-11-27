'use strict';

const assert = require('node:assert/strict');
const { spawn, spawnSync } = require('node:child_process');
const { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { BASE, PREFIX } = require('../scripts/test-tmpdir.cjs');

const WATCHDOG = path.join(__dirname, '..', 'scripts', 'test-watchdog.cjs');
// With FAKE_TMPDIR_LOG set, it names its TMPDIR there and leaves what a killed test leaves.
const FAKE_RUN = [
  '#!/usr/bin/env node',
  "const fs = require('node:fs');",
  'if (process.env.FAKE_TMPDIR_LOG) {',
  '  fs.writeFileSync(process.env.FAKE_TMPDIR_LOG, process.env.TMPDIR);',
  "  fs.mkdirSync(process.env.TMPDIR + '/.tmpLEFT/locked', { recursive: true });",
  "  fs.writeFileSync(process.env.TMPDIR + '/.tmpLEFT/locked/file', 'x');",
  "  fs.chmodSync(process.env.TMPDIR + '/.tmpLEFT/locked', 0o000);",
  '}',
  "console.log('test slow has been running for over 60 seconds');",
  "if (process.argv[2] === 'finishes') console.log('test slow ... ok');",
  "if (process.argv[2] === 'hangs') setInterval(() => {}, 1000);",
  "else if (process.argv[2] === 'leaves-a-helper') {",
  "  require('node:child_process').spawn('sleep', ['30'], { stdio: ['ignore', 1, 'ignore'], detached: true }).unref();",
  '  process.exit(Number(process.argv[3]));',
  '} else process.exit(Number(process.argv[3]));',
].join('\n');

function fixture(t) {
  const dir = mkdtempSync(path.join(tmpdir(), 'watchdog-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const fake = path.join(dir, 'fake-test-binary.cjs');
  writeFileSync(fake, FAKE_RUN, { mode: 0o755 });
  return { fake, log: path.join(dir, 'tmpdir.log') };
}

function run(t, ...args) {
  const { fake } = fixture(t);
  return spawnSync(process.execPath, [WATCHDOG, fake, ...args], {
    encoding: 'utf8',
    env: { ...process.env, OCTOCODE_TEST_STUCK_SECONDS: '1' },
  });
}

/** The test binary ran with a private TMPDIR under BASE, gone once the watchdog exited. */
function assertRemovedRoot(log) {
  const root = readFileSync(log, 'utf8');
  assert.ok(root.startsWith(path.join(BASE, PREFIX)), root);
  assert.equal(existsSync(root), false, `${root} removed`);
}

test('a test still unfinished past the wait fails the run and is named', t => {
  const result = run(t, 'hangs');
  assert.equal(result.status, 101);
  assert.match(result.stderr, /test-watchdog: slow is stuck/);
  assert.match(result.stdout, /test slow has been running/);
});

test('a helper left holding the output pipe does not hold the run', t => {
  const started = Date.now();
  const result = run(t, 'leaves-a-helper', '7');
  assert.equal(result.status, 7);
  assert.ok(Date.now() - started < 30_000, 'finished before the helper did');
});

test('a slow test that finishes leaves the output and exit code alone', t => {
  const result = run(t, 'finishes', '7');
  assert.equal(result.status, 7);
  assert.equal(result.stderr, '');
  assert.equal(result.stdout, 'test slow has been running for over 60 seconds\ntest slow ... ok\n');
});

test('each test binary gets its own temp root, removed after a pass, a failure and the stuck-test kill', { skip: process.platform === 'win32' }, t => {
  for (const [args, status] of [[['finishes', '0'], 0], [['finishes', '7'], 7], [['hangs'], 101]]) {
    const { fake, log } = fixture(t);
    const result = spawnSync(process.execPath, [WATCHDOG, fake, ...args], {
      encoding: 'utf8',
      env: { ...process.env, OCTOCODE_TEST_STUCK_SECONDS: '1', FAKE_TMPDIR_LOG: log },
    });
    assert.equal(result.status, status, `${args.join(' ')}: ${result.stderr}`);
    assertRemovedRoot(log);
  }
});

test('a signal to the watchdog stops the test binary and removes its temp root', { skip: process.platform === 'win32' }, async t => {
  const { fake, log } = fixture(t);
  const child = spawn(process.execPath, [WATCHDOG, fake, 'hangs'], { env: { ...process.env, FAKE_TMPDIR_LOG: log }, stdio: ['ignore', 'pipe', 'ignore'] });
  await new Promise(resolve => child.stdout.once('data', resolve));
  child.kill('SIGTERM');
  const status = await new Promise(resolve => child.on('close', resolve));
  assert.equal(status, 101);
  assertRemovedRoot(log);
});
