import { createHash } from 'node:crypto';
import http from 'node:http';
import type { AddressInfo, Socket } from 'node:net';

/**
 * A fake Chrome DevTools endpoint: `/json/version`, `/json/list`, `/json/new`, `/json/close/<id>` over HTTP and one page WebSocket
 * per target (RFC 6455 framing by hand, text frames only). `handle` answers each CDP command; `emit` pushes events.
 */
export type CdpHandler = (method: string, params: Record<string, unknown>, page: StubPage) => unknown;

export interface StubPage {
  id: string;
  emit(method: string, params?: Record<string, unknown>): void;
  /** Send a raw text frame (for malformed-message tests). */
  raw(text: string): void;
  /** Drop the connection without a close handshake. */
  drop(): void;
}

export interface CdpStubOptions {
  handle?: CdpHandler;
  /** Targets `/json/list` reports; default one page `page-1`. */
  targets?: Array<{ id: string; type: string; ws?: boolean }>;
  /** HTTP status for `PUT /json/new` (default 200). */
  newTabStatus?: number;
}

export interface CdpStub {
  port: number;
  /** Every CDP command received, in order. */
  commands: Array<{ target: string; method: string; params: Record<string, unknown> }>;
  /** HTTP requests as `METHOD path`. */
  requests: string[];
  pages: Map<string, StubPage>;
  /** Adds a page target, as a popup the page opened would. */
  addTarget(id: string): void;
  close(): Promise<void>;
}

function frame(opcode: number, payload: Buffer): Buffer {
  const length = payload.length;
  const head = length < 126 ? Buffer.from([0x80 | opcode, length]) : length < 65_536 ? Buffer.alloc(4) : Buffer.alloc(10);
  if (length >= 126 && length < 65_536) {
    head[0] = 0x80 | opcode;
    head[1] = 126;
    head.writeUInt16BE(length, 2);
  } else if (length >= 65_536) {
    head[0] = 0x80 | opcode;
    head[1] = 127;
    head.writeBigUInt64BE(BigInt(length), 2);
  }
  return Buffer.concat([head, payload]);
}

/** Parse complete (masked, client-to-server) frames off the front of `buffer`. */
function readFrames(buffer: Buffer): { frames: Array<{ opcode: number; payload: Buffer }>; rest: Buffer } {
  const frames: Array<{ opcode: number; payload: Buffer }> = [];
  let offset = 0;
  for (;;) {
    if (buffer.length - offset < 2) break;
    const opcode = buffer[offset]! & 0x0f;
    const masked = (buffer[offset + 1]! & 0x80) !== 0;
    let length = buffer[offset + 1]! & 0x7f;
    let cursor = offset + 2;
    if (length === 126) {
      if (buffer.length < cursor + 2) break;
      length = buffer.readUInt16BE(cursor);
      cursor += 2;
    } else if (length === 127) {
      if (buffer.length < cursor + 8) break;
      length = Number(buffer.readBigUInt64BE(cursor));
      cursor += 8;
    }
    const mask = masked ? buffer.subarray(cursor, cursor + 4) : undefined;
    if (masked) cursor += 4;
    if (buffer.length < cursor + length) break;
    const payload = Buffer.from(buffer.subarray(cursor, cursor + length));
    if (mask) for (let i = 0; i < payload.length; i += 1) payload[i] = payload[i]! ^ mask[i % 4]!;
    frames.push({ opcode, payload });
    offset = cursor + length;
  }
  return { frames, rest: buffer.subarray(offset) };
}

export async function startCdpStub(options: CdpStubOptions = {}): Promise<CdpStub> {
  const commands: CdpStub['commands'] = [];
  const requests: string[] = [];
  const pages = new Map<string, StubPage>();
  const sockets = new Set<Socket>();
  let targets = options.targets ?? [{ id: 'page-1', type: 'page' }];
  let tabs = 0;
  const server = http.createServer((req, res) => {
    const url = req.url ?? '/';
    requests.push(`${req.method} ${url}`);
    const wsUrl = (id: string) => `ws://127.0.0.1:${(server.address() as AddressInfo).port}/devtools/page/${id}`;
    const json = (status: number, body: unknown) => {
      res.writeHead(status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(body));
    };
    if (url === '/json/version') return json(200, { Browser: 'Stub/1.0' });
    if (url === '/json/list') return json(200, targets.map((target) => ({ id: target.id, type: target.type, ...(target.ws === false ? {} : { webSocketDebuggerUrl: wsUrl(target.id) }) })));
    if (url.startsWith('/json/new')) {
      if (req.method !== 'PUT') return json(405, {});
      if ((options.newTabStatus ?? 200) !== 200) return json(options.newTabStatus!, {});
      tabs += 1;
      const id = `tab-${tabs}`;
      targets = [...targets, { id, type: 'page' }];
      return json(200, { id, type: 'page', webSocketDebuggerUrl: wsUrl(id) });
    }
    if (url.startsWith('/json/close/')) {
      targets = targets.filter((target) => target.id !== url.slice('/json/close/'.length));
      res.end('Target is closing');
      return;
    }
    json(404, {});
  });
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });
  server.on('upgrade', (req, socket: Socket) => {
    const id = /^\/devtools\/page\/(.+)$/.exec(req.url ?? '')?.[1];
    const key = req.headers['sec-websocket-key'];
    if (!id || typeof key !== 'string') {
      socket.end('HTTP/1.1 404 Not Found\r\n\r\n');
      return;
    }
    const accept = createHash('sha1').update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    const send = (text: string) => socket.writable && socket.write(frame(1, Buffer.from(text)));
    const page: StubPage = {
      id,
      emit: (method, params = {}) => send(JSON.stringify({ method, params })),
      raw: send,
      drop: () => socket.destroy(),
    };
    pages.set(id, page);
    let buffer: Buffer = Buffer.alloc(0);
    socket.on('data', (chunk: Buffer) => {
      const parsed = readFrames(Buffer.concat([buffer, chunk]));
      buffer = parsed.rest;
      for (const { opcode, payload } of parsed.frames) {
        if (opcode === 8) {
          if (socket.writable) socket.end(frame(8, payload.subarray(0, 2)));
          return;
        }
        if (opcode === 9) {
          socket.write(frame(10, payload));
          continue;
        }
        if (opcode !== 1) continue;
        const message = JSON.parse(payload.toString('utf8')) as { id: number; method: string; params?: Record<string, unknown> };
        const params = message.params ?? {};
        commands.push({ target: id, method: message.method, params });
        void Promise.resolve()
          .then(() => (options.handle ? options.handle(message.method, params, page) : {}))
          .then(
            (result) => {
              if (result === NO_REPLY) return;
              send(JSON.stringify(isError(result) ? { id: message.id, error: result.error } : { id: message.id, result: result ?? {} }));
            },
            (error: unknown) => send(JSON.stringify({ id: message.id, error: { message: error instanceof Error ? error.message : String(error) } })),
          );
      }
    });
    socket.on('error', () => undefined);
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    port: (server.address() as AddressInfo).port,
    commands,
    requests,
    pages,
    addTarget: (id: string) => {
      targets = [{ id, type: 'page' }, ...targets];
    },
    close: () =>
      new Promise((resolve) => {
        for (const socket of sockets) socket.destroy();
        server.close(() => resolve());
      }),
  };
}

/** Return this from a handler to leave the command unanswered. */
export const NO_REPLY = Symbol('no reply');

function isError(value: unknown): value is { error: { message: string } } {
  return typeof value === 'object' && value !== null && 'error' in value;
}
