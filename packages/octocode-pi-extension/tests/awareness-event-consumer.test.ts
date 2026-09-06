import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { openAwarenessStore, type InboundDecision, type OutboxEventV1 } from '@octocodeai/octocode-awareness';
import { awarenessEventStatusText, registerAwarenessEventConsumer, resolvePiEventConsumerId } from '../src/tools/awareness-event-consumer.js';
import { createAwarenessEventConsumer, type AwarenessEventStore } from '@octocodeai/octocode-awareness';
import type { PiContext, PiInstance } from '../src/types.js';

const workspace = '/work/repo';
const tempRoots: string[] = [];

describe('Awareness event status projection', () => {
  const stats = {
    consumerId: 'pi:session-1',
    backlogDepth: 0,
    backlogCapped: false,
    lastAcknowledgedSequence: 7,
    accepted: 4,
    held: 2,
    refused: 1,
    errors: 3,
    drainAccepted: 0,
    drainHeld: 0,
    drainRefused: 0,
    drainErrors: 0,
  };

  it('clears recovered UI attention even when lifetime diagnostics retain old failures', () => {
    expect(awarenessEventStatusText(stats)).toBeUndefined();
  });

  it('shows current bounded-drain pressure with queue and cursor context', () => {
    expect(awarenessEventStatusText({
      ...stats,
      backlogDepth: 2,
      backlogCapped: true,
      drainAccepted: 1,
      drainRefused: 1,
    })).toBe('peer events · 2+ queued · 1 refused · seq 7');
  });
});

afterEach(() => {
  for (const root of tempRoots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function peerEvent(sequence: number, overrides: Partial<OutboxEventV1> = {}): OutboxEventV1 {
  return {
    sequence,
    version: 1,
    eventId: `evt-${sequence}`,
    workspace,
    type: 'peer.message',
    actor: { kind: 'agent', id: 'peer-a' },
    provenance: { source: 'peer', trust: 'attributed-data' },
    createdAt: '2026-08-27T00:00:00.000Z',
    payload: { messageId: `msg-${sequence}`, fromAgentId: 'peer-a', toAgentId: 'pi:session-1', topic: 'EVIDENCE', text: `body-${sequence}`, files: [] },
    ...overrides,
  };
}

function fakeStore(events: OutboxEventV1[]) {
  let cursor = 0;
  const acknowledgements: Array<{ eventId: string; decision: InboundDecision }> = [];
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
    markMessageRead: () => undefined,
    close: vi.fn(),
  };
  return { store, acknowledgements, cursor: () => cursor };
}

describe('ordered Awareness event consumer', () => {
  it('delivers accepted peer data in sequence and never redelivers acknowledged events', async () => {
    const fixture = fakeStore([peerEvent(1), peerEvent(2)]);
    const delivered: string[] = [];
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'pi:session-1',
      expectedAgentId: 'pi:session-1',
      openStore: () => fixture.store,
      deliver: (message) => { delivered.push(message.content); },
      now: () => Date.parse('2026-08-27T00:01:00.000Z'),
    });

    const first = await consumer.drain();
    const second = await consumer.drain();

    expect(delivered).toEqual([
      '[peer:peer-a; class:informational; authority:data]\nbody-1',
      '[peer:peer-a; class:informational; authority:data]\nbody-2',
    ]);
    expect(fixture.acknowledgements).toEqual([
      { eventId: 'evt-1', decision: 'accept' },
      { eventId: 'evt-2', decision: 'accept' },
    ]);
    expect(first).toMatchObject({ lastAcknowledgedSequence: 2, accepted: 2, held: 0, refused: 0, errors: 0, backlogDepth: 0 });
    expect(second).toMatchObject({ accepted: 2, lastAcknowledgedSequence: 2, backlogDepth: 0 });
  });

  it('uses the Awareness recipient identity independently from the durable cursor identity', async () => {
    const awarenessAgentId = 'octo:lead-session-1';
    const toAwarenessAgent = peerEvent(1, {
      payload: { messageId: 'm1', fromAgentId: 'peer-a', toAgentId: awarenessAgentId, topic: 'EVIDENCE', text: 'for-awareness-agent' },
    });
    const toCursorOnly = peerEvent(2, {
      payload: { messageId: 'm2', fromAgentId: 'peer-a', toAgentId: 'pi:session-1', topic: 'EVIDENCE', text: 'for-cursor-id' },
    });
    const fixture = fakeStore([toAwarenessAgent, toCursorOnly]);
    const deliver = vi.fn();
    const stats = await createAwarenessEventConsumer({
      workspace,
      consumerId: 'pi:session-1',
      expectedAgentId: awarenessAgentId,
      openStore: () => fixture.store,
      deliver,
    }).drain();

    expect(deliver).toHaveBeenCalledTimes(1);
    expect(deliver.mock.calls[0]?.[0].content).toContain('for-awareness-agent');
    expect(JSON.stringify(deliver.mock.calls)).not.toContain('for-cursor-id');
    expect(fixture.acknowledgements).toEqual([
      { eventId: 'evt-1', decision: 'accept' },
      { eventId: 'evt-2', decision: 'refuse' },
    ]);
    expect(stats).toMatchObject({ consumerId: 'pi:session-1', accepted: 1, refused: 1 });
  });

  it('holds proposals, refuses wrong-target/expired/malformed events, and never exposes their bodies', async () => {
    const internal = peerEvent(1, {
      type: 'plan.projected',
      actor: { kind: 'system', id: 'awareness-plan-projector' },
      provenance: { source: 'harness', trust: 'authority' },
      payload: { secret: 'internal-body' },
    });
    const proposal = peerEvent(2, { payload: { messageId: 'm2', fromAgentId: 'peer-a', toAgentId: 'pi:session-1', topic: 'DECISION', text: 'proposal-body' } });
    const wrongTarget = peerEvent(3, { payload: { messageId: 'm3', fromAgentId: 'peer-a', toAgentId: 'someone-else', text: 'wrong-body' } });
    const expired = peerEvent(4, { expiresAt: '2026-08-26T23:59:00.000Z', payload: { messageId: 'm4', fromAgentId: 'peer-a', toAgentId: 'pi:session-1', text: 'expired-body' } });
    const malformed = peerEvent(5, { provenance: { source: 'peer', trust: 'authority' } });
    const fixture = fakeStore([internal, proposal, wrongTarget, expired, malformed]);
    const deliver = vi.fn();
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'pi:session-1',
      expectedAgentId: 'pi:session-1',
      openStore: () => fixture.store,
      deliver,
      now: () => Date.parse('2026-08-27T00:01:00.000Z'),
    });

    const stats = await consumer.drain();

    expect(deliver).not.toHaveBeenCalled();
    expect(fixture.acknowledgements).toEqual([
      { eventId: 'evt-1', decision: 'refuse' },
      { eventId: 'evt-2', decision: 'hold' },
      { eventId: 'evt-3', decision: 'refuse' },
      { eventId: 'evt-4', decision: 'refuse' },
      { eventId: 'evt-5', decision: 'refuse' },
    ]);
    expect(stats).toMatchObject({ accepted: 0, held: 1, refused: 4, errors: 1, lastAcknowledgedSequence: 5 });
  });

  it('replays after a delivery crash before ack and advances only after a successful replay', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    const deliver = vi.fn()
      .mockImplementationOnce(() => { throw new Error('host persistence failed'); })
      .mockImplementationOnce(() => undefined);
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'pi:session-1',
      expectedAgentId: 'pi:session-1',
      openStore: () => fixture.store,
      deliver,
      now: () => Date.parse('2026-08-27T00:01:00.000Z'),
    });

    expect(await consumer.drain()).toMatchObject({ errors: 1, lastAcknowledgedSequence: 0, backlogDepth: 1 });
    expect(await consumer.drain()).toMatchObject({ accepted: 1, lastAcknowledgedSequence: 1, backlogDepth: 0 });
    expect(deliver).toHaveBeenCalledTimes(2);
    expect(fixture.acknowledgements).toHaveLength(1);
  });

  it('serializes concurrent wake points so one event is delivered and acknowledged once', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    let releaseDelivery!: () => void;
    const deliveryGate = new Promise<void>((resolve) => { releaseDelivery = resolve; });
    const deliver = vi.fn(async () => { await deliveryGate; });
    const consumer = createAwarenessEventConsumer({
      workspace,
      consumerId: 'pi:session-1',
      expectedAgentId: 'pi:session-1',
      openStore: () => fixture.store,
      deliver,
      now: () => Date.parse('2026-08-27T00:01:00.000Z'),
    });

    const first = consumer.drain();
    const second = consumer.drain();
    releaseDelivery();
    const [firstStats, secondStats] = await Promise.all([first, second]);

    expect(firstStats).toEqual(secondStats);
    expect(deliver).toHaveBeenCalledTimes(1);
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
  });

  it('persists the session cursor in the real Awareness store across consumer recreation', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-event-consumer-'));
    tempRoots.push(root);
    const realWorkspace = path.join(root, 'workspace');
    const dbPath = path.join(root, 'agent.sqlite3');
    fs.mkdirSync(realWorkspace);
    const seed = openAwarenessStore({ workspace: realWorkspace, dbPath });
    seed.sendMessage({ fromAgentId: 'peer-a', toAgentId: 'pi:session-1', topic: 'EVIDENCE', text: 'durable body' });
    seed.close();
    const openStore = () => openAwarenessStore({ workspace: realWorkspace, dbPath });
    const firstDelivery = vi.fn();

    await createAwarenessEventConsumer({
      workspace: realWorkspace,
      consumerId: 'pi:session-1',
      expectedAgentId: 'pi:session-1',
      openStore,
      deliver: firstDelivery,
    }).drain();
    const secondDelivery = vi.fn();
    const restarted = await createAwarenessEventConsumer({
      workspace: realWorkspace,
      consumerId: 'pi:session-1',
      expectedAgentId: 'pi:session-1',
      openStore,
      deliver: secondDelivery,
    }).drain();

    expect(firstDelivery).toHaveBeenCalledTimes(1);
    expect(secondDelivery).not.toHaveBeenCalled();
    expect(restarted).toMatchObject({ backlogDepth: 0, lastAcknowledgedSequence: 1 });
  });

  it('derives a restart-stable cursor from a normalized session file and never from the process id', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-event-identity-'));
    tempRoots.push(root);
    const sessionFile = path.join(root, 'sessions', 'one.jsonl');
    const first = resolvePiEventConsumerId({
      sessionManager: { getSessionFile: () => path.join(root, 'sessions', '..', 'sessions', 'one.jsonl') },
    } as PiContext);
    const afterRestart = resolvePiEventConsumerId({
      sessionManager: { getSessionFile: () => sessionFile },
    } as PiContext);

    expect(first).toBe(afterRestart);
    expect(first).toMatch(/^pi:file:[a-f0-9]{24}$/);
    expect(first).not.toContain(String(process.pid));
  });

  it('does not open or acknowledge the outbox when no restart-stable session identity exists', async () => {
    const handlers = new Map<string, Array<(event: unknown, ctx: PiContext) => Promise<void>>>();
    const openStore = vi.fn(() => { throw new Error('must not open'); });
    const observations: unknown[] = [];
    const pi = {
      on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
      },
      sendMessage: vi.fn(),
    } as unknown as PiInstance;
    const ctx = { cwd: workspace, sessionManager: {} } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore,
      resolveExpectedAgentId: () => 'octo:lead',
      onObservability: (stats) => { observations.push(stats); },
    });
    await handlers.get('session_start')?.[0]?.({}, ctx);

    expect(resolvePiEventConsumerId(ctx)).toBeUndefined();
    expect(openStore).not.toHaveBeenCalled();
    expect(pi.sendMessage).not.toHaveBeenCalled();
    expect(observations).toEqual([expect.objectContaining({ consumerId: 'unavailable', errors: 1, lastAcknowledgedSequence: 0 })]);
    expect(JSON.stringify(observations)).not.toContain('body');
  });

  it('does not acknowledge when Pi returns before custom-message persistence is observable', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    const handlers = new Map<string, Array<(event: unknown, ctx: PiContext) => Promise<void>>>();
    const observations: unknown[] = [];
    const pi = {
      on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
      },
      // Pi 0.84.x returns void. Simulate its asynchronous persistence failing after return.
      sendMessage: vi.fn(() => undefined),
    } as unknown as PiInstance;
    const ctx = {
      cwd: workspace,
      sessionManager: { getSessionId: () => 'session-1', getEntries: () => [] },
    } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore: () => fixture.store,
      resolveExpectedAgentId: () => 'pi:session-1',
      onObservability: (stats) => { observations.push(stats); },
    });
    await handlers.get('session_start')?.[0]?.({}, ctx);

    expect(pi.sendMessage).toHaveBeenCalledTimes(1);
    expect(fixture.acknowledgements).toEqual([]);
    expect(fixture.cursor()).toBe(0);
    expect(observations.at(-1)).toMatchObject({ backlogDepth: 1, lastAcknowledgedSequence: 0, errors: 1 });
  });

  it('reuses a persisted receipt after a crash before Awareness acknowledgment', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    let rejectAck = true;
    const store: AwarenessEventStore = {
      ...fixture.store,
      acknowledgeEvent: (params) => {
        if (rejectAck) {
          rejectAck = false;
          throw new Error('crash before ack commit');
        }
        return fixture.store.acknowledgeEvent(params);
      },
    };
    const handlers = new Map<string, Array<(event: unknown, ctx: PiContext) => Promise<void>>>();
    const persistedEntries: unknown[] = [];
    const sendMessage = vi.fn((message: unknown) => {
      persistedEntries.push({ type: 'custom_message', ...(message as object) });
    });
    const pi = {
      on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
      },
      sendMessage,
    } as unknown as PiInstance;
    const ctx = {
      cwd: workspace,
      sessionManager: { getSessionId: () => 'session-1', getEntries: () => persistedEntries },
    } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore: () => store,
      resolveExpectedAgentId: () => 'pi:session-1',
    });
    await handlers.get('session_start')?.[0]?.({}, ctx);
    await handlers.get('turn_end')?.[0]?.({}, ctx);

    expect(sendMessage).toHaveBeenCalledTimes(1);
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
    expect(fixture.cursor()).toBe(1);
  });

  it('registers bounded lifecycle wake points and emits body-free observability', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    const handlers = new Map<string, Array<(event: unknown, ctx: PiContext) => Promise<void>>>();
    const sent: unknown[] = [];
    const statuses: unknown[] = [];
    const persistedEntries: unknown[] = [];
    const pi = {
      on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
      },
      sendMessage: (message: unknown, options: unknown) => {
        sent.push({ message, options });
        persistedEntries.push({ type: 'custom_message', ...(message as object) });
      },
    } as unknown as PiInstance;
    const ctx = {
      cwd: workspace,
      sessionManager: { getSessionId: () => 'session-1', getEntries: () => persistedEntries },
    } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore: () => fixture.store,
      resolveExpectedAgentId: () => 'pi:session-1',
      onObservability: (stats) => { statuses.push(stats); },
      now: () => Date.parse('2026-08-27T00:01:00.000Z'),
    });
    await handlers.get('session_start')?.[0]?.({}, ctx);
    await handlers.get('turn_end')?.[0]?.({}, ctx);

    expect([...handlers.keys()].sort()).toEqual(['session_start', 'turn_end']);
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({
      message: { customType: 'octocode-peer-event', content: '[peer:peer-a; class:informational; authority:data]\nbody-1', display: false },
      options: { triggerTurn: false },
    });
    expect(JSON.stringify(statuses)).not.toContain('body-1');
    expect(statuses.at(-1)).toMatchObject({ backlogDepth: 0, lastAcknowledgedSequence: 1, accepted: 1, held: 0, refused: 0, errors: 0 });
  });
});
