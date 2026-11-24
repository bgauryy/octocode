// Lean local viewer server: 127.0.0.1 only, one random token per run, the viewer assets, and one WebSocket
// that replays every event so far and then streams new ones. The browser asks for AST outlines, LSP answers and
// chat with the agent over the same socket; those are events too, so a replay shows every tool call and message.
// `startServer` is the module API (commands/channel.ts runs it in-process); the CLI below keeps the script's flags.
import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from 'node:http';
import { open as openFile, type FileHandle } from 'node:fs/promises';
import type { AddressInfo, Socket } from 'node:net';
import type { Duplex } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { appendFileSync, readFileSync, rmSync } from 'node:fs';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { spawn } from 'node:child_process';
import { dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { eventStream } from './lib/event-stream.ts';
import { graphInput, runOptions } from './lib/run-options.ts';
import { beginRun, freshOut } from './lib/run-output.ts';
import { accept, type WsClient } from './ws.ts';
import {
  astOutline,
  defaults,
  logLine,
  lspQuery,
  readEvents,
  reportTools,
  runPipeline,
  type LoggedRunEvent,
  type LspQuestion,
  type Publish,
} from './pipeline.ts';
import type { RefreshOptions } from './refresh.ts';
import {
  createAnswers,
  fieldsOf,
  isRecord,
  logRoot,
  type Answers,
  type LogEmit,
  type ReadLive,
  type ViewerMessage,
  type WikiBody,
  HttpError,
} from './serve-answers.ts';
import type { RefreshedPayload, RunEvent, RunOptions } from './types.ts';
import { writeServerInfo } from './lib/served.ts';
import { errorMessage } from './lib.ts';
import { graphCommand } from './invocation.ts';

const usage =
  () => `usage: ${graphCommand()} serve <file-or-directory> [--out <dir>] [--whole] [--lsp smart|all|off] [--jobs N] [--ingest-jobs N] [--exclude a,b]
                 [--port N] [--no-open] [--exit-when-done]
       ${graphCommand()} serve --replay <result-dir> [--port N] [--no-open]
  Builds <root> chunk by chunk and streams every graph, LSP and AST page to the viewer as it arrives.
  Run it from the target workspace. Prints the viewer URL; Ctrl-C stops it (results stay in <out>).
  Chat from the viewer lands in <out>/chat-inbox.jsonl; answer with ${graphCommand()} reply (references/chat.md).
  Graph queries and the wiki: ${graphCommand()} reply query|wiki <out> ... (references/query.md).`;

const ASSETS = dirname(
  fileURLToPath(import.meta.resolve('#graph-assets/live.html'))
);
const TYPES: Record<string, string> = {
  '.txt': 'text/plain; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
};
// The viewer page, its one bundled script (every live-*.ts module), the renderer bundle and the stylesheets.
const STATIC = new Set([
  'renderer.generated.js',
  'renderer.generated.LICENSE.txt',
  'live.html',
  'live.js',
  'live.css',
  'live-chat.css',
  'live-query.css',
  'live-wiki.css',
]);
// A request body larger than this is refused whole (never cut): replies are text, not files.
const MAX_BODY = 4 * 1024 * 1024;

type Scalar = string | number | boolean | null;

/** The selection a viewer chat message carries: a repository-relative file, a line and the node's scalar metrics. */
export interface ChatSelection {
  file: string;
  line: number;
  metrics?: Record<string, Scalar>;
}

/** One browser chat message, as a transport receives it. */
export interface ChatMessage {
  id: string;
  text: string;
  context: ChatSelection | undefined;
  at: number;
}

/** Delivers one browser chat message: the inbox file by default, the channel's notification in-process. */
export type ChatTransport = (m: ChatMessage) => void | Promise<void>;

/** A reply (POST /reply, the channel's reply tool); each field is checked where it is read. */
export interface ReplyBody {
  text?: unknown;
  chat_id?: unknown;
  nodes?: unknown;
}

/** Rebuilds <out> after edits (refresh.ts `refreshRun`); embedders and tests pass their own. */
export type RefreshRun = (options: RefreshOptions) => Promise<unknown>;

export interface ServerOptions {
  root?: string | null;
  out?: string;
  opts?: RunOptions;
  replay?: boolean;
  port?: number;
  open?: boolean;
  transport?: 'inbox' | ChatTransport;
  refreshRun?: RefreshRun | null;
}

/** How the first build ended. */
export interface BuildResult {
  ok: boolean;
  error?: string;
}

/** A refresh's answer: its `refreshed` summary (rebuilt chunks; changed, added and removed files) and time. */
interface RefreshResult extends RefreshedPayload {
  ok: true;
}

type EventLog = ReturnType<typeof createLog>;
type Chat = ReturnType<typeof createChat>;
/** One socket op the viewer can ask for; `m` is its parsed message. */
type Op = (m: ViewerMessage, client: WsClient) => unknown;

/** The server's state; the services that read it (answers, chat, ops) join it in ServerContext. */
interface ServerCore {
  root: string | null;
  out: string;
  opts: RunOptions;
  token: string;
  L: EventLog;
  controller: AbortController;
  building: boolean;
  refreshing: Promise<RefreshResult> | null;
  queued: Promise<RefreshResult> | null;
  refreshAbort: AbortController | null;
  refreshRun: RefreshRun | null;
  port: number;
  readLive: ReadLive | null;
  publish: Publish;
}

interface ServerContext extends ServerCore {
  answers: Answers;
  chat: Chat;
  ops: Partial<Record<string, Op>>;
}

export function openBrowser(url: string) {
  const [cmd, args]: [string, string[]] =
    process.platform === 'darwin'
      ? ['open', [url]]
      : process.platform === 'win32'
        ? ['cmd', ['/c', 'start', '', url]]
        : ['xdg-open', [url]];
  spawn(cmd, args, { stdio: 'ignore', detached: true })
    .on('error', () => {})
    .unref();
}

// Chat messages and graph answers get ids (c12, q13): later events and replies refer to them.
const ID_PREFIX: Partial<Record<string, string>> = { chat: 'c', query: 'q' };

const sameToken = (given: unknown, token: string) =>
  typeof given === 'string' &&
  Buffer.byteLength(given) === Buffer.byteLength(token) &&
  timingSafeEqual(Buffer.from(given), Buffer.from(token));

// Every event gets a sequence number; the log is the replay source and events.jsonl its saved copy. A replay keeps
// its saved file as it was except for chat and refresh events, so sequence numbers can skip: the next one always
// follows the highest seen, and `since` filters by number, not position.
function createLog({ out, persistAll }: { out: string; persistAll: boolean }) {
  const log: LoggedRunEvent[] = [];
  const stream = eventStream<WsClient>(log);
  let nextSeq = 0;
  let t0 = Date.now();
  const emit: LogEmit = (event, save = persistAll || event.type === 'chat') => {
    const seq = nextSeq++;
    const prefix = ID_PREFIX[event.type];
    const e = {
      t: Date.now() - t0,
      ...(prefix && !event.id ? { id: `${prefix}${seq}` } : {}),
      ...event,
      seq,
    };
    log.push(e);
    if (save) appendFileSync(join(out, 'events.jsonl'), `${logLine(e)}\n`);
    stream.schedule();
    return e;
  };
  const load = (events: readonly RunEvent[]) => {
    for (const e of events) {
      const seq = e.seq ?? nextSeq;
      if (!Number.isSafeInteger(seq) || seq < nextSeq)
        throw new Error('Saved event sequence is out of order');
      // e.seq === seq: the saved event is numbered already.
      log.push(e.seq === seq ? (e as LoggedRunEvent) : { ...e, seq });
      nextSeq = seq + 1;
      t0 = Math.min(t0, Date.now() - (e.t ?? 0));
    }
  };
  return { log, ...stream, emit, load };
}

const scalar = (v: unknown): v is Scalar =>
  v === null || ['number', 'string', 'boolean'].includes(typeof v);

// Chat context from the page: the selected node's file, line and metrics; anything else is dropped.
function cleanContext(c: unknown): ChatSelection | undefined {
  if (!isRecord(c) || typeof c.file !== 'string' || !c.file) return undefined;
  const metrics = isRecord(c.metrics)
    ? Object.fromEntries(
        Object.entries(c.metrics).filter(
          (entry): entry is [string, Scalar] =>
            /^\w+$/.test(entry[0]) && scalar(entry[1])
        )
      )
    : undefined;
  return {
    file: c.file,
    line:
      typeof c.line === 'number' && Number.isInteger(c.line) && c.line > 0
        ? c.line
        : 1,
    ...(metrics ? { metrics } : {}),
  };
}

// The default transport: one JSON line per message in <out>/chat-inbox.jsonl, for an agent watching that file.
function inboxTransport(out: string): ChatTransport {
  const file = join(out, 'chat-inbox.jsonl');
  return m =>
    appendFileSync(
      file,
      `${JSON.stringify({ chat_id: m.id, text: m.text, ...m.context, at: new Date(m.at).toISOString() })}\n`
    );
}

// User messages and their delivery state (queued -> delivered -> replied, or failed); rebuilt from a replayed log.
function createChat(
  ctx: ServerCore,
  answers: Answers,
  transport: 'inbox' | ChatTransport
) {
  const deliver =
    typeof transport === 'function' ? transport : inboxTransport(ctx.out);
  const states = new Map<string, unknown>();
  for (const e of ctx.L.log)
    if (
      e.type === 'chat' &&
      typeof e.id === 'string' &&
      (e.role === 'user' || states.has(e.id))
    )
      states.set(e.id, e.state ?? states.get(e.id));
  const mark = (
    id: string,
    state: string,
    extra: Record<string, unknown> = {}
  ) => {
    states.set(id, state);
    ctx.L.emit({ type: 'chat', id, state, ...extra });
  };
  const system = (text: string) =>
    ctx.L.emit({ type: 'chat', role: 'system', text, at: Date.now() });
  // A reply names a message of this viewer, or none.
  const knownChat = (chatId: unknown) => {
    if (typeof chatId !== 'string' || !states.has(chatId))
      throw new HttpError(
        404,
        `no chat message ${String(chatId)} in this viewer`
      );
    return chatId;
  };
  async function fromBrowser(m: ViewerMessage) {
    const text = typeof m.text === 'string' ? m.text.trim() : '';
    if (!text) return;
    const e = ctx.L.emit({
      type: 'chat',
      role: 'user',
      text,
      context: cleanContext(m.context),
      state: 'queued',
      at: Date.now(),
    });
    // emit gives every chat event its id (c<seq>).
    const id = String(e.id);
    states.set(id, 'queued');
    try {
      await deliver({ id, text, context: e.context, at: e.at });
      if (states.get(id) === 'queued') mark(id, 'delivered');
    } catch (error) {
      mark(id, 'failed', { error: errorMessage(error) });
    }
  }
  // `nodes` (paths, suffixes or ids) resolve against the graph before anything is sent: a reply never points at a guess.
  async function reply({ text, chat_id: chatId, nodes }: ReplyBody = {}) {
    if (typeof text !== 'string' || !text.trim())
      throw new HttpError(400, 'text must be a non-empty string');
    const asked = chatId ? knownChat(chatId) : undefined;
    const ids = await answers.nodes(nodes);
    const target =
      asked || [...states].findLast(([, s]) => s !== 'replied')?.[0];
    const e = ctx.L.emit({
      type: 'chat',
      role: 'agent',
      text,
      ...(ids.length ? { nodes: ids } : {}),
      ...(target ? { replyTo: target } : {}),
      at: Date.now(),
    });
    if (target) mark(target, 'replied');
    return {
      ok: true,
      id: e.id,
      replyTo: target ?? null,
      ...(ids.length ? { nodes: ids } : {}),
    };
  }
  return { fromBrowser, reply, system };
}

// Rebuilds the graph after code changes through refresh.ts (`refreshRun`, which reads the run's options from
// <out>); its events are saved even in a replay. A failure is an `error` event and a system chat line.
async function runRefresh(ctx: ServerContext): Promise<RefreshResult> {
  const root = runRoot(ctx);
  if (!root) throw new HttpError(409, 'this viewer has no run root to refresh');
  const refreshRun =
    ctx.refreshRun ?? (await import('./refresh.ts')).refreshRun;
  const t = Date.now();
  ctx.refreshAbort = new AbortController();
  ctx.chat.system('Refreshing the graph from the files on disk…');
  let summary: RefreshedPayload | undefined;
  try {
    await refreshRun({
      root,
      out: ctx.out,
      emit: event => {
        if (event.type === 'refreshed')
          summary = event as RunEvent & RefreshedPayload;
        ctx.L.emit(event, true);
      },
      signal: ctx.refreshAbort.signal,
      publish: ctx.publish,
    });
  } catch (error) {
    ctx.L.emit(
      { type: 'error', message: `refresh: ${errorMessage(error)}` },
      true
    );
    ctx.chat.system(`Refresh failed: ${errorMessage(error)}`);
    throw new HttpError(500, `refresh failed: ${errorMessage(error)}`);
  }
  const ms = Date.now() - t;
  ctx.chat.system(`Graph refreshed in ${(ms / 1000).toFixed(1)} s.`);
  const { type: _type, ...done } = (summary ?? {
    chunks: [],
  }) as RefreshedPayload & { type?: string };
  return { ...done, ok: true, ms };
}

function startRefresh(ctx: ServerContext) {
  ctx.refreshing = runRefresh(ctx).finally(() => {
    ctx.refreshing = null;
  });
  return ctx.refreshing;
}

// One refreshRun at a time per server (they share <out>). A request during a run queues exactly one follow-up,
// which every later request joins, so edits made meanwhile are picked up once.
function refresh(ctx: ServerContext) {
  if (ctx.building)
    return Promise.reject(
      new HttpError(
        409,
        'the first build is still running; refresh after it finishes'
      )
    );
  if (ctx.queued) {
    ctx.chat.system(
      'A follow-up refresh is already queued; this request joins it.'
    );
    return ctx.queued;
  }
  if (!ctx.refreshing) return startRefresh(ctx);
  ctx.chat.system(
    'A refresh is running; one more will run after it to pick up the latest changes.'
  );
  ctx.queued = ctx.refreshing
    .catch(() => {})
    .then(() => {
      ctx.queued = null;
      return startRefresh(ctx);
    });
  return ctx.queued;
}

const runRoot = (ctx: ServerCore) => ctx.root ?? logRoot(ctx.L.log);

// The viewer's on-demand questions (ast, lsp) run against the run's project; a malformed one fails like a failed
// tool call, in its result event.
function questionRoot(ctx: ServerCore) {
  const root = runRoot(ctx);
  if (!root) throw new Error('this viewer has no run root');
  return root;
}

function stringField(m: ViewerMessage, name: string) {
  const value = m[name];
  if (typeof value !== 'string')
    throw new TypeError(`${name} must be a string`);
  return value;
}

function lspQuestion(m: ViewerMessage): LspQuestion {
  const { line } = m;
  if (line !== undefined && typeof line !== 'number')
    throw new TypeError('line must be a number');
  return {
    file: stringField(m, 'file'),
    symbolName: stringField(m, 'symbolName'),
    line,
    operation: stringField(m, 'operation'),
  };
}

function createOps(
  ctx: ServerCore,
  answers: Answers,
  chat: Chat
): Partial<Record<string, Op>> {
  const { emit } = ctx.L;
  return {
    hello: (m, client) =>
      ctx.L.hello(client, Math.max(0, Number(m.since) || 0)),
    cancel: () => {
      ctx.controller.abort();
      ctx.refreshAbort?.abort();
      emit({
        type: 'note',
        level: 'warn',
        text: 'Cancel requested: running tool calls finish, no new work starts.',
      });
    },
    chat: m => chat.fromBrowser(m),
    query: m => answers.ops.query(m),
    wiki: m => answers.ops.wiki(m),
    ast: async m => {
      emit({ type: 'ast', file: m.file, state: 'start' });
      const r = await Promise.resolve()
        .then(() => astOutline(questionRoot(ctx), stringField(m, 'file')))
        .catch((error: unknown) => ({
          symbols: [],
          error: errorMessage(error),
        }));
      emit({ type: 'ast', file: m.file, state: 'done', ...r });
    },
    lsp: async m => {
      const req = {
        file: m.file,
        symbolName: m.symbolName,
        line: m.line,
        operation: m.operation,
      };
      emit({ type: 'lspResult', req, state: 'start' });
      const r = await Promise.resolve()
        .then(() => lspQuery(questionRoot(ctx), lspQuestion(m)))
        .catch((error: unknown) => ({ rows: [], error: errorMessage(error) }));
      emit({ type: 'lspResult', req, state: 'done', ...r });
    },
  };
}

function onMessage(ctx: ServerContext, text: string, client: WsClient) {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return;
  }
  if (!isRecord(parsed)) return;
  const m = parsed;
  const name = String(m.op);
  const op = Object.hasOwn(ctx.ops, name) ? ctx.ops[name] : undefined;
  if (op)
    Promise.resolve()
      .then(() => reportTools(ctx.L.emit, () => op(m, client)))
      .catch((error: unknown) =>
        ctx.L.emit({
          type: 'note',
          level: 'warn',
          text: `${String(m.op)}: ${errorMessage(error)}`,
        })
      );
}

const origins = (ctx: ServerCore) => [
  `http://127.0.0.1:${ctx.port}`,
  `http://localhost:${ctx.port}`,
];

function readBody(req: IncomingMessage) {
  return new Promise<Record<string, unknown>>((ok, fail) => {
    const parts: Buffer[] = [];
    let size = 0;
    req.on('data', (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_BODY) {
        fail(new HttpError(413, `body over ${MAX_BODY} bytes`));
        req.destroy();
        return;
      }
      parts.push(chunk);
    });
    req.on('end', () => {
      const text = Buffer.concat(parts).toString('utf8');
      // A JSON body that is not an object (null, a string) has no fields.
      try {
        ok(fieldsOf(text.trim() ? JSON.parse(text) : {}));
      } catch {
        fail(new HttpError(400, 'body must be JSON'));
      }
    });
    req.on('error', fail);
  });
}

const API: Record<
  string,
  (ctx: ServerContext, req: IncomingMessage) => Promise<unknown>
> = {
  '/reply': async (ctx, req) => ctx.chat.reply(await readBody(req)),
  '/refresh': ctx => refresh(ctx),
  '/query': async (ctx, req) =>
    ctx.answers.query((await readBody(req)).query, 'agent'),
  '/wiki': async (ctx, req) => ctx.answers.wiki(await readBody(req)),
};

// An HttpError (serve-answers.ts) carries a refusal's status and JSON detail; anything else is a 500.
const statusOf = (error: unknown) =>
  isRecord(error) && typeof error.status === 'number' ? error.status : 500;
const detailOf = (error: unknown) =>
  isRecord(error) && isRecord(error.detail) ? error.detail : {};

// POST /reply, /refresh, /query and /wiki: this run's token in ?t=, no foreign Origin (a page elsewhere cannot post here).
async function api(
  ctx: ServerContext,
  req: IncomingMessage,
  res: ServerResponse,
  url: URL
) {
  const send = (status: number, body: unknown) =>
    res
      .writeHead(status, {
        'content-type': 'application/json',
        'cache-control': 'no-store',
      })
      .end(JSON.stringify(body));
  try {
    if (
      !sameToken(url.searchParams.get('t'), ctx.token) ||
      (req.headers.origin && !origins(ctx).includes(req.headers.origin))
    )
      throw new HttpError(403, 'forbidden');
    if (req.method !== 'POST') throw new HttpError(405, 'POST only');
    send(200, await API[url.pathname](ctx, req));
  } catch (error) {
    send(statusOf(error), {
      ok: false,
      error: errorMessage(error),
      ...detailOf(error),
    });
  }
}

async function asset(req: IncomingMessage, res: ServerResponse, name: string) {
  let file: FileHandle | undefined;
  try {
    file = await openFile(join(ASSETS, name));
    const { size, mtimeNs } = await file.stat({ bigint: true });
    const etag = `W/"${size}-${mtimeNs}"`;
    const headers = { 'cache-control': 'private, no-cache', etag };
    if (
      req.headers['if-none-match']
        ?.split(',')
        .some(value => [etag, '*'].includes(value.trim()))
    ) {
      res.writeHead(304, headers).end();
      return;
    }
    res.writeHead(200, {
      ...headers,
      'content-type': TYPES[extname(name)],
      'content-length': String(size),
    });
    if (req.method === 'HEAD') res.end();
    else await pipeline(file.createReadStream(), res);
  } catch (error) {
    if (!res.headersSent && !res.destroyed)
      res
        .writeHead(isRecord(error) && error.code === 'ENOENT' ? 404 : 500)
        .end();
    else if (!res.destroyed) res.destroy();
  } finally {
    await file?.close();
  }
}

function route(ctx: ServerContext, req: IncomingMessage, res: ServerResponse) {
  const url = new URL(req.url ?? '/', 'http://127.0.0.1');
  if (Object.hasOwn(API, url.pathname)) {
    void api(ctx, req, res, url);
    return;
  }
  const name = url.pathname === '/' ? 'live.html' : url.pathname.slice(1);
  if (
    !STATIC.has(name) ||
    (name === 'live.html' && !sameToken(url.searchParams.get('t'), ctx.token))
  ) {
    res.writeHead(404).end();
    return;
  }
  void asset(req, res, name);
}

// The socket needs this run's token and a page served by this server (Origin), so other local pages cannot attach.
// That socket is the only way a chat message enters: it is the channel's sender gate.
function upgrade(ctx: ServerContext, req: IncomingMessage, socket: Duplex) {
  const url = new URL(req.url ?? '/', 'http://127.0.0.1');
  if (
    url.pathname !== '/ws' ||
    !sameToken(url.searchParams.get('t'), ctx.token) ||
    !origins(ctx).includes(req.headers.origin ?? '')
  ) {
    socket.end('HTTP/1.1 403 Forbidden\r\n\r\n');
    return;
  }
  // An http.Server upgrades over the request's net.Socket (typed here as its Duplex base).
  const client = accept(req, socket as Socket, {
    onMessage: (text, c) => onMessage(ctx, text, c),
    onDrain: ctx.L.schedule,
    onClose: () => ctx.L.clients.delete(client),
  });
  ctx.L.clients.add(client);
}

async function build(ctx: ServerContext): Promise<BuildResult> {
  const t0 = Date.now();
  let error: Error | null = null;
  try {
    await runPipeline({
      // startServer resolved the root of every build (only a replay has none).
      root: ctx.root!,
      out: ctx.out,
      opts: ctx.opts,
      emit: ctx.L.emit,
      signal: ctx.controller.signal,
      publish: ctx.publish,
    });
  } catch (e) {
    error = e instanceof Error ? e : new Error(String(e));
    ctx.L.emit({ type: 'error', message: error.message });
  }
  ctx.building = false;
  void ctx.answers.prime();
  process.stderr.write(
    `done in ${((Date.now() - t0) / 1000).toFixed(1)} s: ${join(ctx.out, 'data.json')}; the viewer stays live for AST, LSP and chat (Ctrl-C to stop)\n`
  );
  return { ok: !error, error: error?.message };
}

function closeServer(ctx: ServerContext, server: Server) {
  ctx.controller.abort();
  ctx.refreshAbort?.abort();
  for (const c of ctx.L.clients) c.close(1001);
  server.closeAllConnections?.();
  try {
    const info: unknown = JSON.parse(
      readFileSync(join(ctx.out, 'server.json'), 'utf8')
    );
    if (isRecord(info) && info.token === ctx.token)
      rmSync(join(ctx.out, 'server.json'));
  } catch {
    /* already gone or replaced by a newer server */
  }
  return new Promise<void>(ok => server.close(() => ok()));
}

/**
 * Starts the viewer server: builds `root` into `out` (or replays `out` when `replay`), writes <out>/server.json and
 * never prints to stdout. `transport` delivers each browser chat message: 'inbox' (append to <out>/chat-inbox.jsonl)
 * or a function receiving { id, text, context, at }. `refreshRun` replaces refresh.ts's (embedding, tests).
 */
export async function startServer({
  root = null,
  out,
  opts = defaults(),
  replay = false,
  port = 0,
  open = true,
  transport = 'inbox',
  refreshRun = null,
}: ServerOptions = {}) {
  if (!replay) {
    if (root === null)
      throw new TypeError(
        'startServer needs root to build (or replay: true and out)'
      );
    const scope = graphInput(root);
    out ??= freshOut(root);
    root = scope.root;
    if (scope.file) opts = { ...opts, file: scope.file };
  }
  if (out === undefined) throw new TypeError('startServer needs out to replay');
  const L = createLog({ out, persistAll: !replay });
  if (replay) L.load(readEvents(out));
  const core: ServerCore = {
    root,
    out,
    opts,
    token: randomBytes(16).toString('hex'),
    L,
    controller: new AbortController(),
    building: !replay,
    refreshing: null,
    queued: null,
    refreshAbort: null,
    refreshRun,
    port: 0,
    readLive: null,
    publish: read => {
      core.readLive = read;
    },
  };
  // The run event keeps the options the replayed build ran with (pipeline.ts writes them).
  if (replay)
    core.opts = {
      ...defaults(),
      ...(L.log.find(e => e.type === 'run')?.options as
        Partial<RunOptions> | undefined),
    };
  const answers = createAnswers(core);
  const chat = createChat(core, answers, transport);
  const ctx: ServerContext = Object.assign(core, {
    answers,
    chat,
    ops: createOps(core, answers, chat),
  });
  const server = createServer((req, res) => route(ctx, req, res));
  server.on('upgrade', (req: IncomingMessage, socket: Duplex) =>
    upgrade(ctx, req, socket)
  );
  await new Promise<void>((ok, fail) => {
    server.once('error', fail);
    server.listen(port || 0, '127.0.0.1', ok);
  });
  // Listening on a TCP port: address() is an AddressInfo.
  ctx.port = (server.address() as AddressInfo).port;
  const url = `http://127.0.0.1:${ctx.port}/?t=${ctx.token}`;
  let claimed = false;
  try {
    if (!replay) {
      beginRun(out);
      claimed = true;
    }
    writeServerInfo({
      url,
      token: ctx.token,
      port: ctx.port,
      pid: process.pid,
      out,
      root: runRoot(ctx) ?? null,
    });
  } catch (error) {
    await new Promise(resolve => server.close(resolve));
    if (claimed) rmSync(join(out, 'events.jsonl'), { force: true });
    throw error;
  }
  if (open) openBrowser(url);
  // A replay answers queries at once: its data.json loads now (a build loads it when the run writes it).
  void ctx.answers.prime();
  const finished: Promise<BuildResult> = replay
    ? Promise.resolve({ ok: true })
    : build(ctx);
  return {
    url,
    token: ctx.token,
    port: ctx.port,
    out,
    finished,
    root: () => runRoot(ctx),
    emit: (event: RunEvent) => L.emit(event, true),
    reply: (body: ReplyBody) => ctx.chat.reply(body),
    refresh: () => refresh(ctx),
    query: (text: unknown) => ctx.answers.query(text, 'agent'),
    wiki: (body: WikiBody) => ctx.answers.wiki(body),
    close: () => closeServer(ctx, server),
  };
}

/** A running viewer server, as startServer returns it. */
export type ViewerServer = Awaited<ReturnType<typeof startServer>>;

export async function main(argv: string[]) {
  if (argv.includes('--help')) {
    process.stdout.write(`${usage()}\n`);
    return;
  }
  let o: ReturnType<typeof runOptions>;
  try {
    o = runOptions(argv, true);
  } catch (error) {
    process.stderr.write(`${errorMessage(error)}\n${usage()}\n`);
    process.exitCode = 2;
    return;
  }
  let s: ViewerServer;
  try {
    s = await startServer({ ...o, transport: 'inbox' });
  } catch (error) {
    process.stderr.write(`octocode-graph serve: ${errorMessage(error)}\n`);
    process.exitCode = 1;
    return;
  }
  process.stdout.write(
    `${JSON.stringify({ url: s.url, out: s.out, replay: o.replay })}\n`
  );
  const stop = (code: number | string) => () =>
    void s.close().then(() => process.exit(code));
  process.once('SIGINT', stop(130));
  process.once('SIGTERM', stop(143));
  const r = await s.finished;
  if (!r.ok) process.exitCode = 1;
  if (o.exitWhenDone) setTimeout(stop(process.exitCode ?? 0), 200);
}
