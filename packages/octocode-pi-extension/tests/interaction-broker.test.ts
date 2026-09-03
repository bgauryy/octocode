import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';
import type { InteractionAnswerV1, InteractionRequestV1 } from '@octocodeai/octocode-awareness';
import {
  answerPendingInteraction,
  consumeHumanAuthorizationReceipt,
  createHumanAuthorizationReceipt,
  createPendingInteraction,
  listPendingInteractionIds,
  setInteractionStoreFactoryForTests,
  submitHostInteractionAnswer,
} from '../src/tools/interaction-broker.js';
import type { PiContext } from '../src/types.js';

afterEach(() => setInteractionStoreFactoryForTests());

test('TUI and RPC broker requests use the same host-neutral payload', () => {
  const requests: InteractionRequestV1[] = [];
  const answers: InteractionAnswerV1[] = [];
  setInteractionStoreFactoryForTests(() => ({
    createInteraction: (request) => requests.push(request),
    answerInteraction: (answer) => answers.push(answer),
    close: () => undefined,
  }));
  const base = { cwd: '/repo', sessionManager: { getSessionId: () => 'session-1' } };
  const params = { question: 'Choose?', options: [{ id: 'safe', label: 'Safe', recommended: true }] };
  const tui = createPendingInteraction({ ...base, mode: 'tui' } as PiContext, params);
  const rpc = createPendingInteraction({ ...base, mode: 'rpc' } as PiContext, params);
  assert.deepEqual(
    { ...tui, interactionId: '', correlationId: '', createdAt: '', expiresAt: '' },
    { ...rpc, interactionId: '', correlationId: '', createdAt: '', expiresAt: '' },
  );
  const answer = answerPendingInteraction(rpc, { status: 'selected', value: 'safe' });
  assert.deepEqual(answer.optionIds, ['safe']);
  assert.equal(answers.length, 1);
  assert.equal(requests.length, 2);
});

test('cancellation remains explicit and never selects a recommended default', () => {
  let answer: InteractionAnswerV1 | undefined;
  setInteractionStoreFactoryForTests(() => ({
    createInteraction: () => undefined,
    answerInteraction: (value) => { answer = value; },
    close: () => undefined,
  }));
  const request = createPendingInteraction({ cwd: '/repo', mode: 'rpc' } as PiContext, {
    question: 'Deploy?', options: [{ id: 'yes', label: 'Yes', recommended: true }],
  });
  answerPendingInteraction(request, { status: 'cancelled' });
  assert.equal(answer?.cancelled, true);
  assert.equal(answer?.optionIds, undefined);
});

test('memory mode keeps interaction and authorization state in process only', () => {
  const previousVitest = process.env['VITEST'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  delete process.env['VITEST'];
  process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
  setInteractionStoreFactoryForTests();
  const ctx = {
    cwd: `/memory-workspace-${process.pid}`,
    mode: 'rpc',
    sessionManager: { getSessionId: () => 'memory-session' },
  } as PiContext;

  try {
    const request = createPendingInteraction(ctx, {
      question: 'Continue?',
      options: [{ id: 'yes', label: 'Yes' }],
    });
    assert.deepEqual(listPendingInteractionIds(ctx), [request.interactionId]);

    const answer = submitHostInteractionAnswer(ctx, {
      version: 1,
      interactionId: request.interactionId,
      correlationId: request.correlationId,
      sessionId: request.sessionId,
      outcome: { status: 'selected', value: 'yes' },
    });
    assert.deepEqual(answer.optionIds, ['yes']);
    assert.deepEqual(listPendingInteractionIds(ctx), []);

    const receipt = createHumanAuthorizationReceipt(ctx, {
      planId: 'plan-1',
      revision: 'revision-1',
      scope: 'workspace',
      question: 'Authorize?',
    });
    consumeHumanAuthorizationReceipt(ctx.cwd!, {
      receiptId: receipt.receiptId,
      planId: 'plan-1',
      revision: 'revision-1',
      scope: 'workspace',
    });
    assert.throws(() => consumeHumanAuthorizationReceipt(ctx.cwd!, {
      receiptId: receipt.receiptId,
      planId: 'plan-1',
      revision: 'revision-1',
      scope: 'workspace',
    }), /invalid or expired/);
  } finally {
    if (previousVitest === undefined) delete process.env['VITEST'];
    else process.env['VITEST'] = previousVitest;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    setInteractionStoreFactoryForTests();
  }
});
