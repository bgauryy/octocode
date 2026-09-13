import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { openAwarenessStore, type InboundDecision, type OutboxEventV1 } from '@octocodeai/octocode-awareness/host';
import { awarenessEventStatusText, registerAwarenessEventConsumer } from '../src/tools/awareness-event-consumer.js';
import { resolveAwarenessSessionAgentId } from '../src/tools/awareness-shared.js';
import { createAwarenessEventConsumer, type AwarenessEventStore } from '@octocodeai/octocode-awareness/host';
import type { PiContext, PiInstance } from '../src/types.js';

const workspace = '/work/repo';
const tempRoots: string[] = [];

function persistedSession(): { getSessionFile(): string } {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-peer-session-'));
  tempRoots.push(root);
  const file = path.join(root, 'session.jsonl');
  fs.writeFileSync(file, '');
  return { getSessionFile: () => file };
}

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
    markMessageRead: vi.fn(),
    close: vi.fn(),
  };
  return { store, acknowledgements, cursor: () => cursor };
}

describe('ordered Awareness event consumer', () => {
  it('does not subscribe the same Pi host twice', () => {
    const fixture = fakeStore([]);
    const handlers = new Map<string, unknown>();
    const pi = {
      on: vi.fn((event: string, handler: unknown) => { handlers.set(event, handler); }),
    } as unknown as PiInstance;
    registerAwarenessEventConsumer(pi, { openStore: () => fixture.store, resolveExpectedAgentId: () => 'pi:session-1' });
    registerAwarenessEventConsumer(pi, { openStore: () => fixture.store, resolveExpectedAgentId: () => 'pi:session-1' });
    expect(pi.on).toHaveBeenCalledTimes(5);
    expect(handlers.size).toBe(5);
  });

  it.each(['informational', 'untrusted', 'host-policy', 'retry'] as const)('does not start an automatic turn for %s', async reason => {
    const events: OutboxEventV1[] = [];
    const fixture = fakeStore(events);
    const entries: unknown[] = [];
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const sendMessage = vi.fn((message: unknown, _options?: unknown) => { entries.push({ type: 'custom_message', ...(message as object) }); });
    const pi = { on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(event, handler), sendMessage } as unknown as PiInstance;
    const ctx = { cwd: workspace, isProjectTrusted: () => reason !== 'untrusted', sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => entries } } as PiContext;
    registerAwarenessEventConsumer(pi, { openStore: () => fixture.store, resolveExpectedAgentId: () => 'pi:session-1', ...(reason === 'host-policy' ? { canWake: () => false } : {}) });
    await handlers.get('session_start')?.({}, ctx);
    events.push(reason === 'informational' ? peerEvent(1) : peerEvent(1, { payload: { messageId: 'msg-1', fromAgentId: 'peer-a', toAgentId: 'pi:session-1', signalKind: 'blocker', text: 'Inspect blocker', files: [] } }));
    await handlers.get('agent_end')?.({ willRetry: reason === 'retry' }, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(sendMessage.mock.calls.filter(call => (call[1] as { triggerTurn?: boolean })?.triggerTurn)).toHaveLength(0);
    expect(fixture.acknowledgements).toHaveLength(reason === 'retry' ? 0 : 1);
  });
  it('coalesces directed actionable delivery into one bounded wake and ignores informational noise', async () => {
    const actionable = (sequence: number, directed = true) => peerEvent(sequence, {
      payload: { messageId: `msg-${sequence}`, fromAgentId: 'peer-a', toAgentId: directed ? 'pi:session-1' : null, signalKind: 'request', text: 'Please inspect the request', files: [] },
    });
    const events = [actionable(1), actionable(2), peerEvent(3), actionable(4, false)];
    const fixture = fakeStore(events);
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const entries: unknown[] = [];
    const sendMessage = vi.fn((message: unknown) => { entries.push({ type: 'custom_message', ...(message as object) }); });
    const pi = { on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(event, handler), sendMessage } as unknown as PiInstance;
    const ctx = { cwd: workspace, isProjectTrusted: () => true, sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => entries } } as PiContext;
    registerAwarenessEventConsumer(pi, { openStore: () => fixture.store, resolveExpectedAgentId: () => 'pi:session-1' });
    await handlers.get('session_start')?.({}, ctx);
    const wakes = () => sendMessage.mock.calls.filter(call => (call as unknown[])[1] && ((call as unknown[])[1] as { triggerTurn?: boolean }).triggerTurn);
    expect(wakes()).toHaveLength(1);
    expect(wakes()[0]?.[0]).toMatchObject({ customType: 'octocode-peer-wake', content: expect.stringContaining('2 actionable') });
    events.push(actionable(5));
    await handlers.get('agent_end')?.({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(wakes()).toHaveLength(1);
    await handlers.get('input')?.({ source: 'extension' }, ctx);
    events.push(actionable(6));
    await handlers.get('agent_end')?.({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(wakes()).toHaveLength(1);
    await handlers.get('input')?.({ source: 'rpc' }, ctx);
    events.push(actionable(7));
    await handlers.get('agent_end')?.({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(wakes()).toHaveLength(2);
  });

  it('reports an automatic wake failure without claiming persisted peer delivery failed', async () => {
    const fixture = fakeStore([peerEvent(1, {
      payload: {
        messageId: 'msg-1', fromAgentId: 'peer-a', toAgentId: 'pi:session-1',
        signalKind: 'request', text: 'Inspect this request', files: [],
      },
    })]);
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const entries: object[] = [];
    const notify = vi.fn();
    const onObservability = vi.fn();
    const sendMessage = vi.fn((message: object, options?: { triggerTurn?: boolean }) => {
      if (options?.triggerTurn) throw new Error('wake unavailable');
      entries.push({ type: 'custom_message', ...message });
    });
    const pi = {
      on: (event: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(event, handler),
      sendMessage,
    } as unknown as PiInstance;
    const ctx = {
      hasUI: true,
      ui: { notify },
      cwd: workspace,
      isProjectTrusted: () => true,
      sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => entries },
    } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore: () => fixture.store,
      resolveExpectedAgentId: () => 'pi:session-1',
      onObservability,
    });
    await handlers.get('session_start')?.({}, ctx);

    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
    expect(notify).toHaveBeenCalledWith(
      'Awareness: peer messages were delivered, but automatic wake-up is unavailable. Continue on the next authorized input.',
      'warning',
    );
    expect(JSON.stringify(notify.mock.calls)).not.toContain('Peer message delivery is unavailable');
    expect(onObservability.mock.calls.at(-1)?.[0]).toMatchObject({
      errors: 1,
      drainErrors: 0,
      lastAcknowledgedSequence: 1,
    });
  });
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
    const proposal = peerEvent(2, { payload: { messageId: 'm2', fromAgentId: 'peer-a', toAgentId: 'pi:session-1', signalKind: 'approval', topic: 'APPROVAL', text: 'proposal-body' } });
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
    const first = resolveAwarenessSessionAgentId({
      sessionManager: { getSessionFile: () => path.join(root, 'sessions', '..', 'sessions', 'one.jsonl') },
    } as PiContext);
    const afterRestart = resolveAwarenessSessionAgentId({
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

    expect(resolveAwarenessSessionAgentId(ctx)).toBeUndefined();
    expect(openStore).not.toHaveBeenCalled();
    expect(pi.sendMessage).not.toHaveBeenCalled();
    // No observation emitted: the silent return is the same policy as the !sessionFile guard below it.
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
      sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => [] },
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
      sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => persistedEntries },
    } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore: () => store,
      resolveExpectedAgentId: () => 'pi:session-1',
    });
    await handlers.get('session_start')?.[0]?.({}, ctx);
    await handlers.get('agent_end')?.[0]?.({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));

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
      sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => persistedEntries },
    } as PiContext;

    registerAwarenessEventConsumer(pi, {
      openStore: () => fixture.store,
      resolveExpectedAgentId: () => 'pi:session-1',
      onObservability: (stats) => { statuses.push(stats); },
      now: () => Date.parse('2026-08-27T00:01:00.000Z'),
    });
    await handlers.get('session_start')?.[0]?.({}, ctx);
    await handlers.get('agent_end')?.[0]?.({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));

    expect([...handlers.keys()].sort()).toEqual(['agent_end', 'agent_start', 'input', 'session_shutdown', 'session_start']);
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({
      message: { customType: 'octocode-peer-event', content: '[peer:peer-a; class:informational; authority:data]\nbody-1', display: true },
      options: { triggerTurn: false },
    });
    expect(JSON.stringify(statuses)).not.toContain('body-1');
    expect(statuses.at(-1)).toMatchObject({ backlogDepth: 0, lastAcknowledgedSequence: 1, accepted: 1, held: 0, refused: 0, errors: 0 });
  });

  it('alerts for a persisted blocking peer event once, even when acknowledgment retries', async () => {
    const fixture = fakeStore([peerEvent(1, {
      payload: { messageId: 'msg-1', fromAgentId: 'peer-a', toAgentId: 'pi:session-1', signalKind: 'blocker', topic: 'BLOCKED', text: 'private-body' },
    })]);
    let firstAck = true;
    const store = { ...fixture.store, acknowledgeEvent: (params: Parameters<AwarenessEventStore['acknowledgeEvent']>[0]) => {
      if (firstAck) { firstAck = false; throw new Error('retry ack'); }
      return fixture.store.acknowledgeEvent(params);
    } };
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const entries: object[] = [];
    const notify = vi.fn();
    const pi = {
      on: (name: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(name, handler),
      sendMessage: vi.fn((message: object) => entries.push({ type: 'custom_message', ...message })),
    } as unknown as PiInstance;
    const ctx = { hasUI: true, cwd: workspace, ui: { notify }, sessionManager: { ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => entries } } as PiContext;
    registerAwarenessEventConsumer(pi, { openStore: () => store, resolveExpectedAgentId: () => 'pi:session-1' });
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_end')!({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(pi.sendMessage).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls.filter(([text]) => text.includes('blocking message'))).toEqual([
      ['Awareness: blocking message received. See the peer card for details.', 'warning'],
    ]);
    expect(JSON.stringify(notify.mock.calls)).not.toContain('private-body');
  });

  it('keeps startup and ephemeral messages unread until Pi creates its session file', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-peer-startup-'));
    tempRoots.push(root);
    const file = path.join(root, 'session.jsonl');
    let sessionFile: string | undefined;
    const fixture = fakeStore([peerEvent(1)]);
    const openStore = vi.fn(() => fixture.store);
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const entries: object[] = [];
    const pi = {
      on: (name: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(name, handler),
      sendMessage: vi.fn((message: object) => entries.push({ type: 'custom_message', ...message })),
    } as unknown as PiInstance;
    const ctx = { cwd: workspace, sessionManager: {
      getSessionId: () => 'session-1', getSessionFile: () => sessionFile, getEntries: () => entries,
    } } as PiContext;
    registerAwarenessEventConsumer(pi, { openStore, resolveExpectedAgentId: () => 'pi:session-1' });
    await handlers.get('session_start')!({}, ctx);
    sessionFile = file;
    await handlers.get('agent_end')!({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(openStore).not.toHaveBeenCalled();
    expect(pi.sendMessage).not.toHaveBeenCalled();
    expect(fixture.store.markMessageRead).not.toHaveBeenCalled();
    expect(fixture.acknowledgements).toEqual([]);
    fs.writeFileSync(file, '');
    await handlers.get('agent_end')!({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(pi.sendMessage).toHaveBeenCalledTimes(1);
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
  });

  it('stops an in-flight drain at shutdown and leaves undelivered messages available for replay', async () => {
    const fixture = fakeStore([peerEvent(1), peerEvent(2)]);
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const entries: object[] = [];
    let stopped = false;
    const ctx = { cwd: workspace, sessionManager: {
      ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => entries,
    } } as PiContext;
    const pi = {
      on: (name: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(name, handler),
      sendMessage: vi.fn((message: object) => {
        entries.push({ type: 'custom_message', ...message });
        if (!stopped) {
          stopped = true;
          void handlers.get('session_shutdown')!({}, ctx);
        }
      }),
    } as unknown as PiInstance;
    registerAwarenessEventConsumer(pi, { openStore: () => fixture.store, resolveExpectedAgentId: () => 'pi:session-1' });

    await handlers.get('session_start')!({}, ctx);
    expect(pi.sendMessage).toHaveBeenCalledTimes(1);
    expect(fixture.acknowledgements.some(ack => ack.eventId === 'evt-2')).toBe(false);
    await handlers.get('agent_end')!({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(pi.sendMessage).toHaveBeenCalledTimes(1);

    await handlers.get('session_start')!({}, ctx);
    expect(pi.sendMessage).toHaveBeenCalledTimes(2);
    expect(fixture.acknowledgements).toEqual([
      { eventId: 'evt-1', decision: 'accept' },
      { eventId: 'evt-2', decision: 'accept' },
    ]);
  });

  it('waits for the completion hook to return and cancels scheduled delivery at shutdown', async () => {
    const fixture = fakeStore([peerEvent(1)]);
    const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
    const entries: object[] = [];
    let streaming = true;
    const pi = {
      on: (name: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(name, handler),
      sendMessage: vi.fn((message: object) => {
        // Pi queues steer messages while a completion hook is running.
        if (!streaming) entries.push({ type: 'custom_message', ...message });
      }),
    } as unknown as PiInstance;
    const ctx = { cwd: workspace, sessionManager: {
      ...persistedSession(), getSessionId: () => 'session-1', getEntries: () => entries,
    } } as PiContext;
    registerAwarenessEventConsumer(pi, { openStore: () => fixture.store, resolveExpectedAgentId: () => 'pi:session-1' });
    await handlers.get('agent_end')!({}, ctx);
    expect(pi.sendMessage).not.toHaveBeenCalled();
    streaming = false;
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(fixture.acknowledgements).toEqual([{ eventId: 'evt-1', decision: 'accept' }]);
    await handlers.get('agent_end')!({}, ctx);
    await handlers.get('session_shutdown')!({}, ctx);
    await new Promise<void>(resolve => setImmediate(resolve));
    expect(pi.sendMessage).toHaveBeenCalledTimes(1);
  });
});
