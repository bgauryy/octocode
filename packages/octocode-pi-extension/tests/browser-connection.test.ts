import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CdpConnection } from '../src/browser/cdp.js';
import type { PageTarget } from '../src/browser/devtools.js';
import { NO_REPLY, startCdpStub, type CdpStub, type CdpStubOptions } from './fixtures/cdp-stub.js';

const stubs: CdpStub[] = [];
const connections: CdpConnection[] = [];
const servers: http.Server[] = [];

afterEach(async () => {
  for (const connection of connections.splice(0)) connection.close();
  for (const server of servers.splice(0)) server.close();
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
});

const target = (stub: CdpStub, id: string): PageTarget => ({ id, webSocketDebuggerUrl: `ws://127.0.0.1:${stub.port}/devtools/page/${id}` });

/** A stub with two page targets, `a` and `b`, and one browser connection with a session on each. */
async function twoPages(options: CdpStubOptions = {}) {
  const stub = await startCdpStub({ targets: [{ id: 'a', type: 'page' }, { id: 'b', type: 'page' }], ...options });
  stubs.push(stub);
  const connection = await CdpConnection.connect(stub.port);
  connections.push(connection);
  const a = await connection.attach(target(stub, 'a'));
  const b = await connection.attach(target(stub, 'b'));
  return { stub, connection, a, b };
}

const later = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe('untilDialog', () => {
  it('ends at a dialog, and what the work does after that is not an unhandled rejection', async () => {
    const { stub, a } = await twoPages();
    const unhandled: unknown[] = [];
    const record = (reason: unknown) => unhandled.push(reason);
    process.on('unhandledRejection', record);
    try {
      let fail: (error: Error) => void = () => undefined;
      const work = new Promise<string>((_resolve, reject) => (fail = reject));
      const waiting = a.untilDialog(work);
      stub.pages.get('a')!.emit('Page.javascriptDialogOpening', { type: 'alert', message: 'hi' });
      await expect(waiting).resolves.toBeUndefined();
      fail(new Error('page torn down'));
      await later(50);
      expect(unhandled).toEqual([]);
      // Without a dialog the work's own outcome is the result, a failure included.
      await expect(a.untilDialog(Promise.reject(new Error('boom')))).rejects.toThrow('boom');
      await expect(a.untilDialog(Promise.resolve('done'))).resolves.toBe('done');
    } finally {
      process.off('unhandledRejection', record);
    }
  });
});

describe('the browser connection', () => {
  it('delivers each target\'s events to its own session only', async () => {
    const { stub, a, b } = await twoPages();
    const seenA: string[] = [];
    const seenB: string[] = [];
    a.on((method, params) => seenA.push(`${method}:${String(params['n'])}`));
    b.on((method, params) => seenB.push(`${method}:${String(params['n'])}`));
    stub.pages.get('a')!.emit('Page.loadEventFired', { n: 1 });
    stub.pages.get('b')!.emit('Page.frameNavigated', { n: 2 });
    stub.pages.get('a')!.emit('Page.frameNavigated', { n: 3 });
    await a.send('Ping');
    await b.send('Ping');
    expect(seenA).toEqual(['Page.loadEventFired:1', 'Page.frameNavigated:3']);
    expect(seenB).toEqual(['Page.frameNavigated:2']);
  });

  it('sends each command on its own session and the same id space never mixes the replies', async () => {
    const { stub, a, b } = await twoPages({ handle: (method, params, page) => ({ target: page.id, method, echo: params['value'] }) });
    const [fromA, fromB] = await Promise.all([a.send('Echo', { value: 1 }), b.send('Echo', { value: 2 })]);
    expect(fromA).toEqual({ target: 'a', method: 'Echo', echo: 1 });
    expect(fromB).toEqual({ target: 'b', method: 'Echo', echo: 2 });
    expect(stub.commands.map((command) => `${command.target}:${command.method}`)).toEqual(['a:Echo', 'b:Echo']);
  });

  it('never settles one session\'s pending call with a reply that names another session', async () => {
    let release: (() => void) | undefined;
    const { stub, a, b } = await twoPages({
      handle: async (method, _params, page) => {
        if (method === 'Late') await new Promise<void>((resolve) => (release = resolve));
        // Session a's page answers as if it were the reply to every id b might be waiting on.
        if (method === 'Spoof') for (let id = 1; id < 40; id += 1) page.raw(JSON.stringify({ id, sessionId: 'session-1', result: { spoofed: true } }));
        return { real: method };
      },
    });
    let settled = false;
    const pending = b.send('Late').finally(() => (settled = true));
    await vi.waitFor(() => expect(release).toBeDefined());
    expect(stub.pages.get('a')).toBeDefined();
    await a.send('Spoof');
    await later(50);
    expect(settled).toBe(false);
    release!();
    await expect(pending).resolves.toEqual({ real: 'Late' });
  });

  it('ends only the closed page\'s session: its pending call is rejected, the other session and the socket stay', async () => {
    const { connection, a, b, stub } = await twoPages({ handle: (method) => (method === 'Slow.never' ? NO_REPLY : {}) });
    const pendingA = a.send('Slow.never');
    const pendingB = b.send('Slow.never');
    a.close();
    await expect(pendingA).rejects.toThrow('Browser connection closed');
    expect(a.open).toBe(false);
    expect(b.open).toBe(true);
    expect(connection.open).toBe(true);
    await expect(a.send('Page.enable')).rejects.toThrow('Browser connection closed');
    await expect(b.send('Page.enable')).resolves.toEqual({});
    const seen: string[] = [];
    b.on((method) => seen.push(method));
    stub.pages.get('b')!.emit('Page.loadEventFired');
    await b.send('Page.enable');
    expect(seen).toEqual(['Page.loadEventFired']);
    let settled = false;
    void pendingB.catch(() => undefined).finally(() => (settled = true));
    await later(30);
    expect(settled).toBe(false);
  });

  it('drops a session whose target went away, leaving the rest', async () => {
    const { a, b, stub, connection } = await twoPages({ handle: (method) => (method === 'Slow.never' ? NO_REPLY : {}) });
    const pendingA = a.send('Slow.never');
    stub.pages.get('a')!.drop();
    await expect(pendingA).rejects.toThrow('Browser connection closed');
    expect(a.open).toBe(false);
    expect(b.open).toBe(true);
    expect(connection.open).toBe(true);
  });

  it('rejects every pending call, in every session and on the browser, when the browser socket closes', async () => {
    const { connection, a, b, stub } = await twoPages({ handle: (method) => (method === 'Slow.never' ? NO_REPLY : {}) });
    const pending = [a.send('Slow.never'), b.send('Slow.never'), connection.send('Slow.never')];
    await vi.waitFor(() => expect(stub.commands.filter((command) => command.method === 'Slow.never')).toHaveLength(3));
    stub.browser!.drop();
    for (const call of pending) await expect(call).rejects.toThrow('Browser connection closed');
    await vi.waitFor(() => expect(connection.open).toBe(false));
    expect(a.open).toBe(false);
    expect(b.open).toBe(false);
    await expect(b.send('Page.enable')).rejects.toThrow('Browser connection closed');
  });

  it('keeps the browser socket open until the connection closes, then ends every session', async () => {
    const { connection, a, b } = await twoPages();
    a.close();
    expect(connection.open).toBe(true);
    connection.close();
    expect(connection.open).toBe(false);
    expect(b.open).toBe(false);
  });

  it('settles quietly when the browser goes away while a session is being detached', async () => {
    const { connection, a } = await twoPages();
    a.close();
    connection.close();
    await later(30);
    expect(a.open).toBe(false);
  });

  it('stops waiting for a command when its signal aborts, and rejects an already-aborted one without sending', async () => {
    const { a, stub } = await twoPages({ handle: (method) => (method === 'Slow.never' ? NO_REPLY : {}) });
    const controller = new AbortController();
    const pending = a.send('Slow.never', {}, controller.signal);
    controller.abort(new Error('user stopped'));
    await expect(pending).rejects.toThrow('user stopped');
    const before = stub.commands.length;
    await expect(a.send('Page.enable', {}, controller.signal)).rejects.toThrow('user stopped');
    expect(stub.commands).toHaveLength(before);
  });

  it('rejects a command at once when the socket cannot send it, and leaves the session usable', async () => {
    const { a } = await twoPages();
    const send = vi.spyOn(WebSocket.prototype, 'send').mockImplementationOnce(() => {
      throw new Error('socket is closing');
    });
    try {
      await expect(a.send('Page.enable')).rejects.toThrow('Could not send Page.enable: socket is closing');
    } finally {
      send.mockRestore();
    }
    await expect(a.send('Page.enable')).resolves.toEqual({});
  });

  it('answers a malformed or unknown frame without breaking the session', async () => {
    const { a, stub } = await twoPages();
    stub.pages.get('a')!.raw('not json');
    stub.browser!.raw(JSON.stringify({ method: 'Page.loadEventFired', sessionId: 'session-unknown' }));
    stub.browser!.raw(JSON.stringify({ id: 9999, result: {} }));
    await expect(a.send('Page.enable')).resolves.toEqual({});
  });
});

describe('connecting to a browser', () => {
  it('reports a browser socket that cannot be opened', async () => {
    const server = http
      .createServer((_request, response) => {
        response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ webSocketDebuggerUrl: `ws://127.0.0.1:${(server.address() as AddressInfo).port}/devtools/browser/x` }));
      })
      .on('upgrade', (_request, socket) => socket.destroy());
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    await expect(CdpConnection.connect((server.address() as AddressInfo).port)).rejects.toThrow(/Cannot connect to ws:\/\/127\.0\.0\.1:\d+\/devtools\/browser\/x/);
  });
});

describe('the browser connection refuses a DevTools URL that is not the browser\'s own', () => {
  async function versionServer(url: (port: number) => unknown): Promise<number> {
    let upgraded = false;
    const server = http
      .createServer((_request, response) => {
        response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ webSocketDebuggerUrl: url((server.address() as AddressInfo).port) }));
      })
      .on('upgrade', () => void (upgraded = true));
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    afterEachCheck.push(() => expect(upgraded).toBe(false));
    return (server.address() as AddressInfo).port;
  }
  const afterEachCheck: Array<() => void> = [];
  afterEach(() => afterEachCheck.splice(0).forEach((check) => check()));

  it('a browser URL on another host, before any socket opens', async () => {
    const port = await versionServer((p) => `ws://evil.example:${p}/devtools/browser/x`);
    await expect(CdpConnection.connect(port)).rejects.toThrow(/Refused the DevTools URL ws:\/\/evil\.example:\d+\/devtools\/browser\/x: host evil\.example is not this machine/);
  });

  it('a browser URL on another port or path', async () => {
    const wrongPort = await versionServer((p) => `ws://127.0.0.1:${p + 1}/devtools/browser/x`);
    await expect(CdpConnection.connect(wrongPort)).rejects.toThrow(/port \d+ is not \d+/);
    const wrongPath = await versionServer((p) => `ws://127.0.0.1:${p}/other`);
    await expect(CdpConnection.connect(wrongPath)).rejects.toThrow(/path \/other is not under \/devtools\//);
  });

  it('a browser that reports no DevTools URL', async () => {
    const port = await versionServer(() => undefined);
    await expect(CdpConnection.connect(port)).rejects.toThrow(`Chrome on port ${port} reports no browser DevTools URL`);
  });

  it('a target whose URL points elsewhere, before any command is sent for it', async () => {
    const { connection, stub } = await twoPages();
    const before = stub.pages.size;
    await expect(connection.attach({ id: 'a', webSocketDebuggerUrl: `ws://evil.example:${stub.port}/devtools/page/a` })).rejects.toThrow(/host evil\.example is not this machine/);
    await expect(connection.attach({ id: 'a', webSocketDebuggerUrl: `ws://127.0.0.1:${stub.port + 1}/devtools/page/a` })).rejects.toThrow(/is not/);
    expect(stub.pages.size).toBe(before);
  });

  it('a target the browser does not know', async () => {
    const { connection, stub } = await twoPages();
    await expect(connection.attach(target(stub, 'gone'))).rejects.toThrow('No target with given id found');
  });
});
