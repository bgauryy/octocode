// The loopback CDP proxy: many clients share one browser connection. Every method, event and session id is
// forwarded unchanged; only the top-level message id is remapped so two clients' ids never collide.
import { createHash, timingSafeEqual } from 'node:crypto';
import { closeSync, openSync, writeSync } from 'node:fs';
import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from 'node:http';
import type { Duplex } from 'node:stream';
import { acceptWebSocket, type Peer } from './cdp-proxy-ws.mjs';

/** The only addresses the proxy ever binds. */
export const LOOPBACK_ADDRESSES = ['127.0.0.1', '::1'] as const;
/** The CDP message fields the proxy reads; everything else passes through untouched. */
type CdpMessage = {
  id?: unknown;
  method?: unknown;
  sessionId?: unknown;
  error?: unknown;
  result?: { sessionId?: unknown };
  params?: { sessionId?: unknown };
  [key: string]: unknown;
};

const LOOPBACK_NAMES = new Set(['127.0.0.1', 'localhost', '[::1]', '::1']);

export type ProxyStats = {
  clients: number;
  totalClients: number;
  /** Client calls forwarded to the browser. */
  messages: number;
  /** Browser events delivered to clients. */
  events: number;
};
export type Proxy = {
  listenPort: number;
  addresses: string[];
  stats(): ProxyStats;
  close(): Promise<void>;
};
export type ProxyOptions = {
  browserUrl: string;
  token: string;
  /** 0 picks a free port. */
  listenPort: number;
  /** JSONL method log; opened 0600. Never receives params or results. */
  logFile?: string;
  /** Called once when the browser connection drops. */
  onBrowserClose?: () => void;
  onStats?: () => void;
};

type Client = {
  id: number;
  peer: Peer;
  pending: Set<number>;
};
type Route = {
  client: Client;
  id: number;
  method: string;
  sessionId?: string;
  start: number;
};

const digest = (text: string) => createHash('sha256').update(text).digest();
/** Constant-time token check on the request path. */
export const tokenMatches = (path: string, token: string) =>
  timingSafeEqual(digest(path.split('?')[0]), digest(`/${token}`));
/** A browser page can open a WebSocket to loopback; only a missing or loopback Origin passes. */
export function originAllowed(origin: string | undefined) {
  if (origin === undefined) return true;
  try {
    return LOOPBACK_NAMES.has(new URL(origin).hostname);
  } catch {
    return false;
  }
}
/** DNS rebinding guard: the Host header must name loopback. */
export function hostAllowed(host: string | undefined) {
  if (host === undefined) return false;
  const name = host.startsWith('[')
    ? host.slice(0, host.indexOf(']') + 1)
    : host.split(':')[0];
  return LOOPBACK_NAMES.has(name);
}

const forbid = (res: ServerResponse) => {
  res.writeHead(403, { 'content-type': 'text/plain', connection: 'close' });
  res.end('Forbidden');
};

function listen(server: Server, port: number, host: string) {
  return new Promise<number>((resolve, reject) => {
    server.once('error', reject);
    server.listen({ port, host, exclusive: true }, () => {
      server.off('error', reject);
      resolve((server.address() as { port: number }).port);
    });
  });
}

/** Connects to the browser first, then listens; rejects when either fails. */
export async function startProxy(options: ProxyOptions): Promise<Proxy> {
  const browser = new WebSocket(options.browserUrl);
  await new Promise<void>((resolve, reject) => {
    browser.addEventListener('open', () => resolve(), { once: true });
    browser.addEventListener(
      'error',
      () => reject(new Error('Could not open the browser WebSocket')),
      { once: true }
    );
  });
  const logFd =
    options.logFile === undefined
      ? undefined
      : openSync(options.logFile, 'a', 0o600);
  const clients = new Map<number, Client>();
  const routes = new Map<number, Route>();
  // sessionId -> clients that use it; a session event reaches only them.
  const owners = new Map<string, Set<Client>>();
  const autoAttachers = new Set<Client>();
  const stats: ProxyStats = {
    clients: 0,
    totalClients: 0,
    messages: 0,
    events: 0,
  };
  let nextClient = 1,
    nextId = 1,
    closing = false;

  const record = (route: Route, ok: boolean) => {
    if (logFd === undefined) return;
    try {
      writeSync(
        logFd,
        `${JSON.stringify({
          ts: new Date().toISOString(),
          client: route.client.id,
          method: route.method,
          sessionId: route.sessionId,
          ok,
          durationMs: Date.now() - route.start,
        })}\n`
      );
    } catch {
      // Best-effort access log: a failed log write must not affect proxying.
    }
  };
  const own = (sessionId: string, client: Client) => {
    let set = owners.get(sessionId);
    if (!set) owners.set(sessionId, (set = new Set()));
    set.add(client);
  };
  const deliver = (text: string, targets: Iterable<Client>) => {
    for (const client of targets) {
      client.peer.send(text);
      stats.events++;
    }
  };

  browser.addEventListener('message', event => {
    const text = typeof event.data === 'string' ? event.data : null;
    if (text === null) return;
    let message: CdpMessage;
    try {
      message = JSON.parse(text);
    } catch {
      return;
    }
    if (typeof message.id === 'number') {
      const route = routes.get(message.id);
      if (!route) return;
      routes.delete(message.id);
      route.client.pending.delete(message.id);
      const failed = message.error !== undefined;
      record(route, !failed);
      if (
        !failed &&
        route.method === 'Target.attachToTarget' &&
        typeof message.result?.sessionId === 'string'
      )
        own(message.result.sessionId, route.client);
      message.id = route.id;
      route.client.peer.send(JSON.stringify(message));
      return;
    }
    const sessionId: string | undefined =
      typeof message.sessionId === 'string' ? message.sessionId : undefined;
    let targets: Iterable<Client> = clients.values();
    if (sessionId !== undefined && owners.has(sessionId))
      targets = owners.get(sessionId)!;
    if (message.method === 'Target.attachedToTarget') {
      const child = message.params?.sessionId;
      if (typeof child === 'string') {
        const parents =
          sessionId !== undefined ? owners.get(sessionId) : undefined;
        const heirs = parents?.size
          ? parents
          : autoAttachers.size
            ? autoAttachers
            : clients.values();
        for (const client of heirs) own(child, client);
      }
    }
    deliver(text, targets);
    if (
      message.method === 'Target.detachedFromTarget' &&
      typeof message.params?.sessionId === 'string'
    )
      owners.delete(message.params.sessionId);
  });
  const browserGone = () => {
    if (closing) return;
    options.onBrowserClose?.();
  };
  browser.addEventListener('close', browserGone);
  browser.addEventListener('error', browserGone);

  const refuse = (req: IncomingMessage) =>
    !tokenMatches(req.url ?? '/', options.token) ||
    !originAllowed(req.headers.origin) ||
    !hostAllowed(req.headers.host);
  const handleUpgrade = (
    req: IncomingMessage,
    socket: Duplex,
    head: Buffer
  ) => {
    if (refuse(req) || browser.readyState !== WebSocket.OPEN) {
      socket.end(
        'HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'
      );
      return;
    }
    const client: Client = {
      id: nextClient++,
      peer: undefined as unknown as Peer,
      pending: new Set(),
    };
    const peer = acceptWebSocket(req, socket, head, {
      message(text) {
        let message: CdpMessage;
        try {
          message = JSON.parse(text);
        } catch {
          client.peer.send(
            JSON.stringify({
              error: { code: -32700, message: 'Parse error' },
            })
          );
          return;
        }
        if (typeof message?.id !== 'number') {
          client.peer.send(
            JSON.stringify({
              error: {
                code: -32600,
                message: 'Invalid request: numeric id required',
              },
            })
          );
          return;
        }
        const proxyId = nextId++;
        const sessionId =
          typeof message.sessionId === 'string' ? message.sessionId : undefined;
        if (sessionId !== undefined) own(sessionId, client);
        if (
          message.method === 'Target.setAutoAttach' &&
          sessionId === undefined
        )
          autoAttachers.add(client);
        routes.set(proxyId, {
          client,
          id: message.id,
          method: String(message.method),
          sessionId,
          start: Date.now(),
        });
        client.pending.add(proxyId);
        stats.messages++;
        browser.send(JSON.stringify({ ...message, id: proxyId }));
      },
      close() {
        // The browser connection stays; only this client's routing state goes.
        clients.delete(client.id);
        stats.clients = clients.size;
        autoAttachers.delete(client);
        for (const id of client.pending) routes.delete(id);
        for (const [session, set] of owners) {
          set.delete(client);
          if (!set.size) owners.delete(session);
        }
        options.onStats?.();
      },
    });
    if (!peer) return;
    client.peer = peer;
    clients.set(client.id, client);
    stats.clients = clients.size;
    stats.totalClients++;
    options.onStats?.();
  };

  const servers: Server[] = [];
  const addresses: string[] = [];
  const make = () => {
    const server = createServer((_req, res) => forbid(res));
    server.on('upgrade', handleUpgrade);
    server.on('clientError', (_e, socket) => socket.destroy());
    return server;
  };
  const first = make();
  let listenPort: number;
  try {
    listenPort = await listen(first, options.listenPort, LOOPBACK_ADDRESSES[0]);
  } catch (error) {
    browser.close();
    if (logFd !== undefined) closeSync(logFd);
    const code = (error as NodeJS.ErrnoException).code;
    throw new Error(
      code === 'EADDRINUSE'
        ? `Port ${options.listenPort} is already in use; pick another --listen-port or omit it`
        : `Could not listen on 127.0.0.1: ${(error as Error).message}`,
      { cause: error }
    );
  }
  servers.push(first);
  addresses.push(LOOPBACK_ADDRESSES[0]);
  const second = make();
  try {
    await listen(second, listenPort, LOOPBACK_ADDRESSES[1]);
    servers.push(second);
    addresses.push(LOOPBACK_ADDRESSES[1]);
  } catch {
    // No IPv6 loopback, or its port is taken by another program: IPv4 loopback alone still serves.
  }

  return {
    listenPort,
    addresses,
    stats: () => ({ ...stats }),
    async close() {
      closing = true;
      for (const client of clients.values()) client.peer.close(1001);
      for (const server of servers) {
        server.closeAllConnections();
        await new Promise<void>(resolve => server.close(() => resolve()));
      }
      try {
        browser.close();
      } catch {
        // Already closed; shutdown continues regardless.
      }
      if (logFd !== undefined) closeSync(logFd);
    },
  };
}
