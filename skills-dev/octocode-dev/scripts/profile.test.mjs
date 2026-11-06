import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test('language-server profiles reject persistent or unresolved coverage gaps', () => {
  const run = spawnSync('python3', ['-I', fileURLToPath(new URL('./profile/profile_test.py', import.meta.url))], { encoding: 'utf8' });
  assert.equal(run.status, 0, `${run.error ?? ''}\n${run.stdout}\n${run.stderr}`);
});
