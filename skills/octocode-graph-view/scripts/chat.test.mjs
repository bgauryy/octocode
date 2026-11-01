// Chat between the viewer and the agent: the channel's MCP handshake and tools, chat delivery through the channel
// and the inbox, and the token-gated /reply endpoint. Hermetic: replays a tiny result directory, no octocode CLI.
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';
import { parseFrame } from './ws.mjs';
import { startServer } from './serve.mjs';
import { computeMetrics } from './metrics.mjs';

const here = dirname(fileURLToPath(import.meta.url));

// A finished result to replay: one run event over a one-file repository.
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), 'gv-chat-'));
  const repo = join(dir, 'repo');
  const out = join(dir, 'out');
  mkdirSync(join(repo, 'src'), { recursive: true });
  mkdirSync(out);
  writeFileSync(join(repo, 'src', 'a.ts'), 'export const a = 1;\n');
  writeFileSync(join(out, 'events.jsonl'), `${JSON.stringify({ seq: 0, t: 0, type: 'run', root: repo, name: 'repo', out, options: { lsp: 'off' } })}\n`);
  writeFileSync(join(out, 'data.json'), JSON.stringify(graphData(repo)));
  return { dir, repo, out };
}

const file = (id) => ({ id, label: id.split('/').pop(), chunk: 'repo', loc: { path: id, line: 1 } });

// The result's graph: src/b.ts imports src/a.ts; lib/a.ts makes a bare `a.ts` ambiguous.
function graphData(repo) {
  const D = { section: repo, files: [file('src/a.ts'), file('src/b.ts'), file('lib/a.ts')], imports: [{ source: 'src/b.ts', target: 'src/a.ts', line: 1, lines: [1], test: false }], symbols: [], calls: [] };
  computeMetrics(D);
  return D;
}

// A browser-side frame: masked, as RFC 6455 requires of clients.
function maskedFrame(text) {
  const body = Buffer.from(text);
  const key = randomBytes(4);
  const n = body.length;
  const head = n < 126 ? Buffer.from([0x81, 0x80 | n]) : Buffer.from([0x81, 0x80 | 126, n >> 8, n & 255]);
  return Buffer.concat([head, key, Buffer.from(body.map((b, i) => b ^ key[i & 3]))]);
}

// A WebSocket client on node:http, because Node's built-in WebSocket cannot set the Origin header.
function wsClient(socket, head) {
  let buf = Buffer.from(head ?? []);
  const events = [];
  const waiters = new Set();
  const check = () => {
    for (const w of waiters) {
      const e = events.find(w.pred);
      if (e) { waiters.delete(w); w.ok(e); }
    }
  };
  socket.on('data', (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    for (let f = parseFrame(buf); f; f = parseFrame(buf)) {
      buf = buf.subarray(f.size);
      if (f.opcode === 2) events.push(...JSON.parse(gunzipSync(f.payload)).events);
    }
    check();
  });
  return {
    events,
    send: (value) => socket.write(maskedFrame(JSON.stringify(value))),
    waitFor: (pred) => new Promise((ok) => { waiters.add({ pred, ok }); check(); }),
    close: () => socket.destroy(),
  };
}

function connect(pageUrl, origin) {
  const u = new URL(pageUrl);
  return new Promise((ok, fail) => {
    const headers = { Connection: 'Upgrade', Upgrade: 'websocket', 'Sec-WebSocket-Version': '13', 'Sec-WebSocket-Key': randomBytes(16).toString('base64'), Origin: origin ?? u.origin };
    const req = request({ host: u.hostname, port: u.port, path: `/ws?t=${u.searchParams.get('t')}`, headers });
    req.on('upgrade', (res, socket, head) => ok(wsClient(socket, head)));
    req.on('response', (res) => fail(Object.assign(new Error(`no upgrade: HTTP ${res.statusCode}`), { status: res.statusCode })));
    req.on('error', fail);
    req.end();
  });
}

// reply.mjs as a child, asynchronously: this process hosts the server it talks to.
const replyCli = (...args) => new Promise((ok) => {
  execFile(process.execPath, [join(here, 'reply.mjs'), ...args], { encoding: 'utf8' }, (error, stdout, stderr) => ok({ status: error ? error.code : 0, stdout, stderr }));
});

const post = (pageUrl, path, body, token = new URL(pageUrl).searchParams.get('t')) =>
  fetch(new URL(`${path}${token ? `?t=${token}` : ''}`, pageUrl), { method: 'POST', body: JSON.stringify(body) });

// The channel as Claude Code runs it: a child on stdio, one JSON-RPC message per line.
function channel(cwd) {
  const child = spawn(process.execPath, [join(here, 'channel.mjs')], { cwd, env: { ...process.env, GRAPH_VIEW_NO_OPEN: '1' }, stdio: ['pipe', 'pipe', 'pipe'] });
  const lines = [];
  const waiters = new Set();
  createInterface({ input: child.stdout }).on('line', (line) => {
    lines.push(JSON.parse(line));
    for (const w of waiters) {
      const m = lines.find(w.pred);
      if (m) { waiters.delete(w); w.ok(m); }
    }
  });
  const waitFor = (pred) => new Promise((ok) => {
    const m = lines.find(pred);
    if (m) ok(m);
    else waiters.add({ pred, ok });
  });
  let id = 0;
  const call = (method, params) => {
    const n = ++id;
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: n, method, params })}\n`);
    return waitFor((m) => m.id === n);
  };
  const notify = (method, params) => child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  return { child, lines, waitFor, call, notify };
}

test('the channel declares claude/channel, an earlier protocol revision and its tools', { timeout: 20000 }, async (t) => {
  const c = channel(tmpdir());
  t.after(() => c.child.kill());
  const init = await c.call('initialize', { protocolVersion: '2026-07-28', capabilities: {}, clientInfo: { name: 'test', version: '0' } });
  assert.equal(init.result.protocolVersion, '2025-06-18');
  assert.deepEqual(init.result.capabilities, { experimental: { 'claude/channel': {} }, tools: {} });
  assert.match(init.result.instructions, /<channel source="graph-view"/);
  assert.match(init.result.instructions, /refresh_graph/);
  const older = await c.call('initialize', { protocolVersion: '2025-03-26', capabilities: {} });
  assert.equal(older.result.protocolVersion, '2025-03-26');
  c.notify('notifications/initialized');
  const list = await c.call('tools/list', {});
  assert.deepEqual(list.result.tools.map((tool) => tool.name).toSorted(), ['open_graph', 'query_graph', 'refresh_graph', 'reply', 'wiki_lint', 'wiki_note', 'wiki_promote', 'wiki_read']);
  assert.match(init.result.instructions, /query_graph/);
  assert.match(init.result.instructions, /wiki_promote/);
  const missing = await c.call('resources/list', {});
  assert.equal(missing.error.code, -32601);
  const noGraph = await c.call('tools/call', { name: 'refresh_graph', arguments: {} });
  assert.equal(noGraph.result.isError, true);
  c.child.stdin.end();
  const [code] = await new Promise((ok) => c.child.on('exit', (...a) => ok(a)));
  assert.equal(code, 0);
  assert.ok(c.lines.every((m) => m.jsonrpc === '2.0'), 'stdout carries only JSON-RPC');
});

test('a viewer chat reaches the session as notifications/claude/channel and the reply tool answers it', { timeout: 30000 }, async (t) => {
  const f = fixture();
  const c = channel(f.dir);
  t.after(() => { c.child.kill(); rmSync(f.dir, { recursive: true, force: true }); });
  await c.call('initialize', { protocolVersion: '2025-06-18', capabilities: {} });
  c.notify('notifications/initialized');
  const opened = await c.call('tools/call', { name: 'open_graph', arguments: { result: f.out } });
  assert.notEqual(opened.result.isError, true, opened.result.content[0].text);
  const { url } = JSON.parse(opened.result.content[0].text);
  const ws = await connect(url);
  t.after(() => ws.close());
  ws.send({ op: 'hello', since: 0 });
  await ws.waitFor((e) => e.type === 'run');
  ws.send({ op: 'chat', text: 'why is `a` a hub?', context: { file: 'src/a.ts', line: 1, metrics: { pagerank: 0.25, 'bad-key': 1 } } });
  const note = await c.waitFor((m) => m.method === 'notifications/claude/channel');
  assert.match(note.params.meta.chat_id, /^\w+$/);
  assert.equal(note.params.meta.file, 'src/a.ts');
  assert.equal(note.params.meta.line, '1');
  assert.match(note.params.content, /^why is `a` a hub\?/);
  assert.match(note.params.content, /pagerank=0\.2500/);
  assert.doesNotMatch(note.params.content, /bad-key/);
  const user = await ws.waitFor((e) => e.type === 'chat' && e.role === 'user');
  await ws.waitFor((e) => e.type === 'chat' && e.id === user.id && e.state === 'delivered');
  const sent = await c.call('tools/call', { name: 'reply', arguments: { chat_id: note.params.meta.chat_id, text: 'See src/a.ts:1' } });
  assert.equal(sent.result.content[0].text, 'sent');
  const agent = await ws.waitFor((e) => e.type === 'chat' && e.role === 'agent');
  assert.equal(agent.replyTo, user.id);
  await ws.waitFor((e) => e.type === 'chat' && e.id === user.id && e.state === 'replied');
  const unknown = await c.call('tools/call', { name: 'reply', arguments: { chat_id: 'nope', text: 'x' } });
  assert.equal(unknown.result.isError, true);
  ws.close();
  c.child.stdin.end();
  await new Promise((ok) => c.child.on('exit', ok));
  assert.ok(c.lines.every((m) => m.jsonrpc === '2.0'), 'stdout carries only JSON-RPC');
  assert.equal(existsSync(join(f.out, 'server.json')), false, 'server.json is removed on shutdown');
  rmSync(f.dir, { recursive: true, force: true });
});

test('inbox mode: chat lands in chat-inbox.jsonl, /reply needs the token, reply.mjs answers', { timeout: 30000 }, async (t) => {
  const f = fixture();
  const s = await startServer({ out: f.out, replay: true, open: false });
  t.after(() => { rmSync(f.dir, { recursive: true, force: true }); return s.close(); });
  const info = JSON.parse(readFileSync(join(f.out, 'server.json'), 'utf8'));
  assert.deepEqual({ url: info.url, token: info.token, port: info.port, pid: info.pid }, { url: s.url, token: s.token, port: new URL(s.url).port * 1, pid: process.pid });
  await assert.rejects(connect(s.url, 'http://evil.example'), (e) => e.status === 403);
  const ws = await connect(s.url);
  t.after(() => ws.close());
  ws.send({ op: 'hello', since: 0 });
  ws.send({ op: 'chat', text: 'what calls this?', context: { file: 'src/a.ts', line: 1 } });
  const user = await ws.waitFor((e) => e.type === 'chat' && e.role === 'user');
  await ws.waitFor((e) => e.type === 'chat' && e.id === user.id && e.state === 'delivered');
  const inbox = readFileSync(join(f.out, 'chat-inbox.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.equal(inbox.length, 1);
  assert.equal(inbox[0].chat_id, user.id);
  assert.equal(inbox[0].text, 'what calls this?');
  assert.equal(inbox[0].file, 'src/a.ts');

  assert.equal((await post(s.url, '/reply', { text: 'x' }, '')).status, 403);
  assert.equal((await post(s.url, '/reply', { text: 'x' }, 'f'.repeat(32))).status, 403);
  assert.equal((await fetch(new URL(`/reply?t=${s.token}`, s.url))).status, 405);
  assert.equal((await post(s.url, '/reply', { text: 'x', chat_id: 'c999' })).status, 404);
  const ok = await post(s.url, '/reply', { text: 'Nothing yet: `a` has no callers.', chat_id: user.id });
  assert.equal(ok.status, 200);
  const agent = await ws.waitFor((e) => e.type === 'chat' && e.role === 'agent');
  assert.equal(agent.text, 'Nothing yet: `a` has no callers.');

  const cli = await replyCli(f.out, 'second answer');
  assert.equal(cli.status, 0, cli.stderr);
  assert.equal(JSON.parse(cli.stdout).reply.ok, true);
  await ws.waitFor((e) => e.type === 'chat' && e.role === 'agent' && e.text === 'second answer');
  const saved = readFileSync(join(f.out, 'events.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(saved.filter((e) => e.type === 'chat' && e.role).map((e) => e.role), ['user', 'agent', 'agent'], 'chat events are saved even in a replay');
  ws.close();
  await s.close();
  assert.equal(existsSync(join(f.out, 'server.json')), false);
  const gone = await replyCli(f.out, 'late');
  assert.equal(gone.status, 3);
  assert.match(gone.stderr, /no server\.json/);
  rmSync(f.dir, { recursive: true, force: true });
});

test('refreshes run one at a time: requests during a run coalesce into one follow-up', { timeout: 20000 }, async (t) => {
  const f = fixture();
  let running = 0, most = 0, runs = 0;
  const gates = [];
  const refreshRun = async ({ root, out, emit }) => {
    assert.equal(root, f.repo);
    assert.equal(out, f.out);
    runs++;
    running++;
    most = Math.max(most, running);
    emit({ type: 'note', level: 'info', text: `refresh ${runs}` });
    await new Promise((ok) => gates.push(ok));
    running--;
  };
  const s = await startServer({ out: f.out, replay: true, open: false, refreshRun });
  t.after(() => { rmSync(f.dir, { recursive: true, force: true }); return s.close(); });
  const first = s.refresh();
  const second = s.refresh();
  const third = s.refresh();
  assert.equal(second, third, 'later requests join the queued follow-up');
  while (gates.length < 1) await new Promise((ok) => setImmediate(ok));
  gates.shift()();
  assert.equal((await first).ok, true);
  while (gates.length < 1) await new Promise((ok) => setImmediate(ok));
  gates.shift()();
  assert.equal((await second).ok, true);
  assert.deepEqual({ runs, most }, { runs: 2, most: 1 });
  const res = await post(s.url, '/refresh', {}, '');
  assert.equal(res.status, 403);
  const saved = readFileSync(join(f.out, 'events.jsonl'), 'utf8');
  assert.match(saved, /refresh 2/, 'refresh events are saved even in a replay');
  assert.match(saved, /one more will run after it/);
});

test('the channel points at nodes: query_graph emits a query event, reply nodes resolve to ids', { timeout: 30000 }, async (t) => {
  const f = fixture();
  const c = channel(f.dir);
  t.after(() => { c.child.kill(); rmSync(f.dir, { recursive: true, force: true }); });
  await c.call('initialize', { protocolVersion: '2025-06-18', capabilities: {} });
  const opened = await c.call('tools/call', { name: 'open_graph', arguments: { result: f.out } });
  const ws = await connect(JSON.parse(opened.result.content[0].text).url);
  t.after(() => ws.close());
  ws.send({ op: 'hello', since: 0 });
  await ws.waitFor((e) => e.type === 'run');
  const q = await c.call('tools/call', { name: 'query_graph', arguments: { query: 'impact src/a.ts' } });
  assert.notEqual(q.result.isError, true, q.result.content[0].text);
  assert.match(q.result.content[0].text, /^1 file depends on src\/a\.ts transitively/);
  assert.match(q.result.content[0].text, /src\/b\.ts:1/);
  const event = await ws.waitFor((e) => e.type === 'query');
  assert.deepEqual([event.ok, event.op, event.from, event.nodes], [true, 'impact', 'agent', ['src/a.ts', 'src/b.ts']]);
  assert.match(event.id, /^q\d+$/);
  const amb = await c.call('tools/call', { name: 'query_graph', arguments: { query: 'impact a.ts' } });
  assert.equal(amb.result.isError, true);
  assert.match(amb.result.content[0].text, /impact src\/a\.ts/);
  ws.send({ op: 'chat', text: 'who uses a?', context: { file: 'src/a.ts', line: 1 } });
  const note = await c.waitFor((m) => m.method === 'notifications/claude/channel');
  const bad = await c.call('tools/call', { name: 'reply', arguments: { chat_id: note.params.meta.chat_id, text: 'b does', nodes: ['a.ts'] } });
  assert.equal(bad.result.isError, true);
  assert.match(bad.result.content[0].text, /cannot point at a\.ts/);
  assert.match(bad.result.content[0].text, /lib\/a\.ts/);
  const sent = await c.call('tools/call', { name: 'reply', arguments: { chat_id: note.params.meta.chat_id, text: 'b does', nodes: ['b.ts', 'src/a.ts'] } });
  assert.match(sent.result.content[0].text, /^sent; pointing at 2 nodes/);
  const agent = await ws.waitFor((e) => e.type === 'chat' && e.role === 'agent');
  assert.deepEqual(agent.nodes, ['src/b.ts', 'src/a.ts']);
  const wiki = await c.call('tools/call', { name: 'wiki_note', arguments: { slug: 'x', text: '' } });
  assert.equal(wiki.result.isError, true, 'an empty note is refused (or the wiki is unavailable)');
  ws.close();
  c.child.stdin.end();
  await new Promise((ok) => c.child.on('exit', ok));
  assert.ok(c.lines.every((m) => m.jsonrpc === '2.0'), 'stdout carries only JSON-RPC');
});

test('POST /query and /wiki need the token; queries answer over HTTP, the socket and reply.mjs; reply --nodes', { timeout: 30000 }, async (t) => {
  const f = fixture();
  const s = await startServer({ out: f.out, replay: true, open: false });
  t.after(() => { rmSync(f.dir, { recursive: true, force: true }); return s.close(); });
  const ws = await connect(s.url);
  t.after(() => ws.close());
  ws.send({ op: 'hello', since: 0 });
  assert.equal((await post(s.url, '/query', { query: 'deps b.ts' }, '')).status, 403);
  assert.equal((await post(s.url, '/query', { query: 'deps b.ts' }, 'f'.repeat(32))).status, 403);
  assert.equal((await post(s.url, '/wiki', { action: 'read', slug: 'x' }, '')).status, 403);
  assert.equal((await post(s.url, '/wiki', { action: 'nope' })).status, 400);
  assert.equal((await post(s.url, '/query', {})).status, 400);
  const res = await post(s.url, '/query', { query: 'deps b.ts' });
  assert.equal(res.status, 200);
  const r = await res.json();
  assert.deepEqual([r.ok, r.rows.map((x) => x.id)], [true, ['src/a.ts']]);
  await ws.waitFor((e) => e.type === 'query' && e.id === r.id);
  ws.send({ op: 'query', text: 'top pagerank 1' });
  const fromPage = await ws.waitFor((e) => e.type === 'query' && e.from === 'viewer');
  assert.deepEqual([fromPage.ok, fromPage.rows.length], [true, 1]);
  const cli = await replyCli('query', f.out, 'path', 'b.ts', 'src/a.ts');
  assert.equal(cli.status, 0, cli.stderr);
  assert.deepEqual(JSON.parse(cli.stdout).nodes, ['src/b.ts', 'src/a.ts']);
  const amb = await replyCli('query', f.out, 'impact', 'a.ts');
  assert.equal(amb.status, 1);
  assert.equal(JSON.parse(amb.stdout).candidates.length, 2);
  const refused = await post(s.url, '/reply', { text: 'x', nodes: ['nope.ts'] });
  assert.equal(refused.status, 400);
  assert.equal((await refused.json()).problems[0].arg, 'nope.ts');
  const pointed = await replyCli(f.out, 'see these', '--nodes', 'b.ts,src/a.ts');
  assert.equal(pointed.status, 0, pointed.stderr);
  assert.deepEqual(JSON.parse(pointed.stdout).reply.nodes, ['src/b.ts', 'src/a.ts']);
  const agent = await ws.waitFor((e) => e.type === 'chat' && e.role === 'agent' && e.text === 'see these');
  assert.deepEqual(agent.nodes, ['src/b.ts', 'src/a.ts']);
  const saved = readFileSync(join(f.out, 'events.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.equal(saved.filter((e) => e.type === 'query').length, 4, 'query events are saved even in a replay');
});
