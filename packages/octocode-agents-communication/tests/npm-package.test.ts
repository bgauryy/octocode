import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { copyFileSync, mkdirSync } from 'node:fs';
import { isChildFailure, root, tempWorkspace } from './helpers.ts';

test('npm launcher reports startup failures and preserves command errors', t => {
  const directory = tempWorkspace(t, 'communication-launcher-');
  const entry = join(root, 'bin/octocode-agents-communication.mjs');
  const env = process.env;
  const help = execFileSync(process.execPath, [entry, '/cli', '--help'], { encoding: 'utf8', timeout: 10000, stdio: 'pipe', env });
  assert.match(help, /join/);
  // A package without its built runtime fails with the module error and a non-zero status.
  mkdirSync(join(directory, 'bin'));
  copyFileSync(entry, join(directory, 'bin/octocode-agents-communication.mjs'));
  assert.throws(() => execFileSync(process.execPath, [join(directory, 'bin/octocode-agents-communication.mjs'), '/cli', '--help'], {
    encoding: 'utf8', timeout: 10000, stdio: 'pipe', env,
  }), error => isChildFailure(error) && error.status === 1 && /cli\.js/.test(error.stderr.toString()));
  assert.throws(() => execFileSync(process.execPath, [entry, 'not-a-command'], {
    encoding: 'utf8', timeout: 10000, stdio: 'pipe',
  }), error => isChildFailure(error) && error.status !== 0);
});
