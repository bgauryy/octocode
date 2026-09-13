import { expect, it, vi } from 'vitest';
import type { PiContext, PiTheme } from '../src/types.js';
import { createExecutionState } from '../src/tools/execution-events.js';

const host = vi.hoisted(() => ({
  store: undefined as any,
  factory: undefined as any,
  workers: vi.fn(() => []),
  plan: vi.fn(() => ({
    planId: 'plan',
    phase: 'idle',
    tasks: [],
    summary: { total: 0 },
    review: { generation: 0 },
  })),
  awareness: vi.fn(() => undefined),
  awarenessHealth: vi.fn(
    (): { state: 'idle' | 'unavailable'; message?: string } => ({ state: 'idle' })
  ),
  permission: vi.fn(() => 'default'),
  footer: vi.fn(),
}));
vi.mock('../src/tools/runtime-renderer.js', () => ({
  runtimeStoreFor: () => host.store,
  setManagedFooter: (_ctx: unknown, factory: unknown) => {
    host.factory = factory;
    host.footer();
  },
  setManagedStatus: vi.fn(),
  setManagedWorkingIndicator: vi.fn(),
}));
vi.mock('../src/tools/approval.js', () => ({
  getPermissionLevel: host.permission,
}));
vi.mock('../src/tools/awareness-status.js', () => ({
  getCachedAwarenessStatus: host.awareness,
  getAwarenessStatusHealth: host.awarenessHealth,
}));
vi.mock('../src/tools/agents/ledger.js', () => ({
  listVisibleWorkerLedgerEntries: host.workers,
}));
vi.mock('../src/tools/desktop-notify.js', () => ({
  recordSessionTitle: vi.fn(),
}));
vi.mock('../src/tools/effort-dial.js', () => ({
  getActiveDialLevel: () => undefined,
}));
vi.mock('../src/tools/peer-wip.js', () => ({ peerWipCount: () => 0 }));
vi.mock('../src/tools/planning/plan-store.js', () => ({
  activePlanScope: () => 'plan',
}));
vi.mock('../src/tools/plan-read-model.js', () => ({
  getCurrentPlanReadModel: host.plan,
}));
import { updateOctocodeMetricsUi } from '../src/extension-ui.js';

it('samples external facts on updates and never reads them during redraws or resize', () => {
  const unsubscribeRuntime = vi.fn();
  const unsubscribeBranch = vi.fn();
  let subscription: (() => void) | undefined;
  const state: any = {
    phase: 'ready',
    activity: { kind: 'idle' },
    execution: createExecutionState(),
    statuses: {},
    context: { status: 'pending' },
    footer: {
      sessionStartedAt: 1000,
      completedTurns: 0,
      githubAuth: { status: 'authenticated' },
    },
    setFooter: (patch: unknown) => {
      state.footer = { ...state.footer, ...(patch as object) };
    },
  };
  host.store = {
    getState: () => state,
    subscribe: vi.fn(() => unsubscribeRuntime),
  };
  const usage = vi.fn(() => ({ tokens: 48000, contextWindow: 200000 }));
  const ctx = {
    hasUI: true,
    cwd: '/workspace',
    model: { id: 'test-model' },
    getContextUsage: usage,
  } as unknown as PiContext;
  const theme = {
    fg: (_token: string, text: string) => text,
    bold: (text: string) => text,
  } as PiTheme;
  updateOctocodeMetricsUi(ctx, 10000);
  const component = host.factory(
    { terminal: { rows: 40 }, requestRender: vi.fn() },
    theme,
    {
      getGitBranch: () => 'codex/tui',
      onBranchChange: (callback: () => void) => {
        subscription = callback;
        return unsubscribeBranch;
      },
    }
  );
  const reads = [
    host.plan,
    host.workers,
    host.awareness,
    host.awarenessHealth,
    host.permission,
    usage,
  ];
  const before = reads.map(read => read.mock.calls.length);
  updateOctocodeMetricsUi({ ...ctx }, 10000);
  expect(host.footer).toHaveBeenCalledOnce();
  const afterFreshContext = reads.map(read => read.mock.calls.length);
  for (const width of [36, 80, 120, 36]) {
    component.invalidate();
    expect(component.render(width).join('\n')).toContain('24%');
  }
  subscription?.();
  component.render(80);
  expect(reads.map(read => read.mock.calls.length)).toEqual(afterFreshContext);
  expect(afterFreshContext.every((count, index) => count > before[index]!)).toBe(true);
  usage.mockReturnValue({ tokens: 100000, contextWindow: 200000 });
  updateOctocodeMetricsUi(ctx, 11000);
  component.invalidate();
  expect(component.render(120).join('\n')).toContain('50%');
  host.awarenessHealth.mockReturnValue({
    state: 'unavailable',
    message: 'private database path',
  });
  updateOctocodeMetricsUi(ctx, 11500);
  component.invalidate();
  const awarenessWarning = component.render(120).join('\n');
  expect(awarenessWarning).toContain('Awareness status unavailable');
  expect(awarenessWarning).not.toContain('private database path');
  host.permission.mockReturnValue('relaxed');
  updateOctocodeMetricsUi(ctx, 12000);
  component.invalidate();
  expect(component.render(36).join('\n')).toContain('perm relaxed');
  expect(
    component
      .render(120)
      .join('\n')
      .match(/session /g)
  ).toHaveLength(1);
  expect(host.footer).toHaveBeenCalledOnce();
  expect(host.store.subscribe).toHaveBeenCalledOnce();
  component.dispose();
  expect(unsubscribeRuntime).toHaveBeenCalledOnce();
  expect(unsubscribeBranch).toHaveBeenCalledOnce();
});
