import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';
import { composeStatusPanelLines, resetStatusPanelStateForTests, setStatusPanelAgentSource } from '../src/tools/status-panel.js';
import { clearPlan, setPlan } from '../src/tools/active-plan.js';
import { refreshPlanUi } from '../src/tools/plan-tool.js';
import type { PiContext } from '../src/types.js';

const THEME = { fg: (_c: string, text: string) => text, bold: (text: string) => text };
const STATUS_CWD = '/tmp/status-panel-test-ws';

afterEach(() => {
  clearPlan(STATUS_CWD);
  resetStatusPanelStateForTests();
  setStatusPanelAgentSource(undefined);
});

test('status panel composes every agent through the shared component stack', () => {
  const { ctx } = uiCtx();
  setStatusPanelAgentSource((_theme, width) => [
    `Agents · ${width}`,
    ...Array.from({ length: 12 }, (_, index) => `  agent-${index + 1} · ${index === 3 ? 'running' : 'done'}`),
  ]);
  const body = composeStatusPanelLines(ctx, THEME, 72).lines.join('\n');
  assert.match(body, /Agents · 72/);
  assert.match(body, /agent-1/);
  assert.match(body, /agent-12/);
  assert.match(body, /agent-4 · running/);
});

function uiCtx(cwd = STATUS_CWD): { ctx: PiContext; calls: Array<{ name: string; cleared: boolean; isFn: boolean; content: unknown }> } {
  const calls: Array<{ name: string; cleared: boolean; isFn: boolean; content: unknown }> = [];
  const ctx = {
    cwd,
    hasUI: true,
    model: { id: 'claude-test', provider: 'test-provider' },
    ui: {
      setWidget: (name: string, content: unknown) => calls.push({ name, cleared: content === undefined, isFn: typeof content === 'function', content }),
    },
  } as unknown as PiContext;
  return { ctx, calls };
}

test('status panel renderer shrinks with volatile sections instead of retaining blank rows', () => {
  const { ctx, calls } = uiCtx();
  setPlan(STATUS_CWD, Array.from({ length: 6 }, (_, i) => `step ${i + 1}`));
  refreshPlanUi(ctx);
  const registration = calls.find((call) => call.name === 'octocode-status-panel' && call.isFn)!;
  const factory = registration.content as (tui: unknown, theme: unknown) => { render(width: number): string[] };
  const renderer = factory({ requestRender: () => undefined }, THEME);

  const tall = renderer.render(100);
  assert.ok(tall.length > 2, 'active plan makes the panel taller than the model-only baseline');

  setPlan(STATUS_CWD, ['one remaining step']);
  const shorterVolatile = renderer.render(100);
  assert.ok(shorterVolatile.length < tall.length, 'volatile panel contracts to its current content height');
  assert.equal(shorterVolatile.some((line) => line === ''), false, 'the panel does not synthesize blank padding rows');

  clearPlan(STATUS_CWD);
  const modelOnly = renderer.render(100);
  assert.deepEqual(modelOnly, [''], 'empty volatile state resets the renderer without duplicate model/context chrome');
});

test('status panel shows every task and highlights the running task without duplicate context/model lines', () => {
  const { ctx } = uiCtx();
  setPlan(STATUS_CWD, Array.from({ length: 40 }, (_, i) => `task ${i + 1}`));
  const lines = composeStatusPanelLines(ctx, THEME, 120).lines;
  const body = lines.join('\n');
  assert.match(body, /task 1/);
  assert.match(body, /task 40/);
  assert.doesNotMatch(body, /… \d+ more/);
  assert.doesNotMatch(body, /^model:/m);
  assert.doesNotMatch(body, /^ctx:/m);
});

test('status panel composes the current branch plan at render time, not registration time', () => {
  const { ctx } = uiCtx();
  setPlan(STATUS_CWD, ['initial plan']);
  const initial = composeStatusPanelLines(ctx, THEME, 100).lines.join('\n');
  assert.match(initial, /initial plan/);

  setPlan(STATUS_CWD, ['compaction restored plan']);
  const afterMutation = composeStatusPanelLines(ctx, THEME, 100).lines.join('\n');
  assert.match(afterMutation, /compaction restored plan/);
  assert.doesNotMatch(afterMutation, /initial plan/);
});

test('status panel does not register a blank model widget when provider is unknown', () => {
  const { calls } = uiCtx();
  const ctx = { cwd: STATUS_CWD, hasUI: true, model: { id: 'claude-test' }, ui: { setWidget: (name: string, content: unknown) => calls.push({ name, cleared: content === undefined, isFn: typeof content === 'function', content }) } } as unknown as PiContext;
  refreshPlanUi(ctx);
  assert.ok(calls.some((call) => call.name === 'octocode-status-panel' && call.cleared), 'provider-less model clears instead of painting an empty widget');
  assert.equal(calls.some((call) => call.isFn), false, 'no blank renderer is registered');
});
