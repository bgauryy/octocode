import fs from 'node:fs';
import http from 'node:http';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { nesting, OctocodeApi, optionsFromEnv } from '../src/api/bridge.js';
import { ApiCallError, ApiClient, isNestedToolEvent } from '../src/api/client.js';
import { EventLog } from '../src/api/events.js';
import { parseRequest, type ApiEvent } from '../src/api/protocol.js';
import { listInstances } from '../src/api/registry.js';
import { fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

const fakeCtx = (idle: { value: boolean }) =>
  ({
    cwd: '/work',
    model: { provider: 'anthropic', id: 'sonnet' },
    isIdle: () => idle.value,
    hasPendingMessages: () => false,
    abort: () => void (idle.value = true),
    getContextUsage: () => ({ tokens: 10 }),
    sessionManager: {
      getSessionId: () => 's1',
      getSessionName: () => undefined,
      getBranch: () => [
        { type: 'message', message: { role: 'user', content: 'hi' } },
        { type: 'message', message: { role: 'assistant', content: [{ type: 'text', text: 'hello' }] } },
      ],
    },
  }) as never;

const running: OctocodeApi[] = [];
async function boot(options: { http?: number } = {}, idle = { value: true }) {
  const env = { OCTOCODE_API_DIR: tmp('octo-api-') } as NodeJS.ProcessEnv;
  const { pi, sent, emit: fire } = fakePi();
  const emit = (name: string, event: unknown) => void fire(name, event, undefined);
  const told: Array<[string, string]> = [];
  const api = new OctocodeApi(pi, { list: () => [{ id: 'main-1' }], tell: (to, text) => (told.push([to, text]), to === 'main-1' ? { sent: [to] } : { error: `No agent "${to}".` }), maxChars: 100 }, () => '1.0.0', env);
  running.push(api);
  api.observe();
  await api.start(fakeCtx(idle), { socket: true, ...options });
  return { api, sent, emit, told, client: new ApiClient(api.instance!), env };
}
/** One round trip through the server: everything it wrote before answering has been handled. */
const settle = (client: ApiClient) => client.call('ping');

/** Reads GET /events as raw text until closed; `ready` resolves once the stream's headers arrived. */
function sse(url: string, token: string, headers: Record<string, string> = {}) {
  let text = '';
  let req!: http.ClientRequest;
  const ready = new Promise<number>((resolve, reject) => {
    req = http.get(`${url}/events${headers['query'] ?? ''}`, { headers: { authorization: `Bearer ${token}`, ...(headers['last-event-id'] ? { 'last-event-id': headers['last-event-id'] } : {}) } }, (res) => {
      res.setEncoding('utf8');
      res.on('data', (chunk: string) => (text += chunk));
      resolve(res.statusCode!);
    });
    req.on('error', reject);
  });
  const ids = () => [...text.matchAll(/^id: (\d+)$/gmu)].map((match) => Number(match[1]));
  return { ready, ids, text: () => text, close: () => req.destroy() };
}

afterEach(async () => {
  await Promise.all(running.splice(0).map((api) => api.stop()));
});

describe('external API', () => {
  it('removes a watch abort listener when the server closes the connection', async () => {
    const { client, api } = await boot();
    const controller = new AbortController();
    const remove = vi.spyOn(controller.signal, 'removeEventListener');
    let subscribed!: () => void;
    const ready = new Promise<void>((resolve) => (subscribed = resolve));
    const watching = client.watch(() => undefined, { signal: controller.signal, onSubscribed: subscribed });
    await ready;
    await api.stop();
    await watching;
    expect(remove).toHaveBeenCalledWith('abort', expect.any(Function));
  });

  it('does not connect a watch whose signal is already aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    const client = new ApiClient({ socket: '/nonexistent/octocode-test.sock' });
    await expect(client.watch(() => undefined, { signal: controller.signal })).resolves.toBeUndefined();
  });

  it('is off unless the environment asks for it', () => {
    expect(optionsFromEnv({})).toBeUndefined();
    expect(optionsFromEnv({ OCTOCODE_API: '1' }, 'darwin')).toEqual({ socket: true, http: undefined });
    expect(optionsFromEnv({ OCTOCODE_API_HTTP: '0' }, 'darwin')).toEqual({ socket: false, http: 0 });
    expect(optionsFromEnv({ OCTOCODE_API: '1' }, 'win32')).toEqual({ socket: false, http: 0 });
    expect(optionsFromEnv({ OCTOCODE_API_HTTP: '99999' })).toBeUndefined();
  });

  it('answers status and history over the private socket and registers the instance', async () => {
    const { client, api, env } = await boot();
    expect(await client.call('ping')).toEqual({});
    expect(await client.call('status')).toMatchObject({ state: 'idle', cwd: '/work', model: 'anthropic/sonnet', sessionId: 's1' });
    expect(await client.call('messages.list', { limit: 1 })).toMatchObject({ messages: [{ role: 'assistant', text: 'hello' }] });
    const file = api.instance!.socket!;
    expect(fs.statSync(file).mode & 0o777).toBe(0o600);
    expect(listInstances(env['OCTOCODE_API_DIR']!).map((entry) => entry.id)).toEqual([api.instance!.id]);
    await api.stop();
    expect(fs.existsSync(file)).toBe(false);
    expect(listInstances(env['OCTOCODE_API_DIR']!)).toEqual([]);
  });

  it('delivers a message as a triggering turn, steering a busy agent', async () => {
    const idle = { value: true };
    const { client, sent } = await boot({}, idle);
    expect(await client.call('message.send', { text: 'run tests', from: 'ci "bot"\n' })).toMatchObject({ queued: 'turn' });
    idle.value = false;
    expect(await client.call('message.send', { text: 'also lint', mode: 'followUp' })).toMatchObject({ queued: 'followUp' });
    expect(sent.map((entry) => entry.options)).toEqual([{ triggerTurn: true, deliverAs: 'steer' }, { triggerTurn: true, deliverAs: 'followUp' }]);
    expect(sent[0]!.message.content).toBe('Message from ci bot through the Octocode API:\nrun tests');
  });

  it('draws an API message under a ✉ header with the text sanitized and collapsed, not as Markdown', () => {
    const { pi, renderers } = fakePi();
    new OctocodeApi(pi, { list: () => [], tell: () => ({ sent: [] }), maxChars: 100 }, () => '1.0.0', {}).observe();
    const render = renderers.get('octocode-external-message');
    const message = { content: 'Message from ci bot through the Octocode API:\n**bold**\n\u001b[2Jtwo\nthree\nfour', details: { id: 'm1', from: 'ci bot', at: Date.now() } };
    const collapsed = rendered(render(message, { expanded: false }, theme));
    expect(collapsed).toMatch(/^\s*✉ from ci bot · Octocode API · \d\d:\d\d:\d\d/);
    expect(collapsed).not.toContain('through the Octocode API');
    expect(collapsed).toContain('**bold**');
    expect(collapsed).not.toContain('\u001b[2J');
    expect(collapsed).not.toContain('four');
    expect(collapsed).toMatch(/… \+1 line \(ctrl\+o to expand\)/);
    expect(rendered(render(message, { expanded: true }, theme))).toContain('four');
  });

  it('waits for the agent to finish and returns its answer', async () => {
    const { client, emit, sent } = await boot();
    const reply = client.call('message.send', { text: 'summarise', wait: true });
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    emit('agent_end', { messages: [{ role: 'assistant', content: [{ type: 'text', text: 'done: 3 files' }], stopReason: 'stop' }] });
    // `wait` answers only once Pi has settled (it may continue after agent_end).
    emit('agent_settled', {});
    expect(await reply).toMatchObject({ text: 'done: 3 files', stopReason: 'stop', queued: 'turn' });
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
    emit('tool_execution_end', { toolCallId: 't1', toolName: 'bash', isError: false });
    await vi.waitFor(() => expect(seen).toHaveLength(3));
    controller.abort();
    await done;
    expect(seen).toEqual(['1:agent.start', '2:tool.start', '4:tool.end']);

    const replay: string[] = [];
    const again = new AbortController();
    const second = client.watch((event) => replay.push(event.type), { since: 2, types: ['tool.end', 'message.delta'], signal: again.signal });
    await vi.waitFor(() => expect(replay).toHaveLength(2));
    again.abort();
    await second;
    expect(replay).toEqual(['message.delta', 'tool.end']);
  });

  it('marks nested tool calls with parentId and depth, times tool.end, and filters them for topLevel watchers', async () => {
    expect(nesting(undefined)).toEqual({ depth: 0 });
    expect(nesting('toolu_1')).toEqual({ parentId: 'toolu_1', depth: 1 });
    expect(nesting('toolu_1/1')).toEqual({ parentId: 'toolu_1/1', depth: 2 });

    const { client, emit, api } = await boot();
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
    const now = vi.spyOn(Date, 'now');
    now.mockReturnValue(1_000);
    emit('tool_execution_start', { toolCallId: 'toolu_1', toolName: 'codemode', args: {} });
    now.mockReturnValue(1_010);
    emit('tool_execution_start', { toolCallId: 'toolu_1/1', parentToolCallId: 'toolu_1', toolName: 'read', args: { path: 'a.ts' } });
    now.mockReturnValue(1_020);
    emit('tool_execution_start', { toolCallId: 'toolu_1/1/2', parentToolCallId: 'toolu_1/1', toolName: 'bash', args: {} });
    now.mockReturnValue(1_050);
    emit('tool_execution_end', { toolCallId: 'toolu_1/1/2', parentToolCallId: 'toolu_1/1', toolName: 'bash', isError: true });
    now.mockReturnValue(1_060);
    emit('tool_execution_end', { toolCallId: 'toolu_1/1', parentToolCallId: 'toolu_1', toolName: 'read', isError: false });
    now.mockReturnValue(1_100);
    emit('tool_execution_end', { toolCallId: 'toolu_1', toolName: 'codemode', isError: false });
    // An end whose start the bridge never saw has no duration.
    emit('tool_execution_end', { toolCallId: 'toolu_9', toolName: 'bash', isError: false });
    now.mockRestore();
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
      { id: 'toolu_9', name: 'bash', isError: false, depth: 0 },
    ]);
    expect(top.map((event) => `${event.type}:${event.data['id']}`)).toEqual(['tool.start:toolu_1', 'tool.end:toolu_1', 'tool.end:toolu_9']);
    expect(isNestedToolEvent({ seq: 1, at: 0, type: 'message', data: { parentId: 'x' } })).toBe(false);

    // A start whose end never came does not leak into the next session.
    emit('tool_execution_start', { toolCallId: 'toolu_2', toolName: 'bash', args: {} });
    await api.start(fakeCtx({ value: true }), { socket: true });
    const after = new ApiClient(api.instance!);
    const seen: ApiEvent[] = [];
    const again = new AbortController();
    let resubscribed!: () => void;
    const relive = new Promise<void>((resolve) => (resubscribed = resolve));
    const watch = after.watch((event) => seen.push(event), { signal: again.signal, onSubscribed: () => resubscribed() });
    await relive;
    emit('tool_execution_end', { toolCallId: 'toolu_2', toolName: 'bash', isError: false });
    await vi.waitFor(() => expect(seen).toHaveLength(1));
    again.abort();
    await watch;
    expect(seen[0]!.data).toEqual({ id: 'toolu_2', name: 'bash', isError: false, depth: 0 });
  });

  it('rejects unknown methods, bad params and calls after the session ended', async () => {
    const { client, api } = await boot();
    await expect(client.call('nope')).rejects.toMatchObject({ code: -32601 });
    await expect(client.call('message.send', { text: '' })).rejects.toMatchObject({ code: -32602 });
    await expect(client.call('message.send', { text: 'x', mode: 'later' })).rejects.toBeInstanceOf(ApiCallError);
    expect(parseRequest('{"jsonrpc":"1.0"}')).toMatchObject({ error: { code: -32600 } });
    expect(parseRequest('{')).toMatchObject({ error: { code: -32700 } });
    await api.stop();
    await expect(client.call('ping')).rejects.toThrow();
  });

  it('forwards agent listing and tell to the team', async () => {
    const { client, told } = await boot();
    expect(await client.call('agents.list')).toEqual({ agents: [{ id: 'main-1' }] });
    await client.call('agents.tell', { to: 'main-1', text: 'ping' });
    expect(told).toEqual([['main-1', 'ping']]);
  });

  it('caps agents.tell at the team message limit and turns a refusal into an RPC error', async () => {
    const { client, told } = await boot();
    await expect(client.call('agents.tell', { to: 'main-1', text: 'x'.repeat(101) })).rejects.toThrow();
    await expect(client.call('agents.tell', { to: 'ghost', text: 'hi' })).rejects.toMatchObject({ message: expect.stringContaining('No agent "ghost".') });
    expect(told).toEqual([['ghost', 'hi']]);
  });

  it('serves HTTP on loopback only with the bearer token and no browser origins', async () => {
    const { api } = await boot({ http: 0 });
    const { url, token } = api.instance!.http!;
    const request = (headers: Record<string, string>) =>
      new Promise<{ status: number; body: string }>((resolve, reject) => {
        const req = http.request(`${url}/rpc`, { method: 'POST', headers }, (res) => {
          let body = '';
          res.on('data', (chunk) => (body += chunk));
          res.on('end', () => resolve({ status: res.statusCode!, body }));
        });
        req.on('error', reject);
        req.end(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'ping' }));
      });
    expect((await request({})).status).toBe(401);
    expect((await request({ authorization: 'Bearer wrong' })).status).toBe(401);
    expect((await request({ authorization: `Bearer ${token}`, origin: 'https://evil.example' })).status).toBe(401);
    expect((await request({ authorization: `Bearer ${token}`, host: 'evil.example' })).status).toBe(401);
    expect(JSON.parse((await request({ authorization: `Bearer ${token}` })).body)).toEqual({ jsonrpc: '2.0', id: 1, result: {} });
    expect(await new ApiClient({ http: api.instance!.http! }).call('status')).toMatchObject({ state: 'idle' });
  });

  it('survives a client that resets the connection mid-body', async () => {
    const { api } = await boot({ http: 0 });
    const { url, token } = api.instance!.http!;
    const rejections: unknown[] = [];
    const onRejection = (reason: unknown) => rejections.push(reason);
    process.on('unhandledRejection', onRejection);
    try {
      await new Promise<void>((resolve) => {
        const req = http.request(`${url}/rpc`, { method: 'POST', headers: { authorization: `Bearer ${token}`, 'content-length': '1000' } });
        req.on('error', () => resolve());
        req.write('{"jsonrpc":"2.0",', () => setTimeout(() => req.destroy(), 50));
        req.on('close', () => resolve());
      });
      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(rejections).toEqual([]);
      expect(await new ApiClient({ http: api.instance!.http! }).call('status')).toMatchObject({ state: 'idle' });
    } finally {
      process.off('unhandledRejection', onRejection);
    }
  });

  it('streams GET /events from the next event, replays only on since or Last-Event-ID, and marks a gap', async () => {
    const { api, emit } = await boot({ http: 0 });
    const { url, token } = api.instance!.http!;
    emit('agent_start', {});
    emit('tool_execution_start', { toolCallId: 't1', toolName: 'bash', args: {} });
    const plain = sse(url, token);
    const fromOne = sse(url, token, { query: '?since=1' });
    const resumed = sse(url, token, { 'last-event-id': '0' });
    const blank = sse(url, token, { query: '?since=' });
    expect(await Promise.all([plain.ready, fromOne.ready, resumed.ready, blank.ready])).toEqual([200, 200, 200, 200]);
    await vi.waitFor(() => expect(fromOne.ids()).toEqual([2]));
    await vi.waitFor(() => expect(resumed.ids()).toEqual([1, 2]));
    emit('tool_execution_end', { toolCallId: 't1', toolName: 'bash', isError: false });
    // A plain connection (and an empty since) never replays the buffer: only the new event arrives.
    await vi.waitFor(() => expect(plain.ids()).toEqual([3]));
    await vi.waitFor(() => expect(blank.ids()).toEqual([3]));
    await vi.waitFor(() => expect(fromOne.ids()).toEqual([2, 3]));
    expect(plain.text()).toContain('event: tool.end\n');
    expect(plain.text()).not.toContain(': gap');
    for (let index = 0; index < 600; index += 1) emit('agent_start', {});
    const late = sse(url, token, { query: '?since=1&types=tool.end' });
    await late.ready;
    await vi.waitFor(() => expect(late.text()).toContain(': gap'));
    expect(late.ids()).toEqual([]);
    for (const stream of [plain, fromOne, resumed, blank, late]) stream.close();
  });

  it('replays only what a resumed subscriber missed and reports lost events', () => {
    const log = new EventLog();
    for (let index = 0; index < 600; index += 1) log.publish('message', { index });
    const got: number[] = [];
    expect(log.subscribe((event) => got.push(event.seq), { since: 595 }).gap).toBe(false);
    expect(got).toEqual([596, 597, 598, 599, 600]);
    expect(log.subscribe(() => undefined, { since: 5 }).gap).toBe(true);
  });
});
