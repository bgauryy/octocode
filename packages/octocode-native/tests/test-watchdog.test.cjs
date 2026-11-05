'use strict';

const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const WATCHDOG = path.join(__dirname, '..', 'scripts', 'test-watchdog.cjs');
const FAKE_RUN = [
  '#!/usr/bin/env node',
  "console.log('test slow has been running for over 60 seconds');",
  "if (process.argv[2] === 'finishes') console.log('test slow ... ok');",
  "if (process.argv[2] === 'hangs') setInterval(() => {}, 1000);",
  "else if (process.argv[2] === 'leaves-a-helper') {",
  "  require('node:child_process').spawn('sleep', ['30'], { stdio: ['ignore', 1, 'ignore'], detached: true }).unref();",
  '  process.exit(Number(process.argv[3]));',
  '} else process.exit(Number(process.argv[3]));',
].join('\n');

function run(...args) {
  const dir = mkdtempSync(path.join(tmpdir(), 'watchdog-'));
  try {
    const fake = path.join(dir, 'fake-test-binary.cjs');
    writeFileSync(fake, FAKE_RUN, { mode: 0o755 });
    return spawnSync(process.execPath, [WATCHDOG, fake, ...args], {
      encoding: 'utf8',
      env: { ...process.env, OCTOCODE_TEST_STUCK_SECONDS: '1' },
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a test still unfinished past the wait fails the run and is named', () => {
  const result = run('hangs');
  assert.equal(result.status, 101);
  assert.match(result.stderr, /test-watchdog: slow is stuck/);
  assert.match(result.stdout, /test slow has been running/);
});

test('a helper left holding the output pipe does not hold the run', () => {
  const started = Date.now();
  const result = run('leaves-a-helper', '7');
  assert.equal(result.status, 7);
  assert.ok(Date.now() - started < 30_000, 'finished before the helper did');
});

test('a slow test that finishes leaves the output and exit code alone', () => {
  const result = run('finishes', '7');
  assert.equal(result.status, 7);
  assert.equal(result.stderr, '');
  assert.equal(result.stdout, 'test slow has been running for over 60 seconds\ntest slow ... ok\n');
});
