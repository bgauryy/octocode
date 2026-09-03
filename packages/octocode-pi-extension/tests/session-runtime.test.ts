import assert from 'node:assert/strict';
import { test } from 'vitest';
import { SessionRuntime } from '../src/session-runtime.js';
import { createRuntimeStore } from '../src/tools/runtime-store.js';

test('SessionRuntime runs independent tasks through allSettled and records failures', async () => {
  const runtime = new SessionRuntime({ store: createRuntimeStore(), bindRenderer: () => () => undefined });
  const receipts = await runtime.runTasks([
    { name: 'ok', message: 'ok', run: async () => 'value' },
    { name: 'bad', message: 'bad', run: async () => { throw new Error('broken'); } },
  ]);
  assert.deepEqual(receipts.map((receipt) => receipt.status), ['fulfilled', 'fulfilled']);
  assert.equal(runtime.store.getState().tasks['ok']?.status, 'ready');
  assert.equal(runtime.store.getState().tasks['bad']?.status, 'degraded');
  assert.equal(runtime.store.getState().phase, 'degraded');
});

test('SessionRuntime derives ready only after every initialization receipt settles', async () => {
  let release!: () => void;
  const runtime = new SessionRuntime({ store: createRuntimeStore(), bindRenderer: () => () => undefined });
  const running = runtime.runTasks([
    { name: 'fast', message: 'fast', run: async () => 'ready' },
    { name: 'slow', message: 'slow', run: () => new Promise<void>((resolve) => { release = resolve; }) },
  ]);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  assert.equal(runtime.store.getState().phase, 'initializing');
  release();
  await running;
  assert.equal(runtime.store.getState().phase, 'ready');
});

test('SessionRuntime aborts once and prevents late task publication', async () => {
  let release!: () => void;
  let disposeCalls = 0;
  const runtime = new SessionRuntime({
    store: createRuntimeStore(),
    bindRenderer: () => () => undefined,
    onDispose: () => { disposeCalls += 1; },
  });
  const task = runtime.runTask({
    name: 'late',
    message: 'late',
    run: () => new Promise<void>((resolve) => { release = resolve; }),
  });
  await runtime.dispose('replace');
  await runtime.dispose('replace-again');
  release();
  await task;
  assert.equal(disposeCalls, 1);
  assert.equal(runtime.signal.aborted, true);
  assert.equal(runtime.store.getState().phase, 'disposed');
  assert.equal(runtime.store.getState().tasks['late']?.status, 'running');
});
