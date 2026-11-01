#!/usr/bin/env node
// Lean local viewer server: 127.0.0.1 only, one random token per run, the viewer assets, and one WebSocket
// that replays every event so far and then streams new ones. The browser asks for AST outlines, LSP answers and
// chat with the agent over the same socket; those are events too, so a replay shows every tool call and message.
// `startServer` is the module API (channel.mjs runs it in-process); the CLI below keeps the script's flags.
import { createServer } from 'node:http';
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { spawn } from 'node:child_process';
import { basename, dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { flags } from './lib.mjs';
import { accept } from './ws.mjs';
import { astOutline, defaults, lspQuery, readEvents, runPipeline } from './pipeline.mjs';
import { createAnswers } from './serve-answers.mjs';

const USAGE = `usage: serve.mjs <root> [--out <dir>] [--whole] [--lsp on|off] [--jobs N] [--ingest-jobs N] [--exclude a,b]
                 [--port N] [--no-open] [--exit-when-done]
       serve.mjs --replay <result-dir> [--port N] [--no-open]
  Builds <root> chunk by chunk and streams every graph, LSP and AST page to the viewer as it arrives.
  Run it from the target workspace. Prints the viewer URL; Ctrl-C stops it (results stay in <out>).
  Chat from the viewer lands in <out>/chat-inbox.jsonl; answer with reply.mjs (references/chat.md).
  Graph queries and the wiki: reply.mjs query|wiki <out> ... (references/query.md).`;

const here = dirname(fileURLToPath(import.meta.url));
const ASSETS = join(here, '..', 'assets');
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8' };
const STATIC = new Set(['live.html', 'live.js', 'live-graph.js', 'live-panels.js', 'live-chat.js', 'live-overview.js', 'live-query.js', 'live-wiki.js', 'live.css', 'live-chat.css', 'live-query.css', 'live-wiki.css']);
// A request body larger than this is refused whole (never cut): replies are text, not files.
const MAX_BODY = 4 * 1024 * 1024;

class HttpError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

// A fresh result directory under the workspace's .octocode/octocode-graph-view/.
export function freshOut(root) {
  const parent = resolve('.octocode/octocode-graph-view');
  mkdirSync(parent, { recursive: true });
  return mkdtempSync(join(parent, `${new Date().toISOString().slice(0, 10)}-${basename(root)}-`));
}

export function openBrowser(url) {
  const [cmd, args] = process.platform === 'darwin' ? ['open', [url]] : process.platform === 'win32' ? ['cmd', ['/c', 'start', '', url]] : ['xdg-open', [url]];
  spawn(cmd, args, { stdio: 'ignore', detached: true }).on('error', () => {}).unref();
}

// Chat messages and graph answers get ids (c12, q13): later events and replies refer to them.
const ID_PREFIX = { chat: 'c', query: 'q' };

const sameToken = (given, token) => typeof given === 'string' && given.length === token.length && timingSafeEqual(Buffer.from(given), Buffer.from(token));

// Every event gets a sequence number; the log is the replay source and events.jsonl its saved copy. A replay keeps
// its saved file as it was except for chat and refresh events, so sequence numbers can skip: the next one always
// follows the highest seen, and `since` filters by number, not position.
function createLog({ out, persistAll }) {
  const log = [];
  const clients = new Set();
  let pending = [];
  let flushing = false;
  let nextSeq = 0;
  let t0 = Date.now();
  const flush = () => {
    flushing = false;
    if (!pending.length) return;
    // Batches travel gzipped (lossless; a full-repo replay is 44 MB of JSON and 2.1 MB gzipped), compressed once for all clients.
    const batch = gzipSync(JSON.stringify({ type: 'batch', events: pending }));
    pending = [];
    // A client gets live batches only after its replay went out (see `hello`): a live batch arriving first would
    // advance its sequence past the history it has not received yet, and it would discard that history.
    for (const c of clients) if (c.replayed) c.sendBinary(batch);
  };
  const emit = (event, save = persistAll || event.type === 'chat') => {
    const seq = nextSeq++;
    const e = { seq, t: Date.now() - t0, ...(ID_PREFIX[event.type] && !event.id ? { id: `${ID_PREFIX[event.type]}${seq}` } : {}), ...event };
    log.push(e);
    if (save) appendFileSync(join(out, 'events.jsonl'), JSON.stringify(e) + '\n');
    pending.push(e);
    if (!flushing) { flushing = true; setImmediate(flush); }
    return e;
  };
  const load = (events) => {
    for (const e of events) {
      log.push(e);
      nextSeq = Math.max(nextSeq, (e.seq ?? -1) + 1);
      t0 = Math.min(t0, Date.now() - (e.t ?? 0));
    }
  };
  const since = (n) => (log[n]?.seq === n && log[0]?.seq === 0 && log.at(-1).seq === log.length - 1 ? log.slice(n) : log.filter((e) => e.seq >= n));
  return { log, clients, emit, load, since };
}

const scalar = (v) => v === null || ['number', 'string', 'boolean'].includes(typeof v);

// Chat context from the page: the selected node's file, line and metrics; anything else is dropped.
function cleanContext(c) {
  if (!c || typeof c !== 'object' || typeof c.file !== 'string' || !c.file) return undefined;
  const metrics = c.metrics && typeof c.metrics === 'object'
    ? Object.fromEntries(Object.entries(c.metrics).filter(([k, v]) => /^\w+$/.test(k) && scalar(v)))
    : undefined;
  return { file: c.file, line: Number.isInteger(c.line) && c.line > 0 ? c.line : 1, ...(metrics ? { metrics } : {}) };
}

// The default transport: one JSON line per message in <out>/chat-inbox.jsonl, for an agent watching that file.
function inboxTransport(out) {
  const file = join(out, 'chat-inbox.jsonl');
  return (m) => appendFileSync(file, JSON.stringify({ chat_id: m.id, text: m.text, ...m.context, at: new Date(m.at).toISOString() }) + '\n');
}

// User messages and their delivery state (queued -> delivered -> replied, or failed); rebuilt from a replayed log.
function createChat(ctx, transport) {
  const deliver = typeof transport === 'function' ? transport : inboxTransport(ctx.out);
  const states = new Map();
  for (const e of ctx.L.log) if (e.type === 'chat' && (e.role === 'user' || states.has(e.id))) states.set(e.id, e.state ?? states.get(e.id));
  const mark = (id, state, extra = {}) => {
    states.set(id, state);
    ctx.L.emit({ type: 'chat', id, state, ...extra });
  };
  const system = (text) => ctx.L.emit({ type: 'chat', role: 'system', text, at: Date.now() });
  async function fromBrowser(m) {
    const text = typeof m.text === 'string' ? m.text.trim() : '';
    if (!text) return;
    const e = ctx.L.emit({ type: 'chat', role: 'user', text, context: cleanContext(m.context), state: 'queued', at: Date.now() });
    states.set(e.id, 'queued');
    try {
      await deliver({ id: e.id, text, context: e.context, at: e.at });
      if (states.get(e.id) === 'queued') mark(e.id, 'delivered');
    } catch (error) {
      mark(e.id, 'failed', { error: error.message });
    }
  }
  // `nodes` (paths, suffixes or ids) resolve against the graph before anything is sent: a reply never points at a guess.
  async function reply({ text, chat_id: chatId, nodes } = {}) {
    if (typeof text !== 'string' || !text.trim()) throw new HttpError(400, 'text must be a non-empty string');
    if (chatId && !states.has(chatId)) throw new HttpError(404, `no chat message ${chatId} in this viewer`);
    const ids = await ctx.answers.nodes(nodes);
    const target = chatId || [...states].findLast(([, s]) => s !== 'replied')?.[0];
    const e = ctx.L.emit({ type: 'chat', role: 'agent', text, ...(ids.length ? { nodes: ids } : {}), ...(target ? { replyTo: target } : {}), at: Date.now() });
    if (target) mark(target, 'replied');
    return { ok: true, id: e.id, replyTo: target ?? null, ...(ids.length ? { nodes: ids } : {}) };
  }
  return { fromBrowser, reply, system };
}

async function loadRefresh(ctx) {
  const fn = existsSync(join(here, 'refresh.mjs')) ? (await import('./refresh.mjs')).refreshRun : undefined;
  if (typeof fn === 'function') return fn;
  ctx.chat.system('Refresh is unavailable: this copy of the skill has no scripts/refresh.mjs with refreshRun.');
  throw new HttpError(501, 'refresh unavailable: scripts/refresh.mjs (refreshRun) is missing');
}

// Rebuilds the graph after code changes through refresh.mjs (`refreshRun`, which reads the run's options from
// <out>); its events are saved even in a replay. A failure is an `error` event and a system chat line.
async function runRefresh(ctx) {
  const root = runRoot(ctx);
  if (!root) throw new HttpError(409, 'this viewer has no run root to refresh');
  const refreshRun = ctx.refreshRun ?? (await loadRefresh(ctx));
  const t = Date.now();
  ctx.refreshAbort = new AbortController();
  ctx.chat.system('Refreshing the graph from the files on disk…');
  try {
    await refreshRun({ root, out: ctx.out, emit: (event) => ctx.L.emit(event, true), signal: ctx.refreshAbort.signal });
  } catch (error) {
    ctx.L.emit({ type: 'error', message: `refresh: ${error.message}` }, true);
    ctx.chat.system(`Refresh failed: ${error.message}`);
    throw new HttpError(500, `refresh failed: ${error.message}`);
  }
  const ms = Date.now() - t;
  ctx.chat.system(`Graph refreshed in ${(ms / 1000).toFixed(1)} s.`);
  return { ok: true, ms };
}

function startRefresh(ctx) {
  ctx.refreshing = runRefresh(ctx).finally(() => { ctx.refreshing = null; });
  return ctx.refreshing;
}

// One refreshRun at a time per server (they share <out>). A request during a run queues exactly one follow-up,
// which every later request joins, so edits made meanwhile are picked up once.
function refresh(ctx) {
  if (ctx.building) return Promise.reject(new HttpError(409, 'the first build is still running; refresh after it finishes'));
  if (ctx.queued) {
    ctx.chat.system('A follow-up refresh is already queued; this request joins it.');
    return ctx.queued;
  }
  if (!ctx.refreshing) return startRefresh(ctx);
  ctx.chat.system('A refresh is running; one more will run after it to pick up the latest changes.');
  ctx.queued = ctx.refreshing.catch(() => {}).then(() => {
    ctx.queued = null;
    return startRefresh(ctx);
  });
  return ctx.queued;
}

const runRoot = (ctx) => ctx.root ?? ctx.L.log.find((e) => e.type === 'run')?.root;

function createOps(ctx) {
  const { emit } = ctx.L;
  return {
    hello: (m, client) => {
      client.sendBinary(gzipSync(JSON.stringify({ type: 'batch', events: ctx.L.since(Math.max(0, Number(m.since) || 0)), replay: true })));
      client.replayed = true;
    },
    cancel: () => { ctx.controller.abort(); ctx.refreshAbort?.abort(); emit({ type: 'note', level: 'warn', text: 'Cancel requested: running tool calls finish, no new work starts.' }); },
    chat: (m) => ctx.chat.fromBrowser(m),
    query: (m) => ctx.answers.ops.query(m),
    wiki: (m) => ctx.answers.ops.wiki(m),
    ast: async (m) => {
      emit({ type: 'ast', file: m.file, state: 'start' });
      const r = await astOutline(runRoot(ctx), m.file).catch((error) => ({ symbols: [], error: error.message }));
      emit({ type: 'ast', file: m.file, state: 'done', ...r });
    },
    lsp: async (m) => {
      const req = { file: m.file, symbolName: m.symbolName, line: m.line, operation: m.operation };
      emit({ type: 'lspResult', req, state: 'start' });
      const r = await lspQuery(runRoot(ctx), req).catch((error) => ({ rows: [], error: error.message }));
      emit({ type: 'lspResult', req, state: 'done', ...r });
    },
  };
}

function onMessage(ctx, text, client) {
  let m;
  try { m = JSON.parse(text); } catch { return; }
  const op = ctx.ops[m?.op];
  if (op) Promise.resolve(op(m, client)).catch((error) => ctx.L.emit({ type: 'note', level: 'warn', text: `${m.op}: ${error.message}` }));
}

const origins = (ctx) => [`http://127.0.0.1:${ctx.port}`, `http://localhost:${ctx.port}`];

function readBody(req) {
  return new Promise((ok, fail) => {
    const parts = [];
    let size = 0;
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size > MAX_BODY) { fail(new HttpError(413, `body over ${MAX_BODY} bytes`)); req.destroy(); return; }
      parts.push(chunk);
    });
    req.on('end', () => {
      const text = Buffer.concat(parts).toString('utf8');
      try { ok(text.trim() ? JSON.parse(text) : {}); } catch { fail(new HttpError(400, 'body must be JSON')); }
    });
    req.on('error', fail);
  });
}

const API = {
  '/reply': async (ctx, req) => ctx.chat.reply(await readBody(req)),
  '/refresh': (ctx) => refresh(ctx),
  '/query': async (ctx, req) => ctx.answers.query((await readBody(req)).query, 'agent'),
  '/wiki': async (ctx, req) => ctx.answers.wiki(await readBody(req)),
};

// POST /reply, /refresh, /query and /wiki: this run's token in ?t=, no foreign Origin (a page elsewhere cannot post here).
async function api(ctx, req, res, url) {
  const send = (status, body) => res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' }).end(JSON.stringify(body));
  try {
    if (!sameToken(url.searchParams.get('t'), ctx.token) || (req.headers.origin && !origins(ctx).includes(req.headers.origin))) throw new HttpError(403, 'forbidden');
    if (req.method !== 'POST') throw new HttpError(405, 'POST only');
    send(200, await API[url.pathname](ctx, req));
  } catch (error) {
    send(error.status ?? 500, { ok: false, error: error.message, ...error.detail });
  }
}

function route(ctx, req, res) {
  const url = new URL(req.url, 'http://127.0.0.1');
  if (Object.hasOwn(API, url.pathname)) {
    void api(ctx, req, res, url);
    return;
  }
  const name = url.pathname === '/' ? 'live.html' : url.pathname.slice(1);
  if (!STATIC.has(name) || (name === 'live.html' && !sameToken(url.searchParams.get('t'), ctx.token))) {
    res.writeHead(404).end();
    return;
  }
  res.writeHead(200, { 'content-type': TYPES[extname(name)], 'cache-control': 'no-store' });
  res.end(readFileSync(join(ASSETS, name)));
}

// The socket needs this run's token and a page served by this server (Origin), so other local pages cannot attach.
// That socket is the only way a chat message enters: it is the channel's sender gate.
function upgrade(ctx, req, socket) {
  const url = new URL(req.url, 'http://127.0.0.1');
  if (url.pathname !== '/ws' || !sameToken(url.searchParams.get('t'), ctx.token) || !origins(ctx).includes(req.headers.origin)) {
    socket.end('HTTP/1.1 403 Forbidden\r\n\r\n');
    return;
  }
  const client = accept(req, socket, { onMessage: (text, c) => onMessage(ctx, text, c), onClose: () => ctx.L.clients.delete(client) });
  ctx.L.clients.add(client);
}

async function build(ctx) {
  const t0 = Date.now();
  let error = null;
  try {
    await runPipeline({ root: ctx.root, out: ctx.out, opts: ctx.opts, emit: ctx.L.emit, signal: ctx.controller.signal });
  } catch (e) {
    error = e;
    ctx.L.emit({ type: 'error', message: e.message });
  }
  ctx.building = false;
  void ctx.answers.prime();
  process.stderr.write(`done in ${((Date.now() - t0) / 1000).toFixed(1)} s: ${join(ctx.out, 'data.json')}; the viewer stays live for AST, LSP and chat (Ctrl-C to stop)\n`);
  return { ok: !error, error: error?.message };
}

function closeServer(ctx, server) {
  ctx.controller.abort();
  ctx.refreshAbort?.abort();
  for (const c of ctx.L.clients) c.close(1001);
  server.closeAllConnections?.();
  try {
    const info = JSON.parse(readFileSync(join(ctx.out, 'server.json'), 'utf8'));
    if (info.token === ctx.token) rmSync(join(ctx.out, 'server.json'));
  } catch { /* already gone or replaced by a newer server */ }
  return new Promise((ok) => server.close(() => ok()));
}

/**
 * Starts the viewer server: builds `root` into `out` (or replays `out` when `replay`), writes <out>/server.json and
 * never prints to stdout. `transport` delivers each browser chat message: 'inbox' (append to <out>/chat-inbox.jsonl)
 * or a function receiving { id, text, context, at }. `refreshRun` replaces refresh.mjs's (embedding, tests).
 */
export async function startServer({ root = null, out, opts = defaults(), replay = false, port = 0, open = true, transport = 'inbox', refreshRun = null } = {}) {
  mkdirSync(out, { recursive: true });
  const L = createLog({ out, persistAll: !replay });
  if (replay) L.load(readEvents(out));
  const ctx = { root, out, opts, token: randomBytes(16).toString('hex'), L, controller: new AbortController(), building: !replay, refreshing: null, queued: null, refreshAbort: null, refreshRun, port: 0 };
  if (replay) ctx.opts = { ...defaults(), ...L.log.find((e) => e.type === 'run')?.options };
  ctx.answers = createAnswers(ctx);
  ctx.chat = createChat(ctx, transport);
  ctx.ops = createOps(ctx);
  const server = createServer((req, res) => route(ctx, req, res));
  server.on('upgrade', (req, socket) => upgrade(ctx, req, socket));
  await new Promise((ok, fail) => { server.once('error', fail); server.listen(port || 0, '127.0.0.1', ok); });
  ctx.port = server.address().port;
  const url = `http://127.0.0.1:${ctx.port}/?t=${ctx.token}`;
  writeFileSync(join(out, 'server.json'), JSON.stringify({ url, token: ctx.token, port: ctx.port, pid: process.pid, out, root: runRoot(ctx) ?? null }) + '\n', { mode: 0o600 });
  if (open) openBrowser(url);
  // A replay answers queries at once: its data.json loads now (a build loads it when the run writes it).
  void ctx.answers.prime();
  const finished = replay ? Promise.resolve({ ok: true }) : build(ctx);
  return {
    url, token: ctx.token, port: ctx.port, out, finished,
    root: () => runRoot(ctx),
    emit: (event) => L.emit(event, true),
    reply: (body) => ctx.chat.reply(body),
    refresh: () => refresh(ctx),
    query: (text) => ctx.answers.query(text, 'agent'),
    wiki: (body) => ctx.answers.wiki(body),
    close: () => closeServer(ctx, server),
  };
}

function cliOptions(argv) {
  const f = flags(argv, { out: '', lsp: 'on', jobs: '', 'ingest-jobs': '', exclude: '', port: '0', replay: '' });
  const root = f.replay ? null : argv[0] && !argv[0].startsWith('-') ? resolve(argv[0]) : null;
  if (!root && !f.replay) return null;
  const opts = { ...defaults(), whole: argv.includes('--whole'), lsp: f.lsp, exclude: f.exclude.split(',').filter(Boolean) };
  if (f.jobs) opts.jobs = Math.max(1, Number(f.jobs));
  if (f['ingest-jobs']) opts.ingestJobs = Math.max(1, Number(f['ingest-jobs']));
  const out = f.replay ? resolve(f.replay) : f.out ? resolve(f.out) : freshOut(root);
  return { root, out, opts, replay: Boolean(f.replay), port: Number(f.port), open: !argv.includes('--no-open') };
}

async function main(argv) {
  if (argv.includes('--help')) { process.stdout.write(`${USAGE}\n`); return; }
  const o = cliOptions(argv);
  if (!o) { process.stderr.write(`${USAGE}\n`); process.exitCode = 2; return; }
  let s;
  try {
    s = await startServer({ ...o, transport: 'inbox' });
  } catch (error) {
    process.stderr.write(`serve.mjs: ${error.message}\n`);
    process.exitCode = 1;
    return;
  }
  process.stdout.write(`${JSON.stringify({ url: s.url, out: s.out, replay: o.replay })}\n`);
  const stop = (code) => () => void s.close().then(() => process.exit(code));
  process.once('SIGINT', stop(130));
  process.once('SIGTERM', stop(143));
  const r = await s.finished;
  if (!r.ok) process.exitCode = 1;
  if (argv.includes('--exit-when-done')) setTimeout(stop(process.exitCode ?? 0), 200);
}

const isMain = () => {
  try { return Boolean(process.argv[1]) && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url)); } catch { return false; }
};
if (isMain()) void main(process.argv.slice(2));
