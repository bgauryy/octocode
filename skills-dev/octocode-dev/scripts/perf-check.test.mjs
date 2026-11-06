import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test('performance comparisons retain requested flows and reject incomplete measurements', () => {
  const run = spawnSync('python3', ['-I', fileURLToPath(new URL('./profile/perf_check_test.py', import.meta.url))], { encoding: 'utf8' });
  assert.equal(run.status, 0, `${run.error ?? ''}\n${run.stdout}\n${run.stderr}`);
});
