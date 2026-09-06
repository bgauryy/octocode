import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import type { InteractionAnswerV1, InteractionRequestV1 } from '@octocodeai/octocode-awareness';
import {
  answerPendingInteraction,
  clearInMemoryInteractionState,
  consumeHumanAuthorizationReceipt,
  createHumanAuthorizationReceipt,
  createHumanAuthorizationReceiptFromInteraction,
  createPendingInteraction,
  listPendingInteractionIds,
  setInteractionStoreFactoryForTests,
  submitHostInteractionAnswer,
} from '../src/tools/interaction-broker.js';
import type { PiContext } from '../src/types.js';

afterEach(() => {
  setInteractionStoreFactoryForTests();
  clearInMemoryInteractionState();
  vi.useRealTimers();
});

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

test('memory mode evicts answered questions and releases empty workspaces', () => {
  const previousVitest = process.env['VITEST'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  delete process.env['VITEST'];
  process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
  setInteractionStoreFactoryForTests();
  const ctx = {
    cwd: `/memory-eviction-${process.pid}`,
    mode: 'rpc',
    sessionManager: { getSessionId: () => 'memory-eviction-session' },
  } as PiContext;

  try {
    for (let index = 0; index < 100; index += 1) {
      const request = createPendingInteraction(ctx, {
        question: `Continue ${index}?`,
        options: [{ id: 'yes', label: 'Yes' }],
      });
      submitHostInteractionAnswer(ctx, {
        version: 1,
        interactionId: request.interactionId,
        correlationId: request.correlationId,
        sessionId: request.sessionId,
        outcome: { status: 'selected', value: 'yes' },
      });
      assert.throws(() => submitHostInteractionAnswer(ctx, {
        version: 1,
        interactionId: request.interactionId,
        correlationId: request.correlationId,
        sessionId: request.sessionId,
        outcome: { status: 'selected', value: 'yes' },
      }), /not found/);
    }
    assert.deepEqual(clearInMemoryInteractionState(), { workspaces: 0, interactions: 0, receipts: 0 });
  } finally {
    if (previousVitest === undefined) delete process.env['VITEST'];
    else process.env['VITEST'] = previousVitest;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    setInteractionStoreFactoryForTests();
  }
});

test('memory mode retains authorization answers only until receipt creation', () => {
  const previousVitest = process.env['VITEST'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  delete process.env['VITEST'];
  process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
  setInteractionStoreFactoryForTests();
  const ctx = {
    cwd: `/memory-authorization-${process.pid}`,
    mode: 'rpc',
    sessionManager: { getSessionId: () => 'memory-authorization-session' },
  } as PiContext;

  try {
    const request = createPendingInteraction(ctx, {
      kind: 'authorization',
      question: 'Start?',
      options: [{ id: 'start', label: 'Start' }],
    });
    submitHostInteractionAnswer(ctx, {
      version: 1,
      interactionId: request.interactionId,
      correlationId: request.correlationId,
      sessionId: request.sessionId,
      outcome: { status: 'selected', value: 'start' },
    });
    const acceptReceipt = createHumanAuthorizationReceiptFromInteraction(ctx, {
      interactionId: request.interactionId,
      planId: 'plan-auth',
      revision: 'revision-auth',
      scope: 'plan.accept',
      expectedOptionId: 'start',
      consumeInteraction: false,
    });
    const startReceipt = createHumanAuthorizationReceiptFromInteraction(ctx, {
      interactionId: request.interactionId,
      planId: 'plan-auth',
      revision: 'revision-auth',
      scope: 'plan.start',
      expectedOptionId: 'start',
    });
    assert.throws(() => createHumanAuthorizationReceiptFromInteraction(ctx, {
      interactionId: request.interactionId,
      planId: 'plan-auth',
      revision: 'revision-auth',
      scope: 'workspace',
      expectedOptionId: 'start',
    }), /not found/);
    consumeHumanAuthorizationReceipt(ctx.cwd!, {
      receiptId: acceptReceipt.receiptId,
      planId: 'plan-auth',
      revision: 'revision-auth',
      scope: 'plan.accept',
    });
    consumeHumanAuthorizationReceipt(ctx.cwd!, {
      receiptId: startReceipt.receiptId,
      planId: 'plan-auth',
      revision: 'revision-auth',
      scope: 'plan.start',
    });
    assert.deepEqual(clearInMemoryInteractionState(), { workspaces: 0, interactions: 0, receipts: 0 });
  } finally {
    if (previousVitest === undefined) delete process.env['VITEST'];
    else process.env['VITEST'] = previousVitest;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    setInteractionStoreFactoryForTests();
  }
});

test('memory mode prunes expired entries and clears only the shutting-down session', () => {
  const previousVitest = process.env['VITEST'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  delete process.env['VITEST'];
  process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
  setInteractionStoreFactoryForTests();
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-01-01T00:00:00.000Z'));
  const workspace = `/memory-cleanup-${process.pid}`;
  const first = {
    cwd: workspace,
    mode: 'rpc',
    sessionManager: { getSessionId: () => 'first-session' },
  } as PiContext;
  const second = {
    cwd: workspace,
    mode: 'rpc',
    sessionManager: { getSessionId: () => 'second-session' },
  } as PiContext;

  try {
    const expired = createPendingInteraction(first, {
      question: 'Too late?',
      options: [{ id: 'yes', label: 'Yes' }],
      expiresInMs: 10,
    });
    const expiringReceipt = createHumanAuthorizationReceipt(first, {
      planId: 'plan-expired',
      revision: 'revision-expired',
      scope: 'workspace',
      question: 'Authorize briefly?',
      expiresInMs: 10,
    });
    vi.advanceTimersByTime(11);
    assert.deepEqual(listPendingInteractionIds(first), []);
    assert.throws(() => submitHostInteractionAnswer(first, {
      version: 1,
      interactionId: expired.interactionId,
      correlationId: expired.correlationId,
      sessionId: expired.sessionId,
      outcome: { status: 'selected', value: 'yes' },
    }), /not found/);
    assert.throws(() => consumeHumanAuthorizationReceipt(workspace, {
      receiptId: expiringReceipt.receiptId,
      planId: 'plan-expired',
      revision: 'revision-expired',
      scope: 'workspace',
    }), /invalid or expired/);

    createPendingInteraction(first, {
      question: 'First?', options: [{ id: 'yes', label: 'Yes' }],
    });
    const secondPending = createPendingInteraction(second, {
      question: 'Second?', options: [{ id: 'yes', label: 'Yes' }],
    });
    assert.deepEqual(clearInMemoryInteractionState({ workspace, sessionId: 'first-session' }), {
      workspaces: 0,
      interactions: 1,
      receipts: 0,
    });
    assert.deepEqual(listPendingInteractionIds(first), []);
    assert.deepEqual(listPendingInteractionIds(second), [secondPending.interactionId]);
    assert.deepEqual(clearInMemoryInteractionState({ workspace, sessionId: 'second-session' }), {
      workspaces: 1,
      interactions: 1,
      receipts: 0,
    });
    assert.deepEqual(clearInMemoryInteractionState(), { workspaces: 0, interactions: 0, receipts: 0 });
  } finally {
    if (previousVitest === undefined) delete process.env['VITEST'];
    else process.env['VITEST'] = previousVitest;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    setInteractionStoreFactoryForTests();
  }
});
