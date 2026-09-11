import assert from 'node:assert/strict';
import { test } from 'vitest';
import { projectBackgroundJobs, sanitizeBgTitle, type BgJob } from '../src/tools/bash-bg-tool.js';

test('background jobs publish a redacted serializable runtime projection', () => {
  const job: BgJob = {
    id: 'job-1',
    title: 'Typecheck',
    command: 'secret command',
    cwd: '/private/workspace',
    startedAt: 1_000,
    endedAt: 2_500,
    updatedAt: 2_500,
    status: 'failed',
    exitCode: 1,
    logPath: '/private/log',
    pid: 42,
    timeoutSeconds: 30,
  };

  const projection = projectBackgroundJobs([job]);
  assert.deepEqual(projection, [{
    id: 'job-1', title: 'Typecheck', status: 'failed', startedAt: 1_000,
    endedAt: 2_500, updatedAt: 2_500, exitCode: 1,
  }]);
  assert.equal('command' in projection[0]!, false);
  assert.equal('cwd' in projection[0]!, false);
  assert.equal('pid' in projection[0]!, false);
  assert.equal('logPath' in projection[0]!, false);
});

test('background titles remove ANSI and control characters before reaching UI state', () => {
  assert.equal(sanitizeBgTitle('\u001b[31mBuild\u001b[0m\nworkspace'), 'Build workspace');
});
