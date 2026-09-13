import type { ExternalAwarenessStatus } from '@octocodeai/octocode-awareness/host';
import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import {
  forceAwarenessStatusRefreshForTests,
  formatAwarenessPanel,
  getCachedAwarenessStatus,
  getAwarenessStatusHealth,
  hasAwarenessSignal,
  refreshAwarenessPanel,
  resetAwarenessStatusStateForTests,
  setAwarenessMetricsRefreshForUi,
  setAwarenessStatusRunnerForTests,
} from '../src/tools/awareness-status.js';
import type { PiContext } from '../src/types.js';

const ZERO: ExternalAwarenessStatus = {
  activePlans: 0,
  readyTasks: 0,
  inProgressTasks: 0,
  verifyTasks: 0,
  lockCount: 0,
  workCount: 0,
  agentCount: 0,
  messageCount: 0,
  taskActivities: [],
};

test('cached observation time advances only after a successful source read', async () => {
  vi.useFakeTimers();
  try {
    vi.setSystemTime(100_000);
    const { ctx } = uiCtx();
    const runner = vi.fn(async () => ZERO);
    setAwarenessStatusRunnerForTests(runner);
    refreshAwarenessPanel(ctx);
    await Promise.resolve();
    assert.equal(getCachedAwarenessStatus(ctx.cwd!)?.observedAt, 100_000);
    vi.setSystemTime(104_000);
    refreshAwarenessPanel(ctx);
    assert.equal(runner.mock.calls.length, 1);
    assert.equal(getCachedAwarenessStatus(ctx.cwd!)?.observedAt, 100_000, 'repaint cannot make stale evidence fresh');
    vi.setSystemTime(112_000);
    refreshAwarenessPanel(ctx);
    await Promise.resolve();
    assert.equal(getCachedAwarenessStatus(ctx.cwd!)?.observedAt, 112_000);
  } finally {
    vi.useRealTimers();
  }
});

const FULL: ExternalAwarenessStatus = {
  activePlans: 1,
  readyTasks: 2,
  inProgressTasks: 1,
  verifyTasks: 4,
  lockCount: 2,
  workCount: 1,
  agentCount: 2,
  messageCount: 5,
  unreadInbox: 1,
  taskActivities: [
    {
      taskId: 'task-doing',
      title: 'Implement lifecycle',
      state: 'doing',
      agentId: 'octo-worker',
    },
    { taskId: 'task-ready', title: 'Verify CLI', state: 'ready' },
  ],
  lastMessage: { from: 'planner', to: 'worker', preview: 'take lane' },
  lastInbound: { from: 'planner', preview: 'take lane' },
};

afterEach(() => {
  resetAwarenessStatusStateForTests();
  setAwarenessMetricsRefreshForUi(undefined);
});

test('signal detection uses typed status', () => {
  assert.equal(hasAwarenessSignal(ZERO), false);
  assert.equal(hasAwarenessSignal(FULL), true);
});

test('panel composition preserves counts, debt, tasks, messages, and attention state', () => {
  const lines = formatAwarenessPanel(FULL, undefined, 120);
  const text = lines.join('\n');
  assert.match(text, /verify/);
  assert.match(text, /Implement lifecycle/);
  assert.match(text, /Verify CLI/);
  assert.match(text, /planner/);
  assert.match(text, /take lane/);
  assert.deepEqual(formatAwarenessPanel(ZERO), []);
});

test('panel remains width-bounded without losing semantic groups', () => {
  const lines = formatAwarenessPanel(FULL, undefined, 44);
  assert.ok(lines.length > 1);
  assert.match(lines.join('\n'), /verify|inbox|peer/);
});

function uiCtx() {
  const widget: Array<{ cleared: boolean; isFn: boolean }> = [];
  const ctx = {
    cwd: '/tmp/aware-ws',
    hasUI: true,
    ui: {
      setWidget: (_name: string, content: unknown) =>
        widget.push({
          cleared: content === undefined,
          isFn: typeof content === 'function',
        }),
      setStatus: () => {},
    },
  } as unknown as PiContext;
  return { ctx, widget };
}

test('refresh caches one typed package snapshot and throttles repeated paints', async () => {
  let calls = 0;
  setAwarenessStatusRunnerForTests(async ctx => {
    calls++;
    assert.equal(ctx.cwd, '/tmp/aware-ws');
    return FULL;
  });
  const { ctx, widget } = uiCtx();
  refreshAwarenessPanel(ctx);
  await new Promise(resolve => setTimeout(resolve, 5));
  refreshAwarenessPanel(ctx);
  await new Promise(resolve => setTimeout(resolve, 5));
  assert.equal(calls, 1);
  const cached = getCachedAwarenessStatus(ctx.cwd!);
  assert.ok(cached?.observedAt);
  const { observedAt: _observedAt, ...status } = cached!;
  assert.deepEqual(status, FULL);
  // Awareness data is cached but the panel is not registered unless there is an active
  // plan or agent section — awareness-only state no longer drives panel visibility.
  assert.ok(
    !widget.some(entry => entry.isFn && !entry.cleared),
    'panel is not registered for awareness-only state'
  );
});

test('refresh repaints the unified footer for cached and newly loaded Awareness state', async () => {
  let repaints = 0;
  setAwarenessMetricsRefreshForUi(() => {
    repaints++;
  });
  setAwarenessStatusRunnerForTests(async () => FULL);
  const { ctx } = uiCtx();
  refreshAwarenessPanel(ctx);
  await new Promise(resolve => setTimeout(resolve, 5));
  assert.equal(
    repaints,
    2,
    'cached snapshot and async replacement each repaint the footer'
  );
});

test('refresh preserves last known status and reports a failed source read', async () => {
  let calls = 0;
  setAwarenessStatusRunnerForTests(async () => {
    if (calls++ === 0) return FULL;
    throw new Error('database temporarily unavailable');
  });
  const { ctx, widget } = uiCtx();
  refreshAwarenessPanel(ctx);
  await new Promise(resolve => setTimeout(resolve, 5));
  forceAwarenessStatusRefreshForTests(ctx.cwd!);
  refreshAwarenessPanel(ctx);
  await new Promise(resolve => setTimeout(resolve, 5));
  assert.equal(getCachedAwarenessStatus(ctx.cwd!)?.unreadInbox, 1);
  assert.deepEqual(getAwarenessStatusHealth(ctx.cwd!), {
    state: 'unavailable',
    message: 'database temporarily unavailable',
  });
  assert.equal(
    widget.length,
    0,
    'an unregistered empty panel is not redundantly cleared'
  );
});

test('a disabled status source remains distinct from an unavailable source', async () => {
  setAwarenessStatusRunnerForTests(async () => null);
  const { ctx } = uiCtx();
  refreshAwarenessPanel(ctx);
  await new Promise(resolve => setTimeout(resolve, 5));
  assert.equal(getCachedAwarenessStatus(ctx.cwd!), null);
  assert.deepEqual(getAwarenessStatusHealth(ctx.cwd!), { state: 'disabled' });
});
