import assert from 'node:assert/strict';
import { test } from 'vitest';
import { LifecycleBus } from '@octocodeai/agent-core';
import {
  PI_LIFECYCLE_MAPPINGS,
  bindPiLifecycleBus,
  createPiEventEnvelope,
} from '../src/adapters/pi-lifecycle-adapter.js';
import type { PiContext, PiInstance } from '../src/types.js';

function hostHarness() {
  const handlers = new Map<string, (...args: unknown[]) => unknown>();
  const pi = {
    on: (event: string, handler: (...args: unknown[]) => unknown) => handlers.set(event, handler),
    sendUserMessage: () => undefined,
  } as unknown as PiInstance;
  return { pi, handlers };
}

test('maps every production Pi lifecycle family to a canonical event', () => {
  const expected = [
    'resources_discover', 'project_trust', 'context', 'input', 'before_agent_start',
    'session_start', 'session_shutdown', 'session_info_changed', 'session_before_switch',
    'session_before_fork', 'session_before_compact', 'session_compact', 'session_compact_failed', 'session_tree',
    'agent_start', 'agent_end', 'turn_start', 'turn_end', 'message_start', 'message_end',
    'tool_call', 'tool_execution_start', 'tool_execution_end', 'model_select',
    'thinking_level_select', 'before_provider_request', 'after_provider_response',
  ];
  assert.deepEqual(Object.keys(PI_LIFECYCLE_MAPPINGS).sort(), expected.sort());
});

test('normalizes Pi context into an immutable canonical envelope', async () => {
  const signal = new AbortController().signal;
  const ctx = {
    cwd: '/workspace',
    mode: 'tui',
    model: { provider: 'openai', id: 'gpt-5' },
    isProjectTrusted: () => true,
    sessionManager: { getSessionId: () => 'session-1' },
  } as PiContext;
  const envelope = await createPiEventEnvelope('tool_call', { toolCallId: 'call-1', toolName: 'write', input: {}, signal }, ctx, 7);

  assert.equal(envelope.type, 'tool.requested');
  assert.equal(envelope.mode, 'interactive');
  assert.equal(envelope.sessionId, 'session-1');
  assert.deepEqual(envelope.model, { providerId: 'openai', modelId: 'gpt-5' });
  assert.deepEqual(envelope.trust, { workspace: 'trusted', managedOnly: false });
  assert.ok(Object.isFrozen(envelope.payload));
});

test('maps canonical deny and rewrite decisions back to Pi without widening authority', async () => {
  const { pi, handlers } = hostHarness();
  const toolBus = new LifecycleBus<Record<string, unknown>>({
    eventType: 'tool.requested',
    authority: ['allow-deny', 'rewrite'],
    validate: (payload): payload is Record<string, unknown> => Boolean(payload) && typeof payload === 'object',
  });
  toolBus.subscribe({ id: 'policy', source: 'builtin', handler: async () => ({ kind: 'deny', reason: 'locked' }) });
  bindPiLifecycleBus(pi, 'tool_call', toolBus);
  const toolResult = await handlers.get('tool_call')?.({ toolCallId: 'c', toolName: 'write', input: {} }, { cwd: '/w' });
  assert.deepEqual(toolResult, { block: true, reason: 'locked' });

  const inputBus = new LifecycleBus<Record<string, unknown>>({
    eventType: 'input.received',
    authority: ['rewrite', 'context', 'stop'],
    validate: (payload): payload is Record<string, unknown> => Boolean(payload)
      && typeof payload === 'object'
      && typeof (payload as Record<string, unknown>)['text'] === 'string',
  });
  inputBus.subscribe({ id: 'rewrite', source: 'user', handler: async () => ({ kind: 'rewrite', payload: { text: 'normalized', images: [] } }) });
  bindPiLifecycleBus(pi, 'input', inputBus);
  const inputResult = await handlers.get('input')?.({ text: 'raw', images: [] }, { cwd: '/w' });
  assert.deepEqual(inputResult, { action: 'transform', text: 'normalized', images: [] });
});

test('serializes concurrent tool_execution_end dispatches to prevent LifecycleBus reentrancy', async () => {
  // Reproduces: Extension error: Recursive intercepting event: tool.ended
  // Root cause: Pi fires tool_execution_end concurrently for parallel tools.
  // The LifecycleBus.dispatch() guard uses a per-type #active Set that throws
  // when two calls overlap. The dispatch queue in bindPiLifecycleBus must
  // serialize all calls so only one is in flight at a time.
  const { pi, handlers } = hostHarness();
  const bus = new LifecycleBus<Record<string, unknown>>({
    eventType: 'tool.ended',
    authority: ['observe'],
    validate: (payload): payload is Record<string, unknown> => Boolean(payload) && typeof payload === 'object',
  });
  const order: number[] = [];
  bus.subscribe({
    id: 'observer',
    source: 'builtin',
    // Slow handler so concurrent calls have time to overlap if not queued.
    handler: async (envelope) => {
      await new Promise((r) => setTimeout(r, 10));
      order.push((envelope.payload as { n: number }).n);
      return undefined;
    },
  });
  bindPiLifecycleBus(pi, 'tool_execution_end', bus);
  const handler = handlers.get('tool_execution_end')!;
  // Fire 5 concurrent "parallel tool" completions without awaiting individually.
  const results = await Promise.all([
    handler({ toolCallId: 'a', toolName: 'bash', result: '', isError: false, n: 0 }, { cwd: '/w' }),
    handler({ toolCallId: 'b', toolName: 'bash', result: '', isError: false, n: 1 }, { cwd: '/w' }),
    handler({ toolCallId: 'c', toolName: 'bash', result: '', isError: false, n: 2 }, { cwd: '/w' }),
    handler({ toolCallId: 'd', toolName: 'bash', result: '', isError: false, n: 3 }, { cwd: '/w' }),
    handler({ toolCallId: 'e', toolName: 'bash', result: '', isError: false, n: 4 }, { cwd: '/w' }),
  ]);
  // All 5 dispatches must complete without throwing.
  assert.equal(results.length, 5);
  // All must have been processed in FIFO order (the queue is first-in, first-out).
  assert.deepEqual(order, [0, 1, 2, 3, 4]);
});

test('preserves attributed context messages alongside before-agent-start prompt rewrites', async () => {
  const { pi, handlers } = hostHarness();
  const bus = new LifecycleBus<Record<string, unknown>>({
    eventType: 'agent.before-start',
    authority: ['rewrite', 'context', 'stop'],
    validate: (payload): payload is Record<string, unknown> => Boolean(payload) && typeof payload === 'object',
  });
  const message = {
    customType: 'octocode-context-update',
    content: '<active_plan>current</active_plan>',
    display: false,
    details: { version: 1, segments: [{ id: 'active-plan', kind: 'plan', authority: 'user' }] },
  };
  bus.subscribe({
    id: 'prompt-and-context',
    source: 'builtin',
    handler: async () => ({ kind: 'rewrite', payload: { systemPrompt: 'frozen policy', message } }),
  });
  bindPiLifecycleBus(pi, 'before_agent_start', bus);

  const result = await handlers.get('before_agent_start')?.({ systemPrompt: 'Pi base' }, { cwd: '/w' });

  assert.deepEqual(result, { systemPrompt: 'frozen policy', message });
});
