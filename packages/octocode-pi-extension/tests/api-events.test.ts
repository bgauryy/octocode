import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiClient, isNestedToolEvent } from '../src/api/client.js';
import { nesting } from '../src/api/events.js';
import type { ApiEvent } from '../src/api/protocol.js';
import { boot, fakeCtx, settle, stopApis } from './api-harness.js';

afterEach(stopApis);

describe('external API: events and redaction', () => {
  it('redacts secrets from streamed deltas, message events and the message list', async () => {
    const secret = `ghp_${'a'.repeat(36)}`;
    const { api, emit, client } = await boot({}, { value: true }, [{ type: 'message', message: { role: 'assistant', content: `key ${secret}` } }]);
    const events: ApiEvent[] = [];
    api.log.subscribe((event) => events.push(event), { types: ['message.delta', 'message'] });
    emit('message_update', { assistantMessageEvent: { type: 'text_delta', delta: `token ${secret}` } });
    emit('message_end', { message: { role: 'assistant', content: `again ${secret}` } });
    expect(events.map((event) => event.data)).toEqual([{ delta: 'token [redacted]' }, { role: 'assistant', text: 'again [redacted]' }]);
    const listed = JSON.stringify(await client.call('messages.list', { limit: 10 }));
    expect(listed).toContain('key [redacted]');
    expect(listed).not.toContain(secret);
  });

  it('advertises and delivers agent.settled by default', async () => {
    const { client, api, emit } = await boot();
    const { capabilities } = await client.call<{ capabilities: { events: string[] } }>('initialize');
    expect(capabilities.events).toContain('agent.settled');
    const events: ApiEvent[] = [];
    api.log.subscribe((event) => events.push(event), {});
    emit('agent_settled', {});
    expect(events.map((event) => event.type)).toEqual(['agent.settled']);
  });

  it('redacts the final answer that agent.end, agent.settled and wait carry, secrets that straddle the size limit included', async () => {
    const secret = `ghp_${'a'.repeat(36)}`;
    const { client, emit, sent, api } = await boot();
    const publish = vi.spyOn(api.log, 'publish');
    const reply = client.call('message.send', { text: 'summarise', wait: true });
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    // The token starts 10 characters before the 64,000-character limit: cutting first would leave its prefix.
    const answer = `${'x'.repeat(63_989)} ${secret} end`;
    emit('agent_end', { messages: [{ role: 'assistant', content: [{ type: 'text', text: `key ${secret}` }], stopReason: 'stop' }] });
    emit('agent_settled', {});
    expect(await reply).toMatchObject({ text: 'key [redacted]' });
    expect(publish).toHaveBeenCalledWith('agent.end', { text: 'key [redacted]', stopReason: 'stop' });
    expect(publish).toHaveBeenCalledWith('agent.settled', { text: 'key [redacted]', stopReason: 'stop' });
    emit('agent_end', { messages: [{ role: 'assistant', content: answer, stopReason: 'stop' }] });
    const long = publish.mock.calls.filter(([type]) => type === 'agent.end').at(-1)![1];
    expect(long['text']).not.toContain('ghp_');
  });

  it('streams events with sequence numbers and replays after a reconnect', async () => {
    const { client, emit } = await boot();
    const seen: string[] = [];
    const controller = new AbortController();
    let subscribed!: () => void;
    const live = new Promise<void>((resolve) => (subscribed = resolve));
    const done = client.watch((event) => seen.push(`${event.seq}:${event.type}`), { signal: controller.signal, onSubscribed: () => subscribed() });
    await live;
    emit('agent_start', {});
    emit('tool_execution_start', { toolCallId: 't1', toolName: 'bash', args: { command: 'ls' } });
    emit('message_update', { assistantMessageEvent: { type: 'text_delta', delta: 'hi' } });
    emit('message_end', { message: { role: 'assistant', content: 'hi' } });
    emit('tool_execution_end', { toolCallId: 't1', toolName: 'bash', isError: false });
    await vi.waitFor(() => expect(seen).toHaveLength(4));
    controller.abort();
    await done;
    expect(seen).toEqual(['1:agent.start', '2:tool.start', '4:message', '5:tool.end']);

    const replay: string[] = [];
    const again = new AbortController();
    const second = client.watch((event) => replay.push(event.type), { since: 2, types: ['tool.end', 'message.delta'], signal: again.signal });
    await vi.waitFor(() => expect(replay).toHaveLength(2));
    again.abort();
    await second;
    expect(replay).toEqual(['message.delta', 'tool.end']);
  });
});

describe('external API: nested tool calls', () => {
  it('derives depth and parent from a tool call id', () => {
    expect(nesting(undefined)).toEqual({ depth: 0 });
    expect(nesting('toolu_1')).toEqual({ parentId: 'toolu_1', depth: 1 });
    expect(nesting('toolu_1/1')).toEqual({ parentId: 'toolu_1/1', depth: 2 });
  });

  it('counts only tool events as nested', () => {
    expect(isNestedToolEvent({ seq: 1, at: 0, type: 'message', data: { parentId: 'x' } })).toBe(false);
  });

  it('marks nested tool calls with parentId and depth, times tool.end, and filters them for topLevel watchers', async () => {
    const { client, emit } = await boot();
    const all: ApiEvent[] = [];
    const top: ApiEvent[] = [];
    const controller = new AbortController();
    let ready = 0;
    let subscribed!: () => void;
    const live = new Promise<void>((resolve) => (subscribed = resolve));
    const onSubscribed = () => void (++ready === 2 && subscribed());
    const watching = [
      client.watch((event) => all.push(event), { signal: controller.signal, onSubscribed }),
      client.watch((event) => top.push(event), { topLevel: true, signal: controller.signal, onSubscribed }),
    ];
    await live;
    emit('tool_execution_start', { toolCallId: 'toolu_1', toolName: 'codemode', args: {} });
    emit('tool_execution_start', { toolCallId: 'toolu_1/1', parentToolCallId: 'toolu_1', toolName: 'read', args: { path: 'a.ts' } });
    emit('tool_execution_start', { toolCallId: 'toolu_1/1/2', parentToolCallId: 'toolu_1/1', toolName: 'bash', args: {} });
    emit('tool_execution_end', { toolCallId: 'toolu_1/1/2', parentToolCallId: 'toolu_1/1', toolName: 'bash', isError: true, durationMs: 30 });
    emit('tool_execution_end', { toolCallId: 'toolu_1/1', parentToolCallId: 'toolu_1', toolName: 'read', isError: false, durationMs: 50 });
    emit('tool_execution_end', { toolCallId: 'toolu_1', toolName: 'codemode', isError: false, durationMs: 100 });
    // Pi owns execution timing even when the bridge did not observe the start.
    emit('tool_execution_end', { toolCallId: 'toolu_9', toolName: 'bash', isError: false, durationMs: 0 });
    await vi.waitFor(() => expect(all).toHaveLength(7));
    await vi.waitFor(() => expect(top).toHaveLength(3));
    controller.abort();
    await Promise.all(watching);

    expect(all.map((event) => event.data)).toEqual([
      { id: 'toolu_1', name: 'codemode', hint: expect.any(String), depth: 0 },
      { id: 'toolu_1/1', name: 'read', hint: expect.any(String), parentId: 'toolu_1', depth: 1 },
      { id: 'toolu_1/1/2', name: 'bash', hint: expect.any(String), parentId: 'toolu_1/1', depth: 2 },
      { id: 'toolu_1/1/2', name: 'bash', isError: true, parentId: 'toolu_1/1', depth: 2, durationMs: 30 },
      { id: 'toolu_1/1', name: 'read', isError: false, parentId: 'toolu_1', depth: 1, durationMs: 50 },
      { id: 'toolu_1', name: 'codemode', isError: false, depth: 0, durationMs: 100 },
      { id: 'toolu_9', name: 'bash', isError: false, depth: 0, durationMs: 0 },
    ]);
    expect(top.map((event) => `${event.type}:${String(event.data['id'])}`)).toEqual(['tool.start:toolu_1', 'tool.end:toolu_1', 'tool.end:toolu_9']);
  });

  it('keeps native timing on a tool that ends in a new session, without the previous session\'s clock state', async () => {
    const { emit, api } = await boot();
    emit('tool_execution_start', { toolCallId: 'toolu_2', toolName: 'bash', args: {} });
    await api.start(fakeCtx({ value: true }), { socket: true });
    const after = new ApiClient(api.instance!);
    const seen: ApiEvent[] = [];
    const again = new AbortController();
    let resubscribed!: () => void;
    const relive = new Promise<void>((resolve) => (resubscribed = resolve));
    const watch = after.watch((event) => seen.push(event), { signal: again.signal, onSubscribed: () => resubscribed() });
    await relive;
    emit('tool_execution_end', { toolCallId: 'toolu_2', toolName: 'bash', isError: false, durationMs: 12 });
    await vi.waitFor(() => expect(seen).toHaveLength(1));
    again.abort();
    await watch;
    expect(seen[0].data).toEqual({ id: 'toolu_2', name: 'bash', isError: false, depth: 0, durationMs: 12 });
  });
});

describe('external API: wait', () => {
  it('waits for the agent to finish and returns its answer', async () => {
    const { client, emit, sent } = await boot();
    const reply = client.call('message.send', { text: 'summarise', wait: true });
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    emit('agent_end', { messages: [{ role: 'assistant', content: [{ type: 'text', text: 'done: 3 files' }], stopReason: 'stop' }] });
    // `wait` answers only once Pi has settled (it may continue after agent_end).
    emit('agent_settled', {});
    expect(await reply).toMatchObject({ text: 'done: 3 files', stopReason: 'stop', queued: 'turn' });
  });

  it('reports an aborted run even when its last assistant answer stopped normally', async () => {
    const { client, emit, sent, api } = await boot();
    const publish = vi.spyOn(api.log, 'publish');
    const reply = client.call('message.send', { text: 'continue the investigation', wait: true });
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    emit('agent_end', { messages: [{ role: 'assistant', content: [{ type: 'text', text: 'partial findings' }], stopReason: 'stop' }] });
    emit('agent_settled', { aborted: true });
    expect(await reply).toMatchObject({ text: 'partial findings', stopReason: 'aborted', queued: 'turn' });
    expect(publish).toHaveBeenCalledWith('agent.settled', { text: 'partial findings', stopReason: 'aborted' });
  });

  it('answers a wait only once Pi has settled, never with an earlier run\'s text', async () => {
    const { client, emit, sent } = await boot();
    let answered = false;
    const reply = client.call('message.send', { text: 'go', wait: true }).then((value) => ((answered = true), value));
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    emit('agent_end', { messages: [{ role: 'assistant', content: [{ type: 'text', text: 'intermediate' }], stopReason: 'stop' }] });
    await settle(client);
    // Pi may still continue (a queued follow-up): agent_end alone does not answer.
    expect(answered).toBe(false);
    emit('agent_start', {});
    emit('agent_settled', {});
    // The follow-up run produced no agent_end: the answer is empty, not the stale text.
    expect(await reply).toMatchObject({ text: '' });
  });

  it('times out a wait without losing the message', async () => {
    const { client, sent } = await boot();
    await expect(client.call('message.send', { text: 'x', wait: true, timeoutMs: 1_000 })).rejects.toMatchObject({ code: -32001 });
    expect(sent).toHaveLength(1);
  });
});
