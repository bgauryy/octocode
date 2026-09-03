import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  parseGitHubAuthStatus,
  probeGitHubAuth,
} from '../src/tools/github-auth-status.js';
import type { PiInstance } from '../src/types.js';

test('parseGitHubAuthStatus accepts authenticated Octocode or gh CLI metadata without retaining secrets', () => {
  const state = parseGitHubAuthStatus(JSON.stringify({
    success: true,
    authenticated: true,
    tokenPresent: true,
    tokenSource: 'octocode',
    tokenExpired: false,
    token: 'SECRET-SENTINEL',
  }));

  assert.deepEqual(state, { status: 'authenticated', source: 'octocode' });
  assert.doesNotMatch(JSON.stringify(state), /SECRET-SENTINEL/);

  assert.deepEqual(parseGitHubAuthStatus(JSON.stringify({
    authenticated: true,
    tokenPresent: true,
    tokenSource: 'gh-cli',
    tokenExpired: false,
  })), { status: 'authenticated', source: 'gh-cli' });
});

test('parseGitHubAuthStatus distinguishes missing, expired, and malformed status', () => {
  assert.deepEqual(parseGitHubAuthStatus(JSON.stringify({
    authenticated: false,
    tokenPresent: false,
    tokenSource: 'none',
    tokenExpired: false,
  })), { status: 'missing' });
  assert.deepEqual(parseGitHubAuthStatus(JSON.stringify({
    authenticated: false,
    tokenPresent: true,
    tokenSource: 'octocode',
    tokenExpired: true,
  })), { status: 'missing' });
  assert.deepEqual(parseGitHubAuthStatus('not json'), { status: 'error' });
});

test('probeGitHubAuth runs the read-only Octocode CLI check with a timeout', async () => {
  const calls: Array<{ command: string; args: string[]; timeout?: number }> = [];
  const exec = async (command: string, args: string[], opts?: { timeout?: number }) => {
    calls.push({ command, args, timeout: opts?.timeout });
    return {
      code: 0,
      stdout: JSON.stringify({ authenticated: true, tokenSource: 'env', tokenExpired: false }),
      stderr: '',
    };
  };

  const state = await probeGitHubAuth(exec as NonNullable<PiInstance['exec']>);
  assert.deepEqual(state, { status: 'authenticated', source: 'env' });
  assert.deepEqual(calls, [{
    command: 'npx',
    args: ['octocode', 'auth', 'status', '--json'],
    timeout: 10_000,
  }]);
});

test('probeGitHubAuth degrades to error when exec is unavailable, fails, or throws', async () => {
  assert.deepEqual(await probeGitHubAuth(undefined), { status: 'error' });
  assert.deepEqual(await probeGitHubAuth(async () => ({ code: 1, stdout: '', stderr: 'failed' })), { status: 'error' });
  assert.deepEqual(await probeGitHubAuth(async () => { throw new Error('timeout'); }), { status: 'error' });
});
