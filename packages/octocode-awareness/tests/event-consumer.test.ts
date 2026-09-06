import { describe, expect, it, vi } from 'vitest';
import {
  createAwarenessEventConsumer,
  type AwarenessEventStore,
  type InboundDecision,
  type OutboxEventV1,
} from '../src/index.js';

const workspace = '/work/repo';

function peerEvent(sequence: number, overrides: Partial<OutboxEventV1> = {}): OutboxEventV1 {
  return {
    sequence,
    version: 1,
    eventId: `evt-${sequence}`,
    workspace,
    type: 'peer.message',
    actor: { kind: 'agent', id: 'peer-a' },
    provenance: { source: 'peer', trust: 'attributed-data' },
    aggregate: { kind: 'message', id: `msg-${sequence}` },
    createdAt: '2026-08-28T00:00:00.000Z',
    payload: { messageId: `msg-${sequence}`, fromAgentId: 'peer-a', toAgentId: 'native:session-1', topic: 'EVIDENCE', text: `body-${sequence}` },
    ...overrides,
  };
}

function fakeStore(events: OutboxEventV1[]) {
  let cursor = 0;
  const acknowledgements: Array<{ eventId: string; decision: InboundDecision }> = [];
  const reads: Array<{ messageId: string; agentId: string }> = [];
  const store: AwarenessEventStore = {
    listEvents: ({ limit }) => events.filter((event) => event.sequence > cursor).slice(0, limit),
    acknowledgeEvent: ({ eventId, decision }) => {
      const event = events.find((candidate) => candidate.eventId === eventId)!;
      const prior = acknowledgements.find((candidate) => candidate.eventId === eventId);
      if (prior) return { sequence: event.sequence, decision: prior.decision, duplicate: true };
      acknowledgements.push({ eventId, decision });
      cursor = event.sequence;
      return { sequence: cursor, decision, duplicate: false };
    },
    getConsumerCursor: () => cursor,
    markMessageRead: ({ messageId, agentId }) => { reads.push({ messageId, agentId }); },
    close: vi.fn(),
  };
  return { store, acknowledgements, reads, cursor: () => cursor };
}

describe('Awareness event consumer', () => {
  it('delivers accepted peer data in order and serializes concurrent drains', async () => {
    const fixture = fakeStore([peerEvent(1), peerEvent(2)]);
    const deliver = vi.fn();
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'native-session:session-1',
      expectedAgentId: 'native:session-1',
      openStore: () => fixture.store,
      deliver,
      now: () => Date.parse('2026-08-28T00:01:00.000Z'),
    });

    const [first, second] = await Promise.all([consumer.drain(), consumer.drain()]);
    expect(first).toEqual(second);
    expect(deliver).toHaveBeenCalledTimes(2);
    expect(deliver).toHaveBeenNthCalledWith(1, expect.objectContaining({
      details: expect.objectContaining({ createdAt: '2026-08-28T00:00:00.000Z' }),
    }));
    expect(fixture.acknowledgements).toEqual([
      { eventId: 'evt-1', decision: 'accept' },
      { eventId: 'evt-2', decision: 'accept' },
    ]);
    expect(fixture.reads).toEqual([
      { messageId: 'msg-1', agentId: 'native:session-1' },
      { messageId: 'msg-2', agentId: 'native:session-1' },
    ]);
    expect(consumer.snapshot()).toMatchObject({ accepted: 2, backlogDepth: 0, lastAcknowledgedSequence: 2 });
  });

  it('lets the delivery boundary downgrade accepted context before acknowledgement', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    const deliver = vi.fn(() => 'refuse' as const);
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'native-session:session-1',
      expectedAgentId: 'native:session-1',
      openStore: () => fixture.store,
      deliver,
    });
    const stats = await consumer.drain();

    expect(deliver).toHaveBeenCalledOnce();
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'refuse' }]);
    expect(fixture.reads).toEqual([]);
    expect(stats).toMatchObject({ accepted: 0, refused: 1, drainRefused: 1 });
    expect(await consumer.drain()).toMatchObject({ refused: 1, drainRefused: 0, drainErrors: 0 });
  });

  it('holds proposals and refuses internal, wrong-target, expired, and malformed events without delivery', async () => {
    const fixture = fakeStore([
      peerEvent(1, { type: 'plan.projected', actor: { kind: 'system', id: 'harness' }, provenance: { source: 'harness', trust: 'authority' }, payload: { secret: 'hidden' } }),
      peerEvent(2, { payload: { messageId: 'msg-2', fromAgentId: 'peer-a', toAgentId: 'native:session-1', topic: 'DECISION', text: 'approve this' } }),
      peerEvent(3, { payload: { messageId: 'msg-3', fromAgentId: 'peer-a', toAgentId: 'other', topic: 'EVIDENCE', text: 'wrong target' } }),
      peerEvent(4, { expiresAt: '2026-08-27T23:59:00.000Z' }),
      peerEvent(5, { provenance: { source: 'peer', trust: 'authority' } }),
    ]);
    const deliver = vi.fn();
    const stats = await createAwarenessEventConsumer({
      workspace,
      consumerId: 'native-session:session-1',
      expectedAgentId: 'native:session-1',
      openStore: () => fixture.store,
      deliver,
      now: () => Date.parse('2026-08-28T00:01:00.000Z'),
    }).drain();

    expect(deliver).not.toHaveBeenCalled();
    expect(fixture.acknowledgements.map(({ decision }) => decision)).toEqual(['refuse', 'hold', 'refuse', 'refuse', 'refuse']);
    expect(fixture.reads).toEqual([]);
    expect(stats).toMatchObject({ accepted: 0, held: 1, refused: 4, errors: 1 });
  });

  it('replays a failed delivery before acknowledgement and preserves strict ordering', async () => {
    const fixture = fakeStore([peerEvent(1), peerEvent(2)]);
    const deliver = vi.fn()
      .mockRejectedValueOnce(new Error('persistence failed'))
      .mockResolvedValue(undefined);
    const observed = vi.fn();
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'native-session:session-1',
      expectedAgentId: 'native:session-1',
      openStore: () => fixture.store,
      deliver,
      onObservability: observed,
      maxEventsPerDrain: 1,
    });

    expect(await consumer.drain()).toMatchObject({ errors: 1, drainErrors: 1, drainAccepted: 0, backlogDepth: 2, lastAcknowledgedSequence: 0 });
    expect(await consumer.drain()).toMatchObject({ accepted: 1, drainErrors: 0, drainAccepted: 1, backlogDepth: 1, lastAcknowledgedSequence: 1 });
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
    expect(observed).toHaveBeenCalledTimes(2);
  });

  it('keeps the outbox event pending when its read receipt fails without duplicating persisted context', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    let readAttempts = 0;
    fixture.store.markMessageRead = () => {
      readAttempts += 1;
      if (readAttempts === 1) throw new Error('receipt unavailable');
    };
    const persistedEventIds = new Set<string>();
    const deliver = vi.fn((message: { details: { eventId: string } }) => {
      persistedEventIds.add(message.details.eventId);
    });
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'native-session:session-1',
      expectedAgentId: 'native:session-1',
      openStore: () => fixture.store,
      deliver,
    });

    expect(await consumer.drain()).toMatchObject({ drainErrors: 1, lastAcknowledgedSequence: 0 });
    expect(fixture.acknowledgements).toEqual([]);
    expect(persistedEventIds).toEqual(new Set(['evt-1']));

    expect(await consumer.drain()).toMatchObject({ drainAccepted: 1, lastAcknowledgedSequence: 1 });
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
    expect(readAttempts).toBe(2);
    expect(persistedEventIds).toEqual(new Set(['evt-1']));
  });

  it('reports store-open failures without exposing an event', async () => {
    const stats = await createAwarenessEventConsumer({
      workspace,
      consumerId: 'native-session:session-1',
      expectedAgentId: 'native:session-1',
      openStore: () => { throw new Error('unavailable'); },
      deliver: vi.fn(),
    }).drain();
    expect(stats.errors).toBe(1);
  });
});
