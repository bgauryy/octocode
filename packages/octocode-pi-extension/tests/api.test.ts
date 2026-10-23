import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiCallError, ApiClient } from '../src/api/client.js';
import { EventLog } from '../src/api/events.js';
import { optionsFromEnv } from '../src/api/options.js';
import { parseRequest } from '../src/api/protocol.js';
import { apiDir, listInstances } from '../src/api/registry.js';
import { boot, sse, stopApis } from './api-harness.js';

afterEach(stopApis);

describe('external API: transports, lifecycle and history', () => {
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
    for (const bad of ['99999', '-1', 'abc', '80.5']) expect(() => optionsFromEnv({ OCTOCODE_API: '1', OCTOCODE_API_HTTP: bad })).toThrow(`OCTOCODE_API_HTTP=${bad} is not a port`);
  });

  it('answers status and history over the private socket and registers the instance', async () => {
    const { client, api, env } = await boot();
    expect(await client.call('ping')).toEqual({});
    expect(await client.call('status')).toMatchObject({ state: 'idle', cwd: '/work', model: 'anthropic/sonnet', sessionId: 's1' });
    expect(await client.call('messages.list', { limit: 1 })).toMatchObject({ messages: [{ role: 'assistant', text: 'hello' }] });
    const file = api.instance!.socket!;
    expect(fs.statSync(file).mode & 0o777).toBe(0o600);
    expect(listInstances(apiDir(env)).map((entry) => entry.id)).toEqual([api.instance!.id]);
    await api.stop();
    expect(fs.existsSync(file)).toBe(false);
    expect(listInstances(apiDir(env))).toEqual([]);
  });

  it('refuses a socket frame over the byte limit, measured in UTF-8 bytes', async () => {
    const { api } = await boot();
    // 600k two-byte characters: under the limit in UTF-16 units, over it in UTF-8 bytes.
    const reply = await new Promise<string>((resolve, reject) => {
      let text = '';
      const socket = net.connect(api.instance!.socket!, () => socket.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'ping', params: { pad: 'é'.repeat(600_000) } })}\n`));
      socket.setEncoding('utf8');
      socket.on('data', (chunk: string) => (text += chunk));
      socket.on('close', () => resolve(text));
      // The server may close while the client is still writing the rest of the frame.
      socket.on('error', (error: NodeJS.ErrnoException) => (error.code === 'EPIPE' || error.code === 'ECONNRESET' ? undefined : reject(error)));
    });
    expect(reply).not.toContain('"result"');
    expect(await new ApiClient(api.instance!).call('ping')).toEqual({});
  });

  it('pages messages.list by index and saves a text over the limit whole instead of cutting it', async () => {
    const message = (role: string, text: string) => ({ type: 'message', message: { role, content: text } });
    const long = `${'a'.repeat(70_000)}tail`;
    const { client } = await boot({}, { value: true }, [message('user', 'q1'), message('assistant', 'a1'), message('user', 'q2'), message('assistant', long), message('user', 'q3')]);
    const first = (await client.call('messages.list', { limit: 2 })) as { messages: Array<{ index: number; text: string }>; total: number; earlier?: { before: number } };
    expect(first.messages.map((m) => [m.index, m.text])).toEqual([[5, expect.stringContaining('more characters cut; the whole text is in ')], [6, 'q3']]);
    expect(first).toMatchObject({ total: 7, earlier: { before: 5 } });
    const file = /the whole text is in (\S+) \(sha256 [0-9a-f]+, \d+ bytes\)\]/.exec(first.messages[0]!.text)![1]!;
    expect(fs.readFileSync(file, 'utf8')).toBe(long);
    const second = (await client.call('messages.list', { limit: 2, before: 5 })) as { messages: Array<{ index: number; text: string }>; earlier?: { before: number } };
    expect(second.messages.map((m) => [m.index, m.text])).toEqual([[3, 'a1'], [4, 'q2']]);
    expect(second.earlier).toEqual({ before: 3 });
    const last = (await client.call('messages.list', { limit: 100, before: 3 })) as { messages: unknown[]; earlier?: unknown };
    expect(last.messages).toHaveLength(3);
    expect(last.earlier).toBeUndefined();
    // A value outside the range is brought into it, never refused; only a non-number is.
    expect(await client.call('messages.list', { limit: 1, before: -1 })).toMatchObject({ messages: [], limit: 1, total: 7 });
    await expect(client.call('messages.list', { before: 'x' })).rejects.toThrow(/before must be a number/);
  });

  it('clamps a messages.list page size and reaches every message exactly once through `earlier`', async () => {
    const message = (n: number) => ({ type: 'message', message: { role: n % 2 ? 'assistant' : 'user', content: `m${n}` } });
    const { client } = await boot({}, { value: true }, Array.from({ length: 248 }, (_, n) => message(n + 2)));
    type Page = { messages: Array<{ index: number; text: string }>; total: number; limit: number; earlier?: { before: number } };
    const seen: number[] = [];
    let page = (await client.call('messages.list', { limit: 1_000_000 })) as Page;
    expect(page).toMatchObject({ total: 250, limit: 100 });
    for (;;) {
      expect(page.messages.length).toBeLessThanOrEqual(100);
      seen.unshift(...page.messages.map((m) => m.index));
      if (!page.earlier) break;
      page = (await client.call('messages.list', { limit: 1_000_000, before: page.earlier.before })) as Page;
    }
    expect(seen).toEqual(Array.from({ length: 250 }, (_, n) => n));
    // A limit below 1 is raised to 1 and a fractional `before` is floored: one message, the one just before it.
    expect(await client.call('messages.list', { limit: 0, before: 10.9 })).toMatchObject({ limit: 1, messages: [{ index: 9 }], earlier: { before: 9 } });
    // A `before` past the end is the end.
    expect(await client.call('messages.list', { limit: 2, before: 99_999 })).toMatchObject({ messages: [{ index: 248 }, { index: 249 }], earlier: { before: 248 } });
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

describe('status of a session that cannot be read', () => {
  it('says the session was replaced only for Pi\'s stale-context error, and reports any other failure as it is', async () => {
    const t = await boot();
    const usage = vi.spyOn((t.api as unknown as { ctx: { getContextUsage(): unknown } }).ctx, 'getContextUsage');
    usage.mockImplementationOnce(() => {
      throw new Error('This extension ctx is stale after session replacement or reload.');
    });
    const replaced = await t.client.call('status').catch((error: unknown) => error);
    expect(replaced).toBeInstanceOf(ApiCallError);
    expect(replaced).toMatchObject({ code: -32000, message: 'The session was replaced' });
    // Nothing was sent or accepted by a status read: no execution claim.
    expect((replaced as ApiCallError).data).toBeUndefined();

    usage.mockImplementationOnce(() => {
      throw new Error('disk on fire');
    });
    await expect(t.client.call('status')).rejects.toMatchObject({ code: -32603, message: 'The session status could not be read: disk on fire' });
    expect(await t.client.call('status')).toMatchObject({ state: 'idle' });
  });
});
