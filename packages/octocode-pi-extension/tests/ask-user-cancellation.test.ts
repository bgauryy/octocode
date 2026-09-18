import assert from 'node:assert/strict';
import { getEventListeners } from 'node:events';
import { test } from 'vitest';
import { runAskPrompt } from '../src/tools/ask-user-tool.js';
import { configureInteractionBrokerRoute, setInteractionStoreFactoryForTests } from '../src/tools/interaction-broker.js';
import { loadTool, overlayCtx } from './helpers/ask-user-harness.js';

test('askUser abort closes an active question without accepting a late answer', async () => {
  const { ctx, send } = overlayCtx();
  ctx.sessionManager = { getSessionId: () => 'cancelled-question-session' };
  configureInteractionBrokerRoute(ctx, true);
  const controller = new AbortController();
  const reason = new Error('operation cancelled');
  const answers: Array<{ cancelled?: boolean; optionIds?: string[] }> = [];
  let deleted = 0;
  setInteractionStoreFactoryForTests(() => ({
    createInteraction: () => undefined,
    answerInteraction: answer => { answers.push(answer); },
    deleteInteraction: () => { deleted += 1; },
    close: () => undefined,
  }));
  const pending = loadTool().execute('abort-question', {
    question: 'Continue?', options: ['Yes', 'No'], timeoutMs: 60_000,
  }, controller.signal, undefined, ctx);
  const settled = pending.then(() => 'answered', error => error);
  controller.abort(reason);
  let deadline: ReturnType<typeof setTimeout> | undefined;
  try {
    const result = await Promise.race([
      settled,
      new Promise(resolve => { deadline = setTimeout(() => resolve('still pending'), 100); }),
    ]);
    assert.equal(result, reason);
    assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
    assert.equal(deleted, 1, 'the cancelled interaction cannot accept a later answer');
    assert.deepEqual(answers.map(answer => [answer.cancelled, answer.optionIds]), [[true, undefined]]);
    send('\r');
    assert.equal(await settled, reason);
  } finally {
    clearTimeout(deadline);
    send('\x1b');
    await settled;
    setInteractionStoreFactoryForTests();
  }
});

test('askUser rejects a pre-aborted operation before opening UI', async () => {
  const { ctx, render } = overlayCtx();
  const controller = new AbortController();
  const reason = new Error('already cancelled');
  controller.abort(reason);
  const pending = loadTool().execute('pre-aborted', { question: 'Continue?' }, controller.signal, undefined, ctx);
  assert.deepEqual(render(), [], 'no question should open after cancellation');
  await assert.rejects(pending, error => error === reason);
});

test('runAskPrompt observes the host signal and removes its listener after an answer', async () => {
  const { ctx, send } = overlayCtx();
  const controller = new AbortController();
  ctx.signal = controller.signal;
  const pending = runAskPrompt(ctx, { question: 'Choose?', options: [{ value: 'yes' }], durable: false });
  assert.equal(getEventListeners(controller.signal, 'abort').length, 1);
  send('\r');
  assert.equal((await pending)?.status, 'selected');
  assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
  controller.abort();
});

test('runAskPrompt propagates host cancellation instead of a user answer', async () => {
  const { ctx, send } = overlayCtx();
  const controller = new AbortController();
  ctx.signal = controller.signal;
  const reason = new Error('host cancelled prompt');
  const pending = runAskPrompt(ctx, { question: 'Choose?', options: [{ value: 'yes' }], durable: false });
  controller.abort(reason);
  send('\r');
  await assert.rejects(pending, error => error === reason);
  assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
});
