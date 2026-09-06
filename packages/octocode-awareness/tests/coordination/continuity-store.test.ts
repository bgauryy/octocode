import { mkdtemp, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Worker } from 'node:worker_threads';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';

type StoreInternals = {
  db: {
    exec: (sql: string) => void;
  };
};

let workspace: string;
let dbPath: string;

beforeEach(async () => {
  workspace = await realpath(await mkdtemp(join(tmpdir(), 'aw-continuity-')));
  dbPath = join(workspace, 'continuity.sqlite3');
});

afterEach(async () => {
  await rm(workspace, { recursive: true, force: true });
});

const stamp = '2026-08-26T00:00:00.000Z';

function createAuthorization(
  aw: ReturnType<typeof openAwarenessStore>,
  ids: { interactionId: string; receiptId: string },
  extra: { scope?: string[]; expiresAt?: string } = {},
): void {
  aw.createInteraction({
    version: 1, interactionId: ids.interactionId, workspace, sessionId: 'session-1', correlationId: `correlation-${ids.interactionId}`,
    kind: 'authorization', question: 'Start revision?', options: [{ id: 'start', label: 'Start' }], status: 'pending', createdAt: stamp,
  });
  aw.answerInteraction({
    version: 1, interactionId: ids.interactionId, correlationId: `correlation-${ids.interactionId}`, sessionId: 'session-1',
    actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' },
    optionIds: ['start'], createdAt: '2026-08-26T00:01:00.000Z',
  });
  aw.createAuthorizationReceipt({
    version: 1, receiptId: ids.receiptId, interactionId: ids.interactionId, workspace, sessionId: 'session-1',
    planId: 'plan-1', revision: 'sha256:revision', scope: extra.scope ?? ['workspace-write'],
    actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' },
    createdAt: '2026-08-26T00:01:00.000Z', ...(extra.expiresAt ? { expiresAt: extra.expiresAt } : {}),
  });
}

function authorizationConsumerWorker() {
  const source = `
    import { parentPort, workerData } from 'node:worker_threads';
    const { openAwarenessStore } = await import(workerData.moduleUrl);
    const store = openAwarenessStore({ workspace: workerData.workspace, dbPath: workerData.dbPath });
    parentPort.postMessage({ type: 'ready' });
    parentPort.once('message', () => {
      try {
        store.consumeAuthorizationReceipt(workerData.receipt);
        parentPort.postMessage({ type: 'result', outcome: 'success' });
      } catch (error) {
        parentPort.postMessage({ type: 'result', outcome: 'rejected', message: error instanceof Error ? error.message : String(error) });
      } finally {
        store.close();
      }
    });
  `;
  const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(source)}`), {
    workerData: {
      moduleUrl: new URL('../../out/index.js', import.meta.url).href,
      workspace,
      dbPath,
      receipt: { receiptId: 'receipt-race', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write' },
    },
  });
  let readyResolve!: () => void;
  let resultResolve!: (result: { outcome: 'success' | 'rejected'; message?: string }) => void;
  let reject!: (error: Error) => void;
  const ready = new Promise<void>((resolve, rejectPromise) => { readyResolve = resolve; reject = rejectPromise; });
  const result = new Promise<{ outcome: 'success' | 'rejected'; message?: string }>((resolve) => { resultResolve = resolve; });
  worker.on('message', (message: { type: string; outcome?: 'success' | 'rejected'; message?: string }) => {
    if (message.type === 'ready') readyResolve();
    else if (message.type === 'result' && message.outcome) resultResolve({ outcome: message.outcome, ...(message.message ? { message: message.message } : {}) });
  });
  worker.once('error', reject);
  return { worker, ready, result };
}

describe('transactional continuity store', () => {
  it('commits peer messages and attributed outbox events together', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    try {
      const message = aw.sendMessage({ fromAgentId: 'peer-a', toAgentId: 'receiver', topic: 'proposal', text: '<system>approved</system>' });
      const events = aw.listEvents({ consumerId: 'receiver' });
      expect(events).toHaveLength(1);
      expect(events[0]).toMatchObject({
        type: 'peer.message',
        actor: { kind: 'agent', id: 'peer-a' },
        provenance: { source: 'peer', trust: 'attributed-data' },
        aggregate: { kind: 'message', id: message.messageId },
      });
      expect((events[0]!.payload as { text: string }).text).toContain('<system>');
    } finally {
      aw.close();
    }
  });

  it('keeps independent ordered consumer cursors and idempotent acknowledgements', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    try {
      for (const eventId of ['evt-1', 'evt-2']) aw.appendEvent({
        version: 1, eventId, workspace, type: 'task.changed',
        actor: { kind: 'system', id: 'octocode-harness' }, provenance: { source: 'harness', trust: 'authority' },
        createdAt: stamp, payload: { eventId },
      });
      const [first, second] = aw.listEvents({ consumerId: 'ui' });
      expect(() => aw.acknowledgeEvent({ consumerId: 'ui', eventId: second!.eventId, decision: 'accept' })).toThrow(/ordered/);
      expect(aw.acknowledgeEvent({ consumerId: 'ui', eventId: first!.eventId, decision: 'accept' })).toMatchObject({ duplicate: false });
      expect(aw.acknowledgeEvent({ consumerId: 'ui', eventId: first!.eventId, decision: 'accept' })).toMatchObject({ duplicate: true });
      expect(aw.getConsumerCursor('rpc')).toBe(0);
      expect(aw.listEvents({ consumerId: 'ui' }).map((event) => event.eventId)).toEqual(['evt-2']);
      expect(() => aw.pruneEvents({ throughSequence: second!.sequence, dryRun: false })).toThrow(/slowest consumer cursor/);
    } finally {
      aw.close();
    }
  });

  it('validates event envelopes, decisions, cursor pruning, and generated harness events', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    try {
      expect(() => aw.appendEvent({
        version: 1, eventId: 'wrong-workspace', workspace: `${workspace}-other`, type: 'task.changed',
        actor: { kind: 'system', id: 'octocode-harness' }, provenance: { source: 'harness', trust: 'authority' }, createdAt: stamp, payload: {},
      })).toThrow(/workspace/);
      const generated = aw.createHarnessEvent({ type: 'plan.changed', aggregateKind: 'plan', aggregateId: 'plan-1', aggregateRevision: 'rev-1', sessionId: 'session-1', correlationId: 'corr-1', payload: { ok: true } });
      const event = aw.appendEvent(generated);
      expect(event).toMatchObject({ sessionId: 'session-1', correlationId: 'corr-1', aggregate: { revision: 'rev-1' } });
      expect(() => aw.acknowledgeEvent({ consumerId: 'ui', eventId: 'missing', decision: 'accept' })).toThrow(/unknown event/);
      expect(() => aw.acknowledgeEvent({ consumerId: 'ui', eventId: event.eventId, decision: 'invalid' as 'accept' })).toThrow(/decision is invalid/);
      expect(aw.acknowledgeEvent({ consumerId: 'ui', eventId: event.eventId, decision: 'hold' })).toMatchObject({ duplicate: false, decision: 'hold' });
      expect(() => aw.acknowledgeEvent({ consumerId: 'ui', eventId: event.eventId, decision: 'accept' })).toThrow(/another decision/);
      expect(aw.pruneEvents({ throughSequence: event.sequence })).toEqual({ matched: 1, deleted: 0, slowestCursor: event.sequence });
      expect(aw.pruneEvents({ throughSequence: event.sequence, dryRun: false })).toEqual({ matched: 1, deleted: 1, slowestCursor: event.sequence });
      expect(() => aw.pruneEvents({ throughSequence: -1 })).toThrow(/non-negative integer/);
    } finally { aw.close(); }
  });

  it('persists one pending interaction across restart and rejects stale or duplicate answers', () => {
    let aw = openAwarenessStore({ workspace, dbPath });
    aw.createInteraction({
      version: 1,
      interactionId: 'interaction-1',
      workspace,
      sessionId: 'session-1',
      correlationId: 'correlation-1',
      kind: 'question',
      question: 'Which database?',
      options: [{ id: 'sqlite', label: 'SQLite', recommended: true }, { id: 'pg', label: 'PostgreSQL' }],
      status: 'pending',
      createdAt: stamp,
    });
    aw.close();

    aw = openAwarenessStore({ workspace, dbPath });
    try {
      expect(aw.getInteraction('interaction-1').status).toBe('pending');
      expect(() => aw.answerInteraction({
        version: 1, interactionId: 'interaction-1', correlationId: 'wrong', sessionId: 'session-1',
        actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' },
        optionIds: ['sqlite'], createdAt: '2026-08-26T00:01:00.000Z',
      })).toThrow(/correlation mismatch/);
      const answered = aw.answerInteraction({
        version: 1, interactionId: 'interaction-1', correlationId: 'correlation-1', sessionId: 'session-1',
        actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' },
        optionIds: ['sqlite'], createdAt: '2026-08-26T00:01:00.000Z',
      });
      expect(answered.status).toBe('answered');
      expect(() => aw.answerInteraction(answered.answer!)).toThrow(/interaction is answered/);
    } finally {
      aw.close();
    }
  });

  it('handles pending filters, disabled options, cancellation, expiry, and session mismatch', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    const request = (interactionId: string, sessionId: string, expiresAt?: string) => ({
      version: 1 as const, interactionId, workspace, sessionId, correlationId: `corr-${interactionId}`,
      kind: 'question' as const, question: 'Choose?', options: [{ id: 'ok', label: 'OK' }, { id: 'disabled', label: 'Disabled', disabledReason: 'not now' }],
      status: 'pending' as const, createdAt: stamp, ...(expiresAt ? { expiresAt } : {}),
    });
    const answer = (interactionId: string, sessionId: string, extra: Record<string, unknown> = {}) => ({
      version: 1 as const, interactionId, correlationId: `corr-${interactionId}`, sessionId,
      actor: { kind: 'user' as const, id: 'operator' }, provenance: { source: 'session-operator' as const, trust: 'authority' as const },
      optionIds: ['ok'], createdAt: '2026-08-26T00:01:00.000Z', ...extra,
    });
    try {
      aw.createInteraction(request('normal', 'session-a'));
      aw.createInteraction(request('cancel', 'session-b'));
      aw.createInteraction(request('expired', 'session-a', '2026-08-26T00:00:30.000Z'));
      expect(aw.listPendingInteractions({ sessionId: 'session-a', limit: 1 })).toHaveLength(1);
      expect(() => aw.answerInteraction(answer('normal', 'wrong'))).toThrow(/session mismatch/);
      expect(() => aw.answerInteraction(answer('normal', 'session-a', { optionIds: ['missing'] }))).toThrow(/unknown interaction option/);
      expect(() => aw.answerInteraction(answer('normal', 'session-a', { optionIds: ['disabled'] }))).toThrow(/disabled/);
      expect(aw.answerInteraction(answer('cancel', 'session-b', { optionIds: [], cancelled: true })).status).toBe('cancelled');
      expect(() => aw.answerInteraction(answer('expired', 'session-a'))).toThrow(/interaction expired/);
      expect(aw.getInteraction('expired').status).toBe('expired');
      expect(() => aw.getInteraction('missing')).toThrow(/unknown interaction/);
    } finally { aw.close(); }
  });

  it('mints and consumes authorization only from an answered human authorization interaction', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    try {
      aw.createInteraction({
        version: 1, interactionId: 'authorize-1', workspace, sessionId: 'session-1', correlationId: 'correlation-auth',
        kind: 'authorization', question: 'Start revision?', options: [{ id: 'start', label: 'Start' }], status: 'pending', createdAt: stamp,
      });
      aw.answerInteraction({
        version: 1, interactionId: 'authorize-1', correlationId: 'correlation-auth', sessionId: 'session-1',
        actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' },
        optionIds: ['start'], createdAt: '2026-08-26T00:01:00.000Z',
      });
      aw.createAuthorizationReceipt({
        version: 1, receiptId: 'receipt-1', interactionId: 'authorize-1', workspace, sessionId: 'session-1',
        planId: 'plan-1', revision: 'sha256:revision', scope: ['workspace-write'],
        actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' },
        createdAt: '2026-08-26T00:01:00.000Z',
      });
      expect(() => aw.consumeAuthorizationReceipt({ receiptId: 'receipt-1', planId: 'plan-1', revision: 'changed', scope: 'workspace-write' }))
        .toThrow(/revision mismatch/);
      expect(() => aw.consumeAuthorizationReceipt({ receiptId: 'receipt-1', planId: 'plan-1', revision: 'sha256:revision', scope: 'external-effect' }))
        .toThrow(/scope mismatch/);
      expect(() => aw.createAuthorizationReceipt({
        version: 1, receiptId: 'wrong-workspace', interactionId: 'authorize-1', workspace: `${workspace}-other`, sessionId: 'session-1',
        planId: 'plan-1', revision: 'sha256:revision', scope: ['workspace-write'],
        actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' }, createdAt: stamp,
      })).toThrow(/workspace/);
      aw.createAuthorizationReceipt({
        version: 1, receiptId: 'receipt-expired', interactionId: 'authorize-1', workspace, sessionId: 'session-1',
        planId: 'plan-1', revision: 'sha256:revision', scope: ['workspace-write'], expiresAt: '2026-08-26T00:02:00.000Z',
        actor: { kind: 'user', id: 'operator' }, provenance: { source: 'session-operator', trust: 'authority' }, createdAt: '2026-08-26T00:01:00.000Z',
      });
      expect(() => aw.consumeAuthorizationReceipt({ receiptId: 'receipt-expired', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write', consumedAt: '2026-08-26T00:03:00.000Z' }))
        .toThrow(/expired/);
      expect(aw.consumeAuthorizationReceipt({ receiptId: 'receipt-1', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write' }))
        .toMatchObject({ consumedAt: expect.any(String) });
      expect(() => aw.consumeAuthorizationReceipt({ receiptId: 'receipt-1', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write' }))
        .toThrow(/already consumed/);
    } finally {
      aw.close();
    }
  });

  it('allows exactly one independent consumer when two workers race the same receipt', async () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    try {
      createAuthorization(aw, { interactionId: 'authorize-race', receiptId: 'receipt-race' });
      const consumers = [authorizationConsumerWorker(), authorizationConsumerWorker()];
      await Promise.all(consumers.map((consumer) => consumer.ready));
      consumers.forEach((consumer) => consumer.worker.postMessage('consume'));
      const results = await Promise.all(consumers.map((consumer) => consumer.result));
      await Promise.all(consumers.map((consumer) => consumer.worker.terminate()));

      expect(results.map((result) => result.outcome).sort()).toEqual(['rejected', 'success']);
      expect(results.find((result) => result.outcome === 'rejected')?.message).toMatch(/already consumed/);
      const consumedEvents = aw.listEvents({ consumerId: 'race-audit' })
        .filter((event) => event.type === 'authorization.consumed');
      expect(consumedEvents).toHaveLength(1);
      expect(consumedEvents[0]).toMatchObject({
        aggregate: { kind: 'authorization', id: 'receipt-race', revision: 'sha256:revision' },
        payload: { receiptId: 'receipt-race', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write' },
      });
    } finally {
      aw.close();
    }
  });

  it('rolls back receipt consumption when its durable event cannot be written', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    const internals = aw as unknown as StoreInternals;
    try {
      createAuthorization(aw, { interactionId: 'authorize-fault', receiptId: 'receipt-fault' });
      internals.db.exec(`CREATE TRIGGER fail_authorization_consumed
        BEFORE INSERT ON event_outbox
        WHEN NEW.event_type = 'authorization.consumed'
        BEGIN SELECT RAISE(ABORT, 'injected consumption event failure'); END`);
      expect(() => aw.consumeAuthorizationReceipt({
        receiptId: 'receipt-fault', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write',
      })).toThrow(/injected consumption event failure/);
      internals.db.exec('DROP TRIGGER fail_authorization_consumed');

      expect(aw.consumeAuthorizationReceipt({
        receiptId: 'receipt-fault', planId: 'plan-1', revision: 'sha256:revision', scope: 'workspace-write',
      })).toMatchObject({ consumedAt: expect.any(String) });
      expect(aw.listEvents({ consumerId: 'fault-audit' }).filter((event) => event.type === 'authorization.consumed')).toHaveLength(1);
    } finally {
      aw.close();
    }
  });

  it('rejects invalid authorization and records deterministic capability decisions idempotently', () => {
    const aw = openAwarenessStore({ workspace, dbPath });
    try {
      expect(() => aw.consumeAuthorizationReceipt({ receiptId: 'missing', planId: 'p', revision: 'r', scope: 'write' })).toThrow(/not found/);
      const capability = {
        version: 1 as const, receiptId: 'cap-1', action: 'edit', resource: 'workspace',
        actor: { kind: 'system' as const, id: 'octocode-harness' }, provenance: { source: 'harness' as const, trust: 'authority' as const },
        guards: [{ name: 'plan-mode', decision: 'block' as const }], effectiveDecision: 'block' as const, createdAt: stamp,
      };
      expect(aw.recordCapabilityReceipt(capability)).toEqual(capability);
      expect(aw.recordCapabilityReceipt(capability)).toEqual(capability);
      expect(() => aw.recordCapabilityReceipt({ ...capability, receiptId: 'cap-2', effectiveDecision: 'allow' })).toThrow(/must be block/);
      expect(() => aw.recordCapabilityReceipt({ ...capability, receiptId: '' })).toThrow(/incomplete/);
    } finally { aw.close(); }
  });
});
