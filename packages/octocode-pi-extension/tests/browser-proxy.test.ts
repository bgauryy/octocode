import net from 'node:net';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { accepts, cleanUp, connectVia, ESTABLISHED, FORBIDDEN, origin, PRIVATE_IP, PUBLIC_IP, readUntil, redirectTo, resolver, start } from './browser-proxy-harness.js';

afterEach(async () => {
  vi.unstubAllEnvs();
  await cleanUp();
});

describe('filtering proxy: CONNECT', () => {
  it('refuses a CONNECT to a loopback address and reports it', async () => {
    const server = await origin();
    const onBlocked = vi.fn();
    const proxy = await start({ onBlocked });
    const { status, head, socket } = await connectVia(proxy, `127.0.0.1:${server.port}`);
    expect(status).toBe(FORBIDDEN);
    expect(head).toContain('X-Octocode-Blocked: private');
    expect(onBlocked).toHaveBeenCalledExactlyOnceWith({ host: `127.0.0.1:${server.port}`, reason: 'private' });
    await readUntil(socket, () => false);
    expect(server.connections()).toBe(0);
  });

  it('refuses numeric spellings of a private address, and IPv6 loopback', async () => {
    const onBlocked = vi.fn();
    const proxy = await start({ onBlocked });
    const authorities = ['2130706433:80', '0x7f.1:80', '127.1:80', '[::1]:80', '[::ffff:7f00:1]:80', '169.254.169.254:80', '0.0.0.0:80'];
    for (const authority of authorities) expect((await connectVia(proxy, authority)).status, authority).toBe(FORBIDDEN);
    expect(onBlocked).toHaveBeenCalledTimes(authorities.length);
  });

  it('refuses a name that resolves to a private address, even when it also resolves to a public one', async () => {
    const onBlocked = vi.fn();
    const lookup = resolver({ 'internal.test': [PRIVATE_IP], 'mixed.test': [PUBLIC_IP, PRIVATE_IP] });
    const proxy = await start({ lookup, onBlocked });
    expect((await connectVia(proxy, 'internal.test:443')).status).toBe(FORBIDDEN);
    expect((await connectVia(proxy, 'mixed.test:443')).status).toBe(FORBIDDEN);
    expect(onBlocked.mock.calls.map(([blocked]) => blocked)).toEqual([
      { host: 'internal.test:443', reason: 'private' },
      { host: 'mixed.test:443', reason: 'private' },
    ]);
  });

  it('connects to the address it vetted, so a DNS answer that changes afterwards cannot reach a private one', async () => {
    const server = await origin();
    let answers = 0;
    const lookup = vi.fn(async () => [{ address: answers++ === 0 ? PUBLIC_IP : PRIVATE_IP }]);
    const { connect, dialed } = redirectTo(server.port);
    const proxy = await start({ lookup, connect });
    const { status, socket } = await connectVia(proxy, 'rebind.test:80');
    expect(status).toBe(ESTABLISHED);
    socket.write('GET / HTTP/1.1\r\nHost: rebind.test\r\nConnection: close\r\n\r\n');
    expect(await readUntil(socket, (text) => text.includes('hello from origin'))).toContain('hello from origin');
    // One resolution per request, and the connection used its result; the private second answer was never dialed.
    expect(lookup).toHaveBeenCalledTimes(1);
    expect(dialed).toEqual([`${PUBLIC_IP}:80`]);
    // The next request resolves again, and is now refused.
    expect((await connectVia(proxy, 'rebind.test:80')).status).toBe(FORBIDDEN);
    expect(dialed).toHaveLength(1);
  });

  it('tunnels to a public host', async () => {
    const server = await origin();
    const { connect, dialed } = redirectTo(server.port);
    const proxy = await start({ lookup: resolver({ 'example.test': [PUBLIC_IP] }), connect });
    const { status, socket } = await connectVia(proxy, 'example.test:443');
    expect(status).toBe(ESTABLISHED);
    socket.write('GET /x HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n');
    expect(await readUntil(socket, () => false)).toContain('hello from origin');
    expect(server.requests).toEqual([{ url: '/x', host: 'example.test', method: 'GET' }]);
    expect(dialed).toEqual([`${PUBLIC_IP}:443`]);
  });

  it('tunnels to a private host once the agent opened it, and only on that port', async () => {
    const server = await origin();
    const other = await origin();
    const onBlocked = vi.fn();
    const proxy = await start({ onBlocked });
    proxy.allow(new URL(`http://127.0.0.1:${server.port}/page`));
    const { status, socket } = await connectVia(proxy, `127.0.0.1:${server.port}`);
    expect(status).toBe(ESTABLISHED);
    socket.write('GET /api HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n');
    expect(await readUntil(socket, () => false)).toContain('hello from origin');
    expect(onBlocked).not.toHaveBeenCalled();
    expect((await connectVia(proxy, `127.0.0.1:${other.port}`)).status).toBe(FORBIDDEN);
    expect(other.connections()).toBe(0);
  });

  it('connects an allowed private name to the address it resolved to, matching the host case-insensitively', async () => {
    const server = await origin();
    const dialed: string[] = [];
    const connect = (address: string) => {
      dialed.push(address);
      return net.connect({ host: '127.0.0.1', port: server.port });
    };
    const proxy = await start({ lookup: resolver({ 'dev.test': ['127.0.0.1'] }), connect });
    expect((await connectVia(proxy, 'dev.test:3000')).status).toBe(FORBIDDEN);
    proxy.allow(new URL('http://dev.test:3000/'));
    expect((await connectVia(proxy, 'DEV.test:3000')).status).toBe(ESTABLISHED);
    expect(dialed).toEqual(['127.0.0.1']);
  });

  it('refuses a WebSocket upgrade to a blocked host, and carries one to an allowed host', async () => {
    const server = await origin();
    const proxy = await start();
    // Chrome tunnels ws:// and wss:// with CONNECT too: refused, nothing reaches the origin.
    expect((await connectVia(proxy, `127.0.0.1:${server.port}`)).status).toBe(FORBIDDEN);
    expect(server.upgrades).toEqual([]);
    expect(server.connections()).toBe(0);
    proxy.allow(new URL(`ws://127.0.0.1:${server.port}/`));
    const allowed = await connectVia(proxy, `127.0.0.1:${server.port}`);
    expect(allowed.status).toBe(ESTABLISHED);
    allowed.socket.write('GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n');
    expect(await readUntil(allowed.socket, (text) => text.includes('ws-open'))).toContain('101 Switching Protocols');
    expect(server.upgrades).toEqual(['/ws']);
  });

  it('refuses a name nothing here resolves, and reports it only when an upstream proxy might have taken it', async () => {
    const onBlocked = vi.fn();
    const proxy = await start({ lookup: resolver({}), onBlocked });
    expect((await connectVia(proxy, 'nowhere.test:443')).status).toBe(FORBIDDEN);
    expect(onBlocked).not.toHaveBeenCalled();
    const chained = await start({ lookup: resolver({}), onBlocked, env: { HTTPS_PROXY: 'http://127.0.0.1:9' } });
    expect((await connectVia(chained, 'nowhere.test:443')).status).toBe(FORBIDDEN);
    expect(onBlocked).toHaveBeenCalledExactlyOnceWith({ host: 'nowhere.test:443', reason: 'unresolved' });
  });

  it('rejects a CONNECT without a host and port', async () => {
    const proxy = await start();
    for (const authority of ['example.test', 'a@b:80', 'x:0', 'x:99999']) expect((await connectVia(proxy, authority)).status, authority).toBe('HTTP/1.1 400 Bad Request');
  });

  it('answers 502 when the vetted address does not accept the connection', async () => {
    const closed = await new Promise<number>((resolve) => {
      const server = net.createServer().listen(0, '127.0.0.1', () => {
        const { port } = server.address() as net.AddressInfo;
        server.close(() => resolve(port));
      });
    });
    const { connect } = redirectTo(closed);
    const proxy = await start({ lookup: resolver({ 'down.test': [PUBLIC_IP] }), connect });
    expect((await connectVia(proxy, 'down.test:80')).status).toBe('HTTP/1.1 502 Bad Gateway');
  });

  it('stops accepting connections and cuts open tunnels on close', async () => {
    const server = await origin();
    const proxy = await start();
    proxy.allow(new URL(`http://127.0.0.1:${server.port}`));
    const { socket } = await connectVia(proxy, `127.0.0.1:${server.port}`);
    const closed = new Promise<void>((resolve) => socket.once('close', () => resolve()));
    await proxy.close();
    await closed;
    expect(await accepts(proxy.port)).toBe(false);
  });
});
