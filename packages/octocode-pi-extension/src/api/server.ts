import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import type { EventLog } from './events.js';
import { ERR, failure, isResponse, MAX_FRAME_BYTES, parseRequest, RpcError, success, type ApiEvent, type RpcRequest, type RpcResponse } from './protocol.js';
import { errorMessage } from '../shared/util.js';

const MAX_CONNECTIONS = 16;
/** A client that lets this much output queue up is not reading; drop it instead of buffering without limit. */
const MAX_QUEUED_BYTES = 4 * 1_048_576;
const LOOPBACK_HOSTS = new Set(['127.0.0.1', 'localhost', '[::1]']);

/** What a method handler may do for the connection it was called on. */
export interface Connection {
  /** Start streaming events to this connection. Socket connections get notifications; HTTP clients use GET /events. */
  subscribe?(options: { since?: number; types?: string[] }): { seq: number; gap: boolean };
  unsubscribe?(): void;
}

type Dispatch = (method: string, params: Record<string, unknown>, connection: Connection) => Promise<unknown>;

async function answer(request: RpcRequest, dispatch: Dispatch, connection: Connection): Promise<RpcResponse | undefined> {
  const id = request.id ?? null;
  try {
    const result = await dispatch(request.method, request.params ?? {}, connection);
    return request.id === undefined ? undefined : success(id, result);
  } catch (error) {
    if (request.id === undefined) return undefined;
    if (error instanceof RpcError) return failure(id, error.code, error.message, error.data);
    return failure(id, ERR.internal, errorMessage(error));
  }
}

export class ApiServer {
  private unix: net.Server | undefined;
  private web: http.Server | undefined;
  private readonly sockets = new Set<net.Socket>();
  private readonly streams = new Set<http.ServerResponse>();
  socketFile: string | undefined;
  httpUrl: string | undefined;

  constructor(
    private readonly dispatch: Dispatch,
    private readonly log: EventLog,
    private readonly token: string,
  ) {}

  /** Listen on a Unix socket: owner-only file permissions are the authentication. */
  listenUnix(file: string): Promise<void> {
    fs.rmSync(file, { force: true });
    const server = net.createServer((socket) => this.accept(socket));
    this.unix = server;
    return new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(file, () => {
        if (process.platform !== 'win32') fs.chmodSync(file, 0o600);
        this.socketFile = file;
        server.off('error', reject);
        resolve();
      });
    });
  }

  /** Listen on loopback only. Every request needs the bearer token; browsers (requests with an Origin) are refused. */
  listenHttp(port: number): Promise<void> {
    // A client that resets mid-body rejects the body read; an unhandled rejection would end Pi.
    const server = http.createServer((req, res) => void this.request(req, res).catch(() => res.destroy()));
    this.web = server;
    return new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(port, '127.0.0.1', () => {
        server.off('error', reject);
        this.httpUrl = `http://127.0.0.1:${(server.address() as net.AddressInfo).port}`;
        resolve();
      });
    });
  }

  private accept(socket: net.Socket): void {
    if (this.sockets.size >= MAX_CONNECTIONS) {
      socket.end(`${JSON.stringify(failure(null, ERR.unavailable, 'Too many connections'))}\n`);
      return;
    }
    this.sockets.add(socket);
    let subscription: { close(): void } | undefined;
    const write = (value: unknown): void => {
      if (socket.destroyed) return;
      if (socket.writableLength > MAX_QUEUED_BYTES) {
        socket.destroy();
        return;
      }
      socket.write(`${JSON.stringify(value)}\n`);
    };
    const connection: Connection = {
      subscribe: (options) => {
        subscription?.close();
        const sub = this.log.subscribe((event) => write({ jsonrpc: '2.0', method: 'event', params: event }), options);
        subscription = sub;
        return { seq: this.log.seq, gap: sub.gap };
      },
      unsubscribe: () => {
        subscription?.close();
        subscription = undefined;
      },
    };
    let pending = '';
    socket.setEncoding('utf8');
    socket.on('data', (chunk: string) => {
      pending += chunk;
      if (pending.length > MAX_FRAME_BYTES && !pending.includes('\n')) {
        write(failure(null, ERR.invalidRequest, 'Frame too large'));
        socket.destroy();
        return;
      }
      let newline: number;
      while ((newline = pending.indexOf('\n')) >= 0) {
        const line = pending.slice(0, newline).trim();
        pending = pending.slice(newline + 1);
        if (!line) continue;
        const parsed = parseRequest(line);
        if (isResponse(parsed)) write(parsed);
        else void answer(parsed, this.dispatch, connection).then((response) => response && write(response));
      }
    });
    const done = (): void => {
      subscription?.close();
      this.sockets.delete(socket);
    };
    socket.on('close', done);
    socket.on('error', done);
  }

  private authorized(req: http.IncomingMessage): boolean {
    const host = (req.headers.host ?? '').replace(/:\d+$/, '');
    if (!LOOPBACK_HOSTS.has(host) || req.headers.origin !== undefined) return false;
    const supplied = Buffer.from((req.headers.authorization ?? '').replace(/^Bearer /i, ''));
    const expected = Buffer.from(this.token);
    return supplied.length === expected.length && crypto.timingSafeEqual(supplied, expected);
  }

  private async request(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    const reply = (status: number, body: unknown): void => {
      res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      res.end(JSON.stringify(body));
    };
    if (!this.authorized(req)) return reply(401, failure(null, ERR.invalidRequest, 'Unauthorized'));
    const url = new URL(req.url ?? '/', 'http://localhost');
    if (req.method === 'GET' && url.pathname === '/events') return this.stream(req, res, url);
    if (req.method === 'GET' && url.pathname === '/health') return reply(200, { ok: true });
    if (req.method !== 'POST' || url.pathname !== '/rpc') return reply(404, failure(null, ERR.methodNotFound, 'Use POST /rpc, GET /events or GET /health'));
    let body = '';
    for await (const chunk of req as AsyncIterable<Buffer>) {
      body += chunk.toString('utf8');
      if (body.length > MAX_FRAME_BYTES) return reply(413, failure(null, ERR.invalidRequest, 'Body too large'));
    }
    const parsed = parseRequest(body);
    if (isResponse(parsed)) return reply(400, parsed);
    // Event streaming over HTTP is GET /events; a subscription needs a connection that outlives the request.
    const response = await answer(parsed, this.dispatch, {});
    if (response) reply(200, response);
    else {
      res.writeHead(204);
      res.end();
    }
  }

  private stream(req: http.IncomingMessage, res: http.ServerResponse, url: URL): void {
    if (this.streams.size >= MAX_CONNECTIONS) {
      res.writeHead(503).end();
      return;
    }
    // Replay only what the client asked for: a plain GET /events starts with the next event.
    const raw = req.headers['last-event-id'] ?? url.searchParams.get('since') ?? '';
    const last = String(raw).trim() === '' ? Number.NaN : Number(raw);
    const types = url.searchParams.get('types')?.split(',').filter(Boolean);
    res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store', connection: 'keep-alive' });
    // Send the headers now: a client must see the stream open before the first event (or keep-alive) arrives.
    res.flushHeaders();
    const send = (event: ApiEvent): void => {
      if (res.writableLength > MAX_QUEUED_BYTES) res.destroy();
      else res.write(`id: ${event.seq}\nevent: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
    };
    this.streams.add(res);
    const sub = this.log.subscribe(send, { ...(Number.isInteger(last) && last >= 0 ? { since: last } : {}), ...(types ? { types } : {}) });
    if (sub.gap) res.write(': gap\n\n');
    const beat = setInterval(() => res.write(': keep-alive\n\n'), 15_000);
    beat.unref();
    res.on('close', () => {
      clearInterval(beat);
      sub.close();
      this.streams.delete(res);
    });
  }

  async close(): Promise<void> {
    for (const socket of this.sockets) socket.destroy();
    for (const stream of this.streams) stream.destroy();
    const closing: Promise<void>[] = [];
    for (const server of [this.unix, this.web]) if (server?.listening) closing.push(new Promise((resolve) => server.close(() => resolve())));
    this.web?.closeAllConnections();
    await Promise.all(closing);
    if (this.socketFile) fs.rmSync(this.socketFile, { force: true });
  }
}
