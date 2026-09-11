import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import type { AwarenessEventStore, AwarenessEventHintOptions, OutboxEventV1 } from '@octocodeai/octocode-awareness/host';
import { registerAwarenessEventConsumer } from '../src/tools/awareness-event-consumer.js';
import type { PiContext, PiInstance } from '../src/types.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

it.each([false, true])('drains idle hints through receipts (delayed=%s) with one authorized automatic turn', async (delayed) => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'pi-idle-peer-')));
  roots.push(root);
  const sessionFile = join(root, 'session.jsonl');
  writeFileSync(sessionFile, '');
  const handlers = new Map<string, (event: unknown, ctx: PiContext) => Promise<void>>();
  const entries: object[] = [];
  const events: OutboxEventV1[] = [];
  let cursor = 0;
  const reads = vi.fn();
  const store: AwarenessEventStore = {
    dbPath: join(root, 'selected.sqlite3'),
    listEvents: ({ limit }) => events.filter(event => event.sequence > cursor).slice(0, limit),
    acknowledgeEvent: ({ eventId, decision }) => {
      cursor = events.find(event => event.eventId === eventId)!.sequence;
      return { sequence: cursor, decision, duplicate: false };
    },
    getConsumerCursor: () => cursor, markMessageRead: reads, close() {},
  };
  const sendMessage = vi.fn((message: object) => {
    const persist = () => entries.push({ type: 'custom_message', ...message });
    if (delayed) setImmediate(persist);
    else persist();
  });
  const pi = { on: (name: string, handler: (event: unknown, ctx: PiContext) => Promise<void>) => handlers.set(name, handler), sendMessage } as unknown as PiInstance;
  const ctx = { cwd: root, isProjectTrusted: () => true, sessionManager: {
    getSessionId: () => 'session', getSessionFile: () => sessionFile, getEntries: () => entries,
  } } as PiContext;
  let hint!: () => void;
  const closeWatch = vi.fn();
  const watchEvents = vi.fn((options: AwarenessEventHintOptions) => { hint = options.onHint; return { close: closeWatch }; });
  registerAwarenessEventConsumer(pi, { openStore: () => store, watchEvents, resolveExpectedAgentId: () => 'recipient', maxEventsPerDrain: 1 });
  const add = (sequence: number) => events.push({ version: 1, sequence, eventId: `e${sequence}`, workspace: root,
    type: 'peer.message', actor: { kind: 'agent', id: 'sender' }, provenance: { source: 'peer', trust: 'attributed-data' },
    aggregate: { kind: 'message', id: `m${sequence}` }, createdAt: new Date().toISOString(),
    payload: { messageId: `m${sequence}`, fromAgentId: 'sender', toAgentId: 'recipient', topic: 'BLOCKED', text: 'peer needs a decision' } });
  try {
    await handlers.get('session_start')!({}, ctx);
    expect(watchEvents).toHaveBeenCalledOnce();
    expect(watchEvents.mock.calls[0]![0].database).toBe(store.dbPath);
    add(1); add(2);
    hint(); hint(); hint();
    await vi.waitFor(() => expect(cursor).toBe(2));
    expect(reads).toHaveBeenCalledTimes(2);
    expect(sendMessage.mock.calls.filter(([message]) => (message as {customType: string}).customType === 'octocode-peer-wake')).toHaveLength(1);
    add(3); hint();
    await vi.waitFor(() => expect(cursor).toBe(3));
    expect(sendMessage.mock.calls.filter(([message]) => (message as {customType: string}).customType === 'octocode-peer-wake')).toHaveLength(1);
    await handlers.get('session_shutdown')!({}, ctx);
    expect(closeWatch).toHaveBeenCalledOnce();
    add(4); hint();
    await new Promise(resolve => setTimeout(resolve, 100));
    expect(cursor).toBe(3);
  } finally { await handlers.get('session_shutdown')!({}, ctx); }
});
