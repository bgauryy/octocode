import assert from 'node:assert/strict';
import { test } from 'vitest';
import { createRuntimeStore, runRuntimeTask } from '../src/tools/runtime-store.js';

test('runtime store resets session state and tracks ordered initialization work', async () => {
  let now = 100;
  const store = createRuntimeStore(() => ++now);
  const firstGeneration = store.getState().begin('loading configuration');
  store.getState().setStatus('worker', 'running');
  store.getState().setContext({
    status: 'ready',
    mode: 'routing',
    systemPromptChars: 12_000,
    directToolChars: 40_000,
    providerSubtotalChars: 52_000,
    estimatedTokens: 13_000,
  });
  await runRuntimeTask(store, 'environment', 'loading environment', async () => 'ok');
  store.getState().ready('Octocode ready · cached MCP');

  assert.equal(firstGeneration, 1);
  assert.equal(store.getState().phase, 'ready');
  assert.equal(store.getState().tasks['environment']?.status, 'ready');
  assert.equal(store.getState().statuses['worker'], 'running');
  assert.equal(store.getState().context.providerSubtotalChars, 52_000);
  assert.equal(store.getState().context.status, 'ready');

  const secondGeneration = store.getState().begin();
  assert.equal(secondGeneration, 2);
  assert.deepEqual(store.getState().tasks, {});
  assert.deepEqual(store.getState().statuses, {});
  assert.equal(store.getState().mcp.status, 'idle');
  assert.equal(store.getState().context.status, 'pending');
  assert.equal(store.getState().context.providerSubtotalChars, 0);
});

test('non-critical runtime work degrades without rejecting initialization', async () => {
  const store = createRuntimeStore();
  store.getState().begin();
  const result = await runRuntimeTask(store, 'github', 'checking GitHub', async () => {
    throw new Error('offline');
  });

  assert.equal(result, undefined);
  assert.equal(store.getState().tasks['github']?.status, 'degraded');
  assert.equal(store.getState().tasks['github']?.error, 'offline');
});

test('critical runtime work records failure and rejects', async () => {
  const store = createRuntimeStore();
  store.getState().begin();
  await assert.rejects(
    runRuntimeTask(store, 'environment', 'loading environment', () => { throw new Error('bad env'); }, { critical: true }),
    /bad env/,
  );
  assert.equal(store.getState().tasks['environment']?.status, 'failed');
});

test('runtime store owns footer metrics as one Zustand state slice', () => {
  const store = createRuntimeStore(() => 1234);
  store.getState().setFooter({
    sessionStartedAt: 1000,
    completedTurns: 3,
    activeTurnStartedAt: 1200,
    usage: { tokens: 42_000, contextWindow: 200_000 },
    githubAuth: { status: 'authenticated' },
  });
  assert.deepEqual(store.getState().footer, {
    sessionStartedAt: 1000,
    completedTurns: 3,
    activeTurnStartedAt: 1200,
    usage: { tokens: 42_000, contextWindow: 200_000 },
    githubAuth: { status: 'authenticated' },
  });
});

test('foreground activity is timestamped independently from runtime initialization', () => {
  let now = 200;
  const store = createRuntimeStore(() => ++now);

  store.getState().setActivity({ kind: 'planning', planScope: '/workspace', detail: 'Drafting RFC' });

  assert.deepEqual(store.getState().activity, {
    kind: 'planning',
    since: 201,
    planScope: '/workspace',
    detail: 'Drafting RFC',
  });
  assert.equal(store.getState().phase, 'idle');
});

test('background work is generation-scoped, deduplicated, and reset with the session', () => {
  let now = 1_000;
  const store = createRuntimeStore(() => ++now);
  const generation = store.getState().begin();
  let publications = 0;
  const unsubscribe = store.subscribe(() => { publications += 1; });
  const jobs = [{
    id: 'job-1',
    title: 'Run tests',
    status: 'running' as const,
    startedAt: 900,
    updatedAt: 1_001,
  }];

  store.getState().setBackgroundJobs(jobs, generation);
  store.getState().setBackgroundJobs(jobs, generation);
  store.getState().setBackgroundJobs([{ ...jobs[0], status: 'failed' }], generation - 1);

  assert.equal(publications, 1);
  assert.deepEqual(store.getState().backgroundJobs, jobs);
  store.getState().begin();
  assert.deepEqual(store.getState().backgroundJobs, []);
  unsubscribe();
});
