import http from 'node:http';
import net from 'node:net';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanUp, connectVia, ESTABLISHED, FORBIDDEN, getVia, listen, origin, PRIVATE_IP, PUBLIC_IP, rawSocket, readUntil, redirectTo, resolver, start } from './browser-proxy-harness.js';
import { startFilteringProxy } from '../src/browser/proxy.js';

afterEach(async () => {
  vi.unstubAllEnvs();
  await cleanUp();
});

describe('filtering proxy: plain HTTP', () => {
  it('refuses a private target with a 403 page naming it', async () => {
    const server = await origin();
    const onBlocked = vi.fn();
    const proxy = await start({ onBlocked });
    const response = await getVia(proxy, `http://127.0.0.1:${server.port}/secret`);
    expect(response).toMatch(/^HTTP\/1\.1 403 Forbidden/);
    expect(response).toContain(`Blocked by Octocode: 127.0.0.1:${server.port}`);
    expect(onBlocked).toHaveBeenCalledExactlyOnceWith({ host: `127.0.0.1:${server.port}`, reason: 'private' });
    expect(server.connections()).toBe(0);
  });

  it('forwards to a public host and an allowed private one, keeping the path, Host and response headers', async () => {
    const server = await origin();
    const { connect, dialed } = redirectTo(server.port);
    const proxy = await start({ lookup: resolver({ 'example.test': [PUBLIC_IP] }), connect });
    const publicResponse = await getVia(proxy, 'http://example.test/a/b?q=1');
    expect(publicResponse).toMatch(/^HTTP\/1\.1 200 OK/);
    expect(publicResponse.toLowerCase()).toContain('x-origin: yes');
    expect(publicResponse).toContain('hello from origin');
    expect(dialed).toEqual([`${PUBLIC_IP}:80`]);
    proxy.allow(new URL(`http://127.0.0.1:${server.port}/`));
    expect(await getVia(proxy, `http://127.0.0.1:${server.port}/local`)).toContain('hello from origin');
    expect(server.requests).toEqual([
      { url: '/a/b?q=1', host: 'example.test', method: 'GET' },
      { url: '/local', host: `127.0.0.1:${server.port}`, method: 'GET' },
    ]);
  });

  it('forwards a request body', async () => {
    let body = '';
    const echo = http.createServer((request, response) => {
      request.on('data', (chunk: Buffer) => (body += chunk.toString()));
      request.on('end', () => response.end('ok'));
    });
    const port = await listen(echo);
    const proxy = await start();
    proxy.allow(new URL(`http://127.0.0.1:${port}`));
    const socket = await rawSocket(proxy.port);
    socket.write(`POST http://127.0.0.1:${port}/submit HTTP/1.1\r\nHost: 127.0.0.1:${port}\r\nContent-Length: 7\r\nConnection: close\r\n\r\nname=ab`);
    expect(await readUntil(socket, () => false)).toMatch(/\r\n\r\nok$/);
    expect(body).toBe('name=ab');
  });

  it('rejects a request that is not an absolute http URL', async () => {
    const proxy = await start();
    const socket = await rawSocket(proxy.port);
    socket.write('GET /status HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n');
    expect(await readUntil(socket, () => false)).toMatch(/^HTTP\/1\.1 400/);
  });

  it('answers 502 when the host cannot be reached', async () => {
    const proxy = await start({ lookup: resolver({ 'down.test': [PUBLIC_IP] }), connect: () => net.connect({ host: '127.0.0.1', port: 1 }) });
    expect(await getVia(proxy, 'http://down.test/')).toMatch(/^HTTP\/1\.1 502/);
  });
});

/** An upstream HTTP proxy that tunnels every CONNECT to `to`, and records what it was asked for. */
async function upstream(to: number, options: { refuse?: boolean } = {}) {
  const connects: Array<{ target: string | undefined; auth: string | undefined }> = [];
  const forwarded: Array<{ url: string | undefined; auth: string | undefined }> = [];
  const server = http.createServer((request, response) => {
    forwarded.push({ url: request.url, auth: request.headers['proxy-authorization'] });
    response.end('via upstream');
  });
  server.on('connect', (request, client: net.Socket) => {
    connects.push({ target: request.url, auth: request.headers['proxy-authorization'] });
    if (options.refuse) {
      client.end('HTTP/1.1 407 Proxy Authentication Required\r\n\r\n');
      return;
    }
    const remote = net.connect({ host: '127.0.0.1', port: to }, () => {
      client.write('HTTP/1.1 200 OK\r\n\r\n');
      client.pipe(remote);
      remote.pipe(client);
    });
    remote.on('error', () => client.destroy());
    client.on('error', () => remote.destroy());
  });
  return { port: await listen(server), connects, forwarded };
}

describe('filtering proxy: upstream proxy', () => {
  it('tunnels a vetted host through HTTPS_PROXY by name, with the proxy URL credentials', async () => {
    const server = await origin();
    const chain = await upstream(server.port);
    const { connect, dialed } = redirectTo(server.port);
    const proxy = await start({ lookup: resolver({ 'example.test': [PUBLIC_IP] }), connect, env: { HTTPS_PROXY: `http://user:pw@127.0.0.1:${chain.port}` } });
    const { status, socket } = await connectVia(proxy, 'example.test:443');
    expect(status).toBe(ESTABLISHED);
    socket.write('GET / HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n');
    expect(await readUntil(socket, () => false)).toContain('hello from origin');
    expect(chain.connects).toEqual([{ target: 'example.test:443', auth: `Basic ${Buffer.from('user:pw').toString('base64')}` }]);
    // The only direct dial was to the upstream proxy itself.
    expect(dialed).toEqual([`127.0.0.1:${chain.port}`]);
  });

  it('still vets the target before chaining: a private host never reaches the upstream proxy', async () => {
    const server = await origin();
    const chain = await upstream(server.port);
    const proxy = await start({ lookup: resolver({ 'internal.test': [PRIVATE_IP] }), env: { HTTPS_PROXY: `http://127.0.0.1:${chain.port}` } });
    expect((await connectVia(proxy, 'internal.test:443')).status).toBe(FORBIDDEN);
    expect((await connectVia(proxy, `127.0.0.1:${server.port}`)).status).toBe(FORBIDDEN);
    expect(chain.connects).toEqual([]);
  });

  it('connects directly to a host on NO_PROXY and to a private host the agent opened', async () => {
    const server = await origin();
    const chain = await upstream(server.port);
    const { connect, dialed } = redirectTo(server.port);
    const env = { HTTPS_PROXY: `http://127.0.0.1:${chain.port}`, NO_PROXY: 'skip.test, .corp.test:443' };
    const proxy = await start({ lookup: resolver({ 'skip.test': [PUBLIC_IP], 'a.corp.test': [PUBLIC_IP], 'b.corp.test': [PUBLIC_IP] }), connect, env });
    expect((await connectVia(proxy, 'skip.test:443')).status).toBe(ESTABLISHED);
    expect((await connectVia(proxy, 'a.corp.test:443')).status).toBe(ESTABLISHED);
    expect(chain.connects).toEqual([]);
    expect((await connectVia(proxy, 'b.corp.test:8443')).status).toBe(ESTABLISHED);
    expect(chain.connects).toHaveLength(1);
    proxy.allow(new URL(`http://127.0.0.1:${server.port}`));
    expect((await connectVia(proxy, `127.0.0.1:${server.port}`)).status).toBe(ESTABLISHED);
    expect(chain.connects).toHaveLength(1);
    expect(dialed).toEqual([`${PUBLIC_IP}:443`, `${PUBLIC_IP}:443`, `127.0.0.1:${chain.port}`, `127.0.0.1:${server.port}`]);
  });

  it('forwards plain HTTP to HTTP_PROXY as an absolute URL', async () => {
    const chain = await upstream(1);
    const proxy = await start({ lookup: resolver({ 'example.test': [PUBLIC_IP] }), env: { HTTP_PROXY: `127.0.0.1:${chain.port}` } });
    expect(await getVia(proxy, 'http://example.test/page?x=1')).toContain('via upstream');
    expect(chain.forwarded).toEqual([{ url: 'http://example.test/page?x=1', auth: undefined }]);
  });

  it('answers 502 when the upstream proxy refuses the tunnel', async () => {
    const chain = await upstream(1, { refuse: true });
    const proxy = await start({ lookup: resolver({ 'example.test': [PUBLIC_IP] }), env: { HTTPS_PROXY: `http://127.0.0.1:${chain.port}` } });
    const { status, head } = await connectVia(proxy, 'example.test:443');
    expect(status).toBe('HTTP/1.1 502 Bad Gateway');
    expect(head).toContain('HTTP 407');
  });

  it('refuses to start behind a proxy it cannot chain to, instead of going direct', async () => {
    await expect(startFilteringProxy({ env: { HTTPS_PROXY: 'socks5://127.0.0.1:1080' } })).rejects.toThrow(/HTTPS_PROXY must be an http:\/\/ proxy/);
    await expect(startFilteringProxy({ env: { HTTP_PROXY: 'http://' } })).rejects.toThrow(/HTTP_PROXY is not a proxy URL/);
  });
});
