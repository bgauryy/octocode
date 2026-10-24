import { createHash, randomUUID } from 'node:crypto';
import http from 'node:http';
import type { AddressInfo, Socket } from 'node:net';

/**
 * A fake Chrome DevTools endpoint: `/json/version`, `/json/list`, `/json/new`, `/json/close/<id>` over HTTP and the
 * browser-level WebSocket `/json/version` names (RFC 6455 framing by hand, text frames only). `Target.attachToTarget
 * { flatten: true }` opens a session on a target; every command and event of it carries its `sessionId`. `autoAttach` makes
 * the browser attach a session by itself (a popup, a worker), held until `Runtime.runIfWaitingForDebugger`. `handle` answers
 * each CDP command; `emit` pushes events.
 */
export type CdpHandler = (method: string, params: Record<string, unknown>, page: StubPage) => unknown;

export interface StubPage {
  /** The target id of a session; `browser` for the browser itself (commands sent without a `sessionId`). */
  id: string;
  /** Pushes an event on this session (for the browser: an event with no `sessionId`). */
  emit(method: string, params?: Record<string, unknown>): void;
  /** Whether the session is attached: false once the client detached it or the target went away. Always true for the browser. */
  readonly attached: boolean;
  /** Whether the target runs: false for an auto-attached one held for the debugger until it gets `Runtime.runIfWaitingForDebugger`. */
  running: boolean;
  /** Send a raw text frame (for malformed-message tests). */
  raw(text: string): void;
  /** A session: the target went away (Chrome sends `Target.detachedFromTarget` and answers nothing more on it). The browser: drops the connection without a close handshake. */
  drop(): void;
}

/** A target the browser attaches a session to by itself. */
interface AutoAttachSpec {
  id: string;
  type: string;
  url?: string;
  openerId?: string;
  /** The session the attach is announced on (a target id), as for a worker of that page; default the browser. */
  parent?: string;
  /** Held for the debugger (default true). */
  waiting?: boolean;
}

export interface CdpStubOptions {
  handle?: CdpHandler;
  /** Targets `/json/list` reports; default one page `page-1`. */
  targets?: Array<{ id: string; type: string; ws?: boolean; url?: string; title?: string }>;
  /** HTTP status for `PUT /json/new` (default 200). */
  newTabStatus?: number;
}

export interface CdpStub {
  port: number;
  /** The browser's DevTools path `/json/version` reports (`/devtools/browser/<id>`), unique per stub like per Chrome run. */
  browserPath: string;
  /** Every CDP command received, in order: a session's under its target id, the browser's under `browser` (attaching and detaching sessions is not listed). */
  commands: Array<{ target: string; method: string; params: Record<string, unknown> }>;
  /** HTTP requests as `METHOD path`. */
  requests: string[];
  /** The session opened on each target, by target id (kept after it detaches). */
  pages: Map<string, StubPage>;
  /** The browser-level connection the last client opened. */
  browser: StubPage | undefined;
  /** Adds a page target, as a popup the page opened would. */
  addTarget(id: string): void;
  /**
   * Attaches a session to a new target by itself, as `Target.setAutoAttach` does: `Target.attachedToTarget` goes out on the
   * parent session (or the browser). A page is also listed by `/json/list`. Throws when no client is connected.
   */
  autoAttach(spec: AutoAttachSpec): StubPage;
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
  let sessionCount = 0;
  /** Detaches the live session on a target, as Chrome does when the target closes. */
  const detachers = new Map<string, () => void>();
  const state: { browser: StubPage | undefined; autoAttach: ((spec: AutoAttachSpec) => StubPage) | undefined } = { browser: undefined, autoAttach: undefined };
  const browserPath = `/devtools/browser/${randomUUID()}`;
  const server = http.createServer((req, res) => {
    const url = req.url ?? '/';
    requests.push(`${req.method} ${url}`);
    const wsUrl = (id: string) => `ws://127.0.0.1:${(server.address() as AddressInfo).port}/devtools/page/${id}`;
    const json = (status: number, body: unknown) => {
      res.writeHead(status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(body));
    };
    if (url === '/json/version') return json(200, { Browser: 'Stub/1.0', webSocketDebuggerUrl: `ws://127.0.0.1:${(server.address() as AddressInfo).port}${browserPath}` });
    if (url === '/json/list') return json(200, targets.map((target) => ({ id: target.id, type: target.type, ...(target.url ? { url: target.url } : {}), ...(target.title ? { title: target.title } : {}), ...(target.ws === false ? {} : { webSocketDebuggerUrl: wsUrl(target.id) }) })));
    if (url.startsWith('/json/new')) {
      if (req.method !== 'PUT') return json(405, {});
      if ((options.newTabStatus ?? 200) !== 200) return json(options.newTabStatus!, {});
      tabs += 1;
      const id = `tab-${tabs}`;
      targets = [...targets, { id, type: 'page' }];
      return json(200, { id, type: 'page', webSocketDebuggerUrl: wsUrl(id) });
    }
    if (url.startsWith('/json/close/')) {
      const closing = url.slice('/json/close/'.length);
      targets = targets.filter((target) => target.id !== closing);
      detachers.get(closing)?.();
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
    const key = req.headers['sec-websocket-key'];
    if (req.url !== browserPath || typeof key !== 'string') {
      socket.end('HTTP/1.1 404 Not Found\r\n\r\n');
      return;
    }
    const accept = createHash('sha1').update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    const send = (text: string) => socket.writable && socket.write(frame(1, Buffer.from(text)));
    const sessions = new Map<string, { page: StubPage; live: boolean }>();
    const browser: StubPage = {
      id: 'browser',
      attached: true,
      running: true,
      emit: (method, params = {}) => send(JSON.stringify({ method, params })),
      raw: send,
      drop: () => socket.destroy(),
    };
    state.browser = browser;
    const attach = (id: string, parent?: string, waiting = false): { sessionId: string; page: StubPage } => {
      const sessionId = `session-${++sessionCount}`;
      const entry = { live: true, page: undefined as unknown as StubPage };
      // A child target's departure is announced on its parent's session, the target's own on the browser's.
      const announce = (method: string, params: Record<string, unknown>) => {
        const parentSession = parent === undefined ? undefined : [...sessions].find(([, candidate]) => candidate.page.id === parent && candidate.live)?.[0];
        send(JSON.stringify({ method, params, ...(parentSession === undefined ? {} : { sessionId: parentSession }) }));
      };
      entry.page = {
        id,
        get attached() {
          return entry.live;
        },
        running: !waiting,
        emit: (method, params = {}) => entry.live && send(JSON.stringify({ method, params, sessionId })),
        raw: (text) => entry.live && send(text),
        drop: () => {
          if (!entry.live) return;
          entry.live = false;
          announce('Target.detachedFromTarget', { sessionId, targetId: id });
        },
      };
      sessions.set(sessionId, entry);
      pages.set(id, entry.page);
      detachers.set(id, entry.page.drop);
      return { sessionId, page: entry.page };
    };
    state.autoAttach = (spec) => {
      const { sessionId, page } = attach(spec.id, spec.parent, spec.waiting ?? true);
      if (spec.type === 'page') targets = [{ id: spec.id, type: 'page', ...(spec.url ? { url: spec.url } : {}) }, ...targets];
      const targetInfo = { targetId: spec.id, type: spec.type, title: '', url: spec.url ?? '', attached: true, ...(spec.openerId ? { openerId: spec.openerId } : {}) };
      const params = { sessionId, targetInfo, waitingForDebugger: spec.waiting ?? true };
      const parentSession = spec.parent === undefined ? undefined : [...sessions].find(([, candidate]) => candidate.page.id === spec.parent && candidate.live)?.[0];
      send(JSON.stringify({ method: 'Target.attachedToTarget', params, ...(parentSession === undefined ? {} : { sessionId: parentSession }) }));
      return page;
    };
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
        const message = JSON.parse(payload.toString('utf8')) as { id: number; method: string; params?: Record<string, unknown>; sessionId?: string };
        const params = message.params ?? {};
        const reply = (body: Record<string, unknown>) => send(JSON.stringify({ id: message.id, ...(message.sessionId === undefined ? {} : { sessionId: message.sessionId }), ...body }));
        if (message.sessionId === undefined && message.method === 'Target.attachToTarget') {
          const id = String(params['targetId']);
          if (!targets.some((target) => target.id === id) || params['flatten'] !== true) {
            reply({ error: { message: 'No target with given id found' } });
            continue;
          }
          // Chrome announces an attach the client asked for too (not held for the debugger), before the reply.
          const { sessionId } = attach(id);
          send(JSON.stringify({ method: 'Target.attachedToTarget', params: { sessionId, targetInfo: { targetId: id, type: 'page', title: '', url: '', attached: true }, waitingForDebugger: false } }));
          reply({ result: { sessionId } });
          continue;
        }
        if (message.sessionId === undefined && message.method === 'Target.detachFromTarget') {
          const entry = sessions.get(String(params['sessionId']));
          if (entry) entry.live = false;
          reply({ result: {} });
          continue;
        }
        const entry = message.sessionId === undefined ? undefined : sessions.get(message.sessionId);
        if (message.sessionId !== undefined && !entry?.live) {
          reply({ error: { message: 'Session with given id not found.' } });
          continue;
        }
        const page = entry?.page ?? browser;
        commands.push({ target: page.id, method: message.method, params });
        if (message.method === 'Runtime.runIfWaitingForDebugger') page.running = true;
        void Promise.resolve()
          .then(() => (options.handle ? options.handle(message.method, params, page) : {}))
          .then(
            (result) => {
              if (result === NO_REPLY) return;
              reply(isError(result) ? { error: result.error } : { result: result ?? {} });
            },
            (error: unknown) => reply({ error: { message: error instanceof Error ? error.message : String(error) } }),
          );
      }
    });
    socket.on('error', () => undefined);
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    port: (server.address() as AddressInfo).port,
    browserPath,
    commands,
    requests,
    pages,
    get browser() {
      return state.browser;
    },
    addTarget: (id: string) => {
      targets = [{ id, type: 'page' }, ...targets];
    },
    autoAttach: (spec) => {
      if (!state.autoAttach) throw new Error('No client is connected to the stub');
      return state.autoAttach(spec);
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

/** The params of a `Fetch.requestPaused` event for `url`. */
export const paused = (requestId: string, url: string, resourceType = 'XHR', extra: Record<string, unknown> = {}) => ({ requestId, request: { url }, resourceType, frameId: 'f', ...extra });

/** The methods of the commands the stub saw on one target. */
export const methodsOn = (cdp: CdpStub, target: string): string[] => cdp.commands.filter((command) => command.target === target).map((command) => command.method);

/** The request ids the stub saw `method` answer, on `target` when given. */
export const answered = (cdp: CdpStub, method: 'Fetch.failRequest' | 'Fetch.continueRequest', target?: string): string[] =>
  cdp.commands.filter((command) => command.method === method && (target === undefined || command.target === target)).map((command) => String(command.params['requestId']));

function isError(value: unknown): value is { error: { message: string } } {
  return typeof value === 'object' && value !== null && 'error' in value;
}
