import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  createOctocodeCronScheduler,
  formatOctocodeCronStatus,
  handleOctocodeCronCommand,
} from '../src/scheduler.js';
import { resolveAwarenessCliPath } from '../src/assets.js';
import type { PiExecResult } from '../src/types.js';

test('cron scheduler lists the report-first Awareness status job', () => {
  const scheduler = createOctocodeCronScheduler({
    env: {
      OCTOCODE_CRON: '0',
    } as NodeJS.ProcessEnv,
  });

  scheduler.start({ cwd: '/workspace' });
  const jobs = scheduler.list();

  assert.equal(jobs.length, 1);
  assert.equal(jobs[0]!.name, 'awareness-status');
  assert.equal(jobs[0]!.enabled, false);
  assert.equal(jobs[0]!.status, 'cancelled');
  assert.match(formatOctocodeCronStatus(jobs), /awareness-status/);
  // status is report-first but DOES prune expired locks/work rows as a side effect.
  assert.match(formatOctocodeCronStatus(jobs), /prunes expired locks\/work rows/);
});

test('cron scheduler can run the default Awareness status job on demand', async () => {
  const calls: Array<{ command: string; args: string[] }> = [];
  const scheduler = createOctocodeCronScheduler({
    env: {
      OCTOCODE_CRON: '0',
    } as NodeJS.ProcessEnv,
    executor: async (command, args): Promise<PiExecResult> => {
      calls.push({ command, args });
      return { stdout: 'status ok', stderr: '', code: 0 };
    },
  });

  const results = await scheduler.runNow(undefined, { cwd: '/repo' });

  assert.deepEqual(results, [
    {
      job: 'awareness-status',
      status: 'succeeded',
      exitCode: 0,
      message: 'status ok',
    },
  ]);
  assert.equal(calls.length, 1);
  assert.equal(calls[0]!.command, process.execPath);
  assert.equal(calls[0]!.args[0], resolveAwarenessCliPath());
  assert.deepEqual(calls[0]!.args.slice(1), [
    'status',
    '--workspace',
    '/repo',
  ]);
});

test('cron scheduler runs manual checks without an awareness CLI env var', async () => {
  const calls: Array<{ command: string; args: string[] }> = [];
  const scheduler = createOctocodeCronScheduler({
    env: { OCTOCODE_CRON: '0' } as NodeJS.ProcessEnv,
    executor: async (command, args): Promise<PiExecResult> => {
      calls.push({ command, args });
      return { stdout: 'checked through local package CLI', stderr: '', code: 0 };
    },
  });

  const results = await scheduler.runNow('awareness-status', { cwd: '/repo' });

  assert.equal(results[0]!.status, 'succeeded');
  assert.equal(calls[0]!.command, process.execPath);
  assert.equal(calls[0]!.args[0], resolveAwarenessCliPath());
  assert.deepEqual(calls[0]!.args.slice(1, 2), ['status']);
});

test('cron command supports list, check default, check all, cancel, and help', async () => {
  const messages: Array<{ message: string; level?: string }> = [];
  const scheduler = createOctocodeCronScheduler({
    env: {
      OCTOCODE_CRON: '1',
      OCTOCODE_CRON_STATUS_INTERVAL_MS: '600000',
    } as NodeJS.ProcessEnv,
    executor: async (): Promise<PiExecResult> => ({ stdout: 'checked', stderr: '', code: 0 }),
  });
  const notify = (_ctx: unknown, message: string, level?: string) => {
    messages.push({ message, level });
  };

  scheduler.start({ cwd: '/repo' });
  await handleOctocodeCronCommand('', undefined, scheduler, notify);
  assert.match(messages.at(-1)!.message, /Octocode session jobs/);
  assert.match(messages.at(-1)!.message, /Commands: \/octocode-cron list · check \[default\|all\|job\]/);

  await handleOctocodeCronCommand('check', undefined, scheduler, notify);
  assert.match(messages.at(-1)!.message, /awareness-status: succeeded/);
  assert.match(messages.at(-1)!.message, /checked/);

  await handleOctocodeCronCommand('check all', undefined, scheduler, notify);
  assert.match(messages.at(-1)!.message, /awareness-status: succeeded/);
  assert.match(messages.at(-1)!.message, /checked/);

  await handleOctocodeCronCommand('cancel', undefined, scheduler, notify);
  assert.match(messages.at(-1)!.message, /Cancelled Octocode session job\(s\): awareness-status/);

  await handleOctocodeCronCommand('help', undefined, scheduler, notify);
  assert.match(messages.at(-1)!.message, /Usage: \/octocode-cron list\|check \[default\|all\|job\]\|cancel \[default\|all\|job\]\|help/);

  await handleOctocodeCronCommand('run', undefined, scheduler, notify);
  assert.equal(messages.at(-1)!.level, 'warning');
  assert.match(messages.at(-1)!.message, /Unknown \/octocode-cron command: run/);

  scheduler.stop();
});
