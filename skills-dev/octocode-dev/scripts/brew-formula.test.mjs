import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { hermeticEnv } from '../../../test-utils/hermetic-env.ts';
import { renderFormula } from './brew-formula.mjs';
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
