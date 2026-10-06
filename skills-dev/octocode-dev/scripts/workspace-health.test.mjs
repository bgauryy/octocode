import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

test('repo verify executes an additional workspace gate and propagates its failure', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-verify-gates-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const scripts = path.join(root, 'skills-dev/octocode-dev/scripts');
  fs.mkdirSync(scripts, { recursive: true });
  fs.copyFileSync(fileURLToPath(new URL('./workspace-health.mjs', import.meta.url)), path.join(scripts, 'workspace-health.mjs'));
  for (const name of ['dedupe-deps.mjs', 'docs-verify.mjs', 'workspace-health.test.mjs']) fs.writeFileSync(path.join(scripts, name), '');
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ workspaces: ['skills/*'] }));
  for (const [name, extra] of [['plain', {}], ['recovery', { verify: 'release-smoke' }]]) {
    const dir = path.join(root, 'skills', name);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ name, scripts: { build: 'build', lint: 'lint', test: 'test', ...extra } }));
  }
  const bin = path.join(root, 'bin'), trace = path.join(root, 'trace.jsonl');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'yarn'), `#!${process.execPath}\n` + String.raw`const fs=require('node:fs');const args=process.argv.slice(2);fs.appendFileSync(process.env.TEST_GATE_TRACE,JSON.stringify(args)+'\n');if(args[1]==='recovery'&&args[3]==='verify'&&process.env.TEST_GATE_FAIL==='1')process.exit(17);
`, { mode: 0o755 });
  const childEnv = { ...process.env };
  // The subprocess starts its own test runner; inherited node:test state skips it.
  delete childEnv.NODE_TEST_CONTEXT;
  const run = fail => spawnSync(process.execPath, [path.join(scripts, 'workspace-health.mjs'), 'verify'], {
    cwd: root, encoding: 'utf8', env: { ...childEnv, PATH: `${bin}${path.delimiter}${process.env.PATH}`, TEST_GATE_TRACE: trace, TEST_GATE_FAIL: fail ? '1' : '0' }, timeout: 30_000,
  });
  const passing = run(false);
  assert.equal(passing.status, 0, passing.stderr);
  const calls = fs.readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(calls, [
    ['workspace', 'plain', 'run', 'build'], ['workspace', 'plain', 'run', 'lint'], ['workspace', 'plain', 'run', 'test'], ['workspace', 'recovery', 'run', 'verify'],
  ]);
  const failing = run(true);
  assert.equal(failing.status, 17, failing.stderr);
});
