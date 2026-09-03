import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { openAwareness } from '@octocodeai/octocode-awareness';

import {
  registerInteractionBrokerAdapter,
  type InteractionBrokerAdapterRegistry,
  type RegisteredInteractionBrokerAdapter,
} from '../src/tools/interaction-broker-adapter.js';
import { createPendingInteraction, setInteractionStoreFactoryForTests } from '../src/tools/interaction-broker.js';
import type { PiContext } from '../src/types.js';

const roots: string[] = [];
afterEach(() => {
  setInteractionStoreFactoryForTests();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function fixture(sessionId = 'rpc-session'): { ctx: PiContext } {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'interaction-adapter-'));
  roots.push(root);
  const workspace = path.join(root, 'workspace');
  const dbPath = path.join(root, 'agent.sqlite3');
  fs.mkdirSync(workspace, { recursive: true });
  setInteractionStoreFactoryForTests((storeWorkspace) => openAwareness({ workspace: storeWorkspace, dbPath }));
  return {
    ctx: {
      cwd: workspace,
      mode: 'rpc',
      sessionManager: { getSessionId: () => sessionId },
    } as PiContext,
  };
}

test('registered adapter lists, answers, resumes once, and remains drained after restart', async () => {
  const { ctx } = fixture();
  const delivered: Array<{ id: string; prompt: string }> = [];
  let adapter: RegisteredInteractionBrokerAdapter | undefined;
  const registry: InteractionBrokerAdapterRegistry = {
    registerInteractionBrokerAdapter(value): void { adapter = value; },
  };
  registerInteractionBrokerAdapter(registry, {
    deliver: (continuation, prompt) => { delivered.push({ id: continuation.continuationId, prompt }); },
  });
  const request = createPendingInteraction(ctx, { question: 'Ship?', options: [{ id: 'yes', label: 'Yes' }] });
  assert.deepEqual(adapter!.listPending(ctx).map((item) => item.interactionId), [request.interactionId]);

  adapter!.submitAnswer(ctx, {
    version: 1,
    interactionId: request.interactionId,
    correlationId: request.correlationId,
    sessionId: request.sessionId,
    outcome: { status: 'selected', value: 'yes' },
  });
  const first = await adapter!.drain(ctx);
  assert.equal(first.delivered, 1);
  assert.match(delivered[0]!.prompt, /selected option\(s\): yes/);

  let restarted: RegisteredInteractionBrokerAdapter | undefined;
  registerInteractionBrokerAdapter({ registerInteractionBrokerAdapter: (value) => { restarted = value; } }, {
    deliver: () => { throw new Error('acknowledged continuation must not redeliver'); },
  });
  assert.equal((await restarted!.drain(ctx)).delivered, 0);
});

test('delivery failure leaves the durable continuation available for restart redelivery', async () => {
  const { ctx } = fixture('retry-session');
  let first: RegisteredInteractionBrokerAdapter | undefined;
  registerInteractionBrokerAdapter({ registerInteractionBrokerAdapter: (value) => { first = value; } }, {
    deliver: () => { throw new Error('host crashed before durable delivery'); },
  });
  const request = createPendingInteraction(ctx, { question: 'Target?', options: [{ id: 'core', label: 'Core' }] });
  first!.submitAnswer(ctx, { version: 1, interactionId: request.interactionId, correlationId: request.correlationId, sessionId: request.sessionId, outcome: { status: 'selected', value: 'core' } });
  await assert.rejects(first!.drain(ctx), /host crashed/);

  const ids: string[] = [];
  let restarted: RegisteredInteractionBrokerAdapter | undefined;
  registerInteractionBrokerAdapter({ registerInteractionBrokerAdapter: (value) => { restarted = value; } }, {
    deliver: (continuation) => { ids.push(continuation.continuationId); },
  });
  assert.equal((await restarted!.drain(ctx)).delivered, 1);
  assert.deepEqual(ids, [`evt_${request.interactionId}_answered`]);
});

test('host answer submission rejects wrong session, correlation, expiry, and duplicate answers', async () => {
  const { ctx } = fixture('safe-session');
  let adapter: RegisteredInteractionBrokerAdapter | undefined;
  registerInteractionBrokerAdapter({ registerInteractionBrokerAdapter: (value) => { adapter = value; } }, { deliver: () => undefined });
  const request = createPendingInteraction(ctx, { question: 'Choose?', options: [{ id: 'safe', label: 'Safe' }] });
  const base = { version: 1 as const, interactionId: request.interactionId, correlationId: request.correlationId, sessionId: request.sessionId, outcome: { status: 'selected', value: 'safe' } };
  assert.throws(() => adapter!.submitAnswer(ctx, { ...base, sessionId: 'other-session' }), /session mismatch/);
  assert.throws(() => adapter!.submitAnswer(ctx, { ...base, correlationId: 'wrong' }), /correlation mismatch/);
  assert.throws(() => adapter!.submitAnswer(ctx, { ...base, outcome: { status: 'invented' } }), /status is unsupported/);
  adapter!.submitAnswer(ctx, base);
  assert.throws(() => adapter!.submitAnswer(ctx, base), /answered/);

  const expiring = createPendingInteraction(ctx, { question: 'Fast?', options: [{ id: 'yes', label: 'Yes' }], expiresInMs: 1 });
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.throws(() => adapter!.submitAnswer(ctx, { version: 1, interactionId: expiring.interactionId, correlationId: expiring.correlationId, sessionId: expiring.sessionId, outcome: { status: 'selected', value: 'yes' } }), /expired/);
});

test('cancel continuation resumes explicitly without selecting a recommended default', async () => {
  const { ctx } = fixture('cancel-session');
  const statuses: string[] = [];
  let adapter: RegisteredInteractionBrokerAdapter | undefined;
  registerInteractionBrokerAdapter({ registerInteractionBrokerAdapter: (value) => { adapter = value; } }, {
    deliver: (continuation, prompt) => { statuses.push(`${continuation.status}:${prompt}`); },
  });
  const request = createPendingInteraction(ctx, { question: 'Deploy?', options: [{ id: 'yes', label: 'Yes', recommended: true }] });
  adapter!.submitAnswer(ctx, { version: 1, interactionId: request.interactionId, correlationId: request.correlationId, sessionId: request.sessionId, outcome: { status: 'cancelled' } });
  await adapter!.drain(ctx);
  assert.match(statuses[0]!, /^cancelled:.*cancelled this question/s);
  assert.doesNotMatch(statuses[0]!, /selected option/);
});
