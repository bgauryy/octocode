import http from 'node:http';
import net from 'node:net';
import { vi } from 'vitest';
import { addressOf } from './doubles.js';
import { keyOf } from '../src/browser/network.js';
import { startFilteringProxy, type FilteringProxy } from '../src/browser/proxy.js';

/** The filtering proxy (`src/browser/proxy.ts`) against local servers and raw sockets. */
export const PUBLIC_IP = '93.184.216.34';
export const PRIVATE_IP = '10.1.2.3';

/** What a test started (servers, sockets, proxies, folders), undone by `cleanUp` after it. */
export const cleanups: Array<() => Promise<void> | void> = [];

export async function cleanUp(): Promise<void> {
  while (cleanups.length > 0) await cleanups.pop()!();
}

export const listen = (server: net.Server | http.Server): Promise<number> =>
  new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      cleanups.push(() => {
        if (server instanceof http.Server) server.closeAllConnections();
        server.close();
      });
      resolve(addressOf(server).port);
    });
  });

/** A server on 127.0.0.1 that records every connection and the requests it answers. */
export async function origin() {
  const requests: Array<{ url: string; host: string | undefined; method: string | undefined }> = [];
  const upgrades: string[] = [];
  let connections = 0;
  const server = http.createServer((request, response) => {
    requests.push({ url: request.url ?? '', host: request.headers.host, method: request.method });
    response.writeHead(200, { 'content-type': 'text/plain', 'x-origin': 'yes', connection: 'close' });
    response.end('hello from origin');
  });
  server.on('connection', () => (connections += 1));
  server.on('upgrade', (request, socket) => {
    upgrades.push(request.url ?? '');
    socket.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\nws-open');
  });
  const port = await listen(server);
  return { port, requests, upgrades, connections: () => connections };
}

/** Everything the socket sends until `done` accepts it or it closes. */
export function readUntil(socket: net.Socket, done: (text: string) => boolean): Promise<string> {
  return new Promise((resolve) => {
    let text = '';
    const finish = () => resolve(text);
    socket.on('data', (chunk) => {
      text += chunk.toString('latin1');
      if (done(text)) finish();
    });
    socket.once('close', finish);
    socket.once('error', finish);
  });
}

export const rawSocket = (port: number): Promise<net.Socket> =>
  new Promise((resolve, reject) => {
    const socket = net.connect({ host: '127.0.0.1', port }, () => resolve(socket));
    socket.once('error', reject);
    cleanups.push(() => void socket.destroy());
  });

/** Whether something still accepts connections on `port`. */
export const accepts = (port: number): Promise<boolean> =>
  new Promise((resolve) => {
    const socket = net.connect({ host: '127.0.0.1', port }, () => {
      socket.destroy();
      resolve(true);
    });
    socket.once('error', () => resolve(false));
  });

/** Sends `CONNECT authority` to the proxy and returns its status line and the open socket. */
export async function connectVia(proxy: FilteringProxy, authority: string) {
  const socket = await rawSocket(proxy.port);
  socket.write(`CONNECT ${authority} HTTP/1.1\r\nHost: ${authority}\r\n\r\n`);
  const head = await readUntil(socket, (text) => text.includes('\r\n\r\n'));
  return { socket, status: head.split('\r\n')[0], head };
}

/** One plain-HTTP request through the proxy, with the whole response. */
export async function getVia(proxy: FilteringProxy, url: string): Promise<string> {
  const socket = await rawSocket(proxy.port);
  socket.write(`GET ${url} HTTP/1.1\r\nHost: ${new URL(url).host}\r\nConnection: close\r\n\r\n`);
  return readUntil(socket, () => false);
}

export async function start(options: Parameters<typeof startFilteringProxy>[0] = {}) {
  // Stands in for the browser's `PagePolicy.approves`: `allow` is the agent opening a private host.
  const opened = new Set<string>();
  const proxy = await startFilteringProxy({ env: {}, approved: (hostname, port) => opened.has(`${hostname}:${port}`), ...options });
  cleanups.push(() => proxy.close());
  return Object.assign(proxy, { allow: (url: URL): void => void opened.add(keyOf(url)) });
}

/** Names for the stub resolver. */
export const resolver = (names: Record<string, string[]>) => vi.fn(async (name: string) => (names[name] ?? []).map((address) => ({ address })));
/** A connect that lands every connection to PUBLIC_IP on `port` (nothing public is reachable here) and records each dial. */
export const redirectTo = (port: number) => {
  const dialed: string[] = [];
  const connect = (address: string, to: number) => {
    dialed.push(`${address}:${to}`);
    return net.connect({ host: '127.0.0.1', port: address === PUBLIC_IP ? port : to });
  };
  return { connect, dialed };
};

export const FORBIDDEN = 'HTTP/1.1 403 Forbidden';
export const ESTABLISHED = 'HTTP/1.1 200 Connection Established';
