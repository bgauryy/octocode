import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { hermeticEnv } from '../../../test-utils/hermetic-env.ts';
import { checkBinaryVersion, renderFormula } from './brew-formula.mjs';
import { ROOT } from './lib/workspaces.mjs';

test('the generated Homebrew smoke query executes against the built CLI', () => {
  const artifact = { url: 'https://registry.example.test/native.tgz', sha256: 'a'.repeat(64) };
  const formula = renderFormula({ macos: { arm: artifact, intel: artifact }, linux: { arm: artifact, intel: artifact } });
  const query = /query = '([^']+)'/.exec(formula)?.[1];
  assert.ok(query, 'the formula emits its executable smoke request');
  const directory = mkdtempSync(path.join(tmpdir(), 'octocode-brew-test-'));
  const env = hermeticEnv(process.env, 'brew-formula');
  try {
    writeFileSync(path.join(directory, 'notes.txt'), 'alpha\nneedle beta\n');
    const result = spawnSync(process.execPath, [path.join(ROOT, 'packages/octocode/out/octocode.js'), 'localSearch', query, '--json'], {
      cwd: directory, env: { ...env, OCTOCODE_ENABLE_LOCAL: 'true' }, encoding: 'utf8', timeout: 30_000,
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const output = JSON.parse(result.stdout);
    assert.deepEqual(output.results[0].data.files[0].matches.map(({ line, value }) => ({ line, value })), [{ line: 2, value: 'needle beta' }]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
    rmSync(env.OCTOCODE_HOME, { recursive: true, force: true });
  }
});

test('checkBinaryVersion accepts the version with a hex fingerprint suffix and rejects anything else', { skip: process.platform === 'win32' }, () => {
  const directory = mkdtempSync(path.join(tmpdir(), 'octocode-brew-version-'));
  const tarballWith = reported => {
    const root = path.join(directory, reported.replace(/\W/g, '_'));
    mkdirSync(path.join(root, 'package'), { recursive: true });
    writeFileSync(path.join(root, 'package', 'octocode'), `#!/bin/sh\necho "${reported}"\n`, { mode: 0o755 });
    const archive = path.join(root, 'pkg.tgz');
    execFileSync('tar', ['-czf', archive, 'package'], { cwd: root });
    return readFileSync(archive);
  };
  const log = console.log;
  console.log = () => {};
  try {
    checkBinaryVersion(tarballWith('octocode 1.2.3'), 'linux-x64-gnu', '1.2.3');
    checkBinaryVersion(tarballWith('octocode 1.2.3+01a553b7'), 'linux-x64-gnu', '1.2.3');
    assert.throws(() => checkBinaryVersion(tarballWith('octocode 1.3.0+01a553b7'), 'linux-x64-gnu', '1.2.3'), /reports/);
    assert.throws(() => checkBinaryVersion(tarballWith('octocode 1.2.3+nothex'), 'linux-x64-gnu', '1.2.3'), /reports/);
  } finally {
    console.log = log;
    rmSync(directory, { recursive: true, force: true });
  }
});
