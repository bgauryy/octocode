import assert from 'node:assert/strict';
import { test } from 'vitest';
import { bindRuntimeRenderer, publishMcpRuntimeState, runtimeStoreFor, setManagedStatus } from '../src/tools/runtime-renderer.js';
import { activityPresentation } from '../src/tools/activity-presentation.js';
import { createRuntimeStore } from '../src/tools/runtime-store.js';
import type { PiContext } from '../src/types.js';

test('fresh event contexts share the session runtime and disposal removes that binding', () => {
  const sessionManager = { getSessionId: () => 'session-a' };
  const ctx = { hasUI: false, sessionManager } as PiContext;
  const store = createRuntimeStore();
  store.getState().setFooter({ sessionStartedAt: 1000, githubAuth: { status: 'authenticated' } });
  const dispose = bindRuntimeRenderer(ctx, store);
  const turnContext = { ...ctx };
  assert.equal(runtimeStoreFor(turnContext), store);
  setManagedStatus(turnContext, 'test', 'same session');
  assert.equal(store.getState().statuses.test, 'same session');
  assert.equal(runtimeStoreFor({ ...ctx, sessionManager: { getSessionId: () => 'other' } } as PiContext), undefined);
  dispose({ clearUi: false });
  assert.equal(runtimeStoreFor(turnContext), undefined);
});

test('one runtime renderer owns loading, MCP, managed statuses, notifications, and cleanup', () => {
  const statusCalls: Array<[string, string | undefined]> = [];
  const working: Array<boolean | undefined> = [];
  const messages: Array<string | undefined> = [];
  const notifications: string[] = [];
  const ctx: PiContext = {
    hasUI: true,
    ui: {
      setStatus: (name, text) => statusCalls.push([name, text]),
      setWorkingVisible: (visible) => working.push(visible),
      setWorkingMessage: (message) => messages.push(message),
      notify: (message) => notifications.push(message),
    },
  };
  const store = createRuntimeStore();
  const dispose = bindRuntimeRenderer(ctx, store);

  store.getState().begin('loading configuration');
  setManagedStatus(ctx, 'octocode-watch', 'watch: on');
  publishMcpRuntimeState(ctx, { status: 'running', message: 'discovering', servers: 2, tools: 0 });
  store.getState().ready('Octocode ready · 2 MCP servers');

  assert.ok(statusCalls.some(([name, text]) => name === 'octocode-init' && text === 'Octocode · loading configuration'));
  assert.ok(statusCalls.some(([name, text]) => name === 'octocode-watch' && text === 'watch: on'));
  assert.ok(statusCalls.some(([name, text]) => name === 'octocode-mcp-init' && text?.includes('2 servers')));
  assert.deepEqual(working, [false, true, false], 'unrelated status/MCP updates do not repaint working visibility');
  assert.equal(messages.at(-1), undefined);
  assert.deepEqual(notifications, ['Octocode ready · 2 MCP servers']);

  dispose();
  assert.ok(statusCalls.some(([name, text]) => name === 'octocode-watch' && text === undefined));
});

test('managed status creates a renderer-owned provisional runtime before session_start', () => {
  const calls: Array<[string, string | undefined]> = [];
  const ctx: PiContext = { hasUI: true, ui: { setStatus: (name, text) => calls.push([name, text]) } };
  setManagedStatus(ctx, 'standalone', 'ready');
  assert.deepEqual(calls, [['standalone', 'ready']]);
});

test('foreground activity drives motion while the footer exclusively owns lifecycle text', () => {
  const statusCalls: Array<[string, string | undefined]> = [];
  const working: boolean[] = [];
  const messages: Array<string | undefined> = [];
  const ctx: PiContext = {
    hasUI: true,
    ui: {
      setStatus: (name, text) => statusCalls.push([name, text]),
      setWorkingVisible: (visible) => working.push(visible),
      setWorkingMessage: (message) => messages.push(message),
    },
  };
  const store = createRuntimeStore();
  bindRuntimeRenderer(ctx, store);

  store.getState().setActivity({ kind: 'planning', planScope: '/workspace', detail: 'Drafting RFC' });
  assert.equal(working.at(-1), true);
  assert.equal(messages.at(-1), undefined);
  assert.ok(statusCalls.every(([name, text]) => name !== 'octocode-activity' || text === undefined));

  store.getState().setActivity({ kind: 'awaiting_input', planScope: '/workspace', question: 'Choose rollout' });
  assert.equal(working.at(-1), false, 'the input card owns focus without a competing spinner');
  assert.ok(statusCalls.every(([name, text]) => name !== 'octocode-activity' || text === undefined));
});

test('every foreground activity has an explicit motion and status contract', () => {
  const cases = [
    [{ kind: 'idle' } as const, false, undefined],
    [{ kind: 'thinking', since: 1 } as const, true, 'Working'],
    [{ kind: 'researching', since: 1, planScope: 'w' } as const, true, 'Researching'],
    [{ kind: 'awaiting_input', since: 1, planScope: 'w', question: 'q' } as const, false, 'q'],
    [{ kind: 'planning', since: 1, planScope: 'w' } as const, true, 'Planning'],
      [{ kind: 'reviewing', since: 1, planScope: 'w' } as const, false, 'Review plan · Start or Request changes'],
      [{ kind: 'awaiting_start', since: 1, planScope: 'w', revision: 'r' } as const, false, 'Plan ready · Review and Start'],
      [{ kind: 'ready_to_work', since: 1, planScope: 'w', label: 'Implement API' } as const, false, 'Ready · Implement API'],
      [{ kind: 'working', since: 1, label: 'step' } as const, true, 'step'],
    [{ kind: 'verifying', since: 1, planScope: 'w' } as const, true, 'Verifying'],
    [{ kind: 'blocked', since: 1, label: 'reason' } as const, false, 'Blocked · reason'],
    [{ kind: 'complete', since: 1 } as const, false, 'Complete'],
    [{ kind: 'failed', since: 1, label: 'reason' } as const, false, 'Failed · reason'],
  ] as const;

  for (const [activity, visible, status] of cases) {
    const presentation = activityPresentation(activity);
    assert.equal(presentation.visible, visible, activity.kind);
    assert.equal(presentation.status, status, activity.kind);
  }
});
