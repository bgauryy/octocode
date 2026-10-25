// Opt-in OS process-tree integration; ordinary tests use mocked children and time.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { spawnCli } from './mcp-client.mjs';
import { ROOT } from './paths.mjs';

if (process.env.OCTOCODE_LIVE !== '1') throw new Error('OS process-tree integration requires OCTOCODE_LIVE=1');

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-cli-deadline-'));
  const launcher = path.join(dir, 'launcher.cjs');
  const native = path.join(dir, 'native.cjs');
  const pids = path.join(dir, 'pids.json');
  fs.writeFileSync(launcher, `
const {runForwardingSignals} = require(${JSON.stringify(path.join(ROOT, 'packages/octocode-native/bin/launch-native.cjs'))});
runForwardingSignals(process.execPath, process.argv.slice(2)).then(code => process.exit(code));
`);
  fs.writeFileSync(native, `
const fs = require('node:fs');
const {spawn} = require('node:child_process');
if (process.argv[2] === 'hold') {
  const worker = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio:'inherit'});
  fs.writeFileSync(process.argv[3], JSON.stringify([process.ppid, process.pid, worker.pid]));
  process.stdout.write('native child ready\\n');
  setInterval(() => {}, 1000);
} else if (process.argv[2] === 'signal') {
  process.kill(process.pid, 'SIGTERM');
} else {
  process.stdout.write(process.env.OCTOCODE_TEST_VALUE);
  process.stderr.write('native stderr');
  process.exit(7);
}
`);
  const readPids = () => fs.existsSync(pids) ? JSON.parse(fs.readFileSync(pids, 'utf8')) : [];
  const cleanup = () => {
    for (const pid of readPids().toReversed()) {
      try { process.kill(pid, 'SIGKILL'); } catch { /* Already gone. */ }
    }
  };
  t.after(() => { cleanup(); fs.rmSync(dir, { recursive: true, force: true }); });
  return { dir, launcher, native, pids, readPids, cleanup };
}

function alive(pid) {
  try { process.kill(pid, 0); return true; } catch (error) {
    if (error.code === 'ESRCH') return false;
    throw error;
  }
}

test('a CLI deadline ends its launcher, native child and worker without waiting for inherited pipes', { timeout: 10000 }, async t => {
  const f = fixture(t);
  let neededCleanup = false;
  const watchdog = setTimeout(() => { neededCleanup = true; f.cleanup(); }, 5000);
  t.after(() => clearTimeout(watchdog));
  const result = await spawnCli([f.native, 'hold', f.pids], { bin: f.launcher, cwd: f.dir, timeoutMs: 1000 });
  clearTimeout(watchdog);
  const pids = f.readPids();
  assert.equal(pids.length, 3, 'the launcher, native child and worker all started');
  assert.equal(neededCleanup, false, 'deadline must close inherited pipes without test cleanup');
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout, 'native child ready\n');
  for (let attempt = 0; attempt < 50 && pids.some(alive); attempt++) await delay(20);
  assert.deepEqual(pids.filter(alive), [], 'no owned descendant survives the deadline');
});

test('a CLI finishing before its deadline preserves native output, exit status and signal status', async t => {
  const f = fixture(t);
  const options = { bin: f.launcher, cwd: f.dir, timeoutMs: 5000, env: { ...process.env, OCTOCODE_TEST_VALUE: 'native stdout' } };
  const normal = await spawnCli([f.native, 'exit'], options);
  assert.equal(normal.status, 7);
  assert.equal(normal.stdout, 'native stdout');
  assert.equal(normal.stderr, 'native stderr');
  const signalled = await spawnCli([f.native, 'signal'], options);
  assert.equal(signalled.status, 128 + os.constants.signals.SIGTERM);
});
