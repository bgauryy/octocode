// Live viewer: one WebSocket, every event folded into the store, the graph and panels redrawn per frame.
import { createGraph } from './live-graph.js';
import { createPanels } from './live-panels.js';
import { createChat } from './live-chat.js';
import { createQuery } from './live-query.js';
import { createWiki } from './live-wiki.js';

const token = new URLSearchParams(location.search).get('t');
const fileOf = (id) => String(id).split('#')[0];
const VIA_RANK = { both: 4, lsp: 3, graph: 2, stitched: 1 };

export const S = {
  run: null, root: '', phases: new Map(), chunks: new Map(), progress: new Map(),
  nodes: new Map(), edges: new Map(), calls: new Map(), symbols: new Map(),
  metrics: null, stitched: null, lspStats: null, done: null, error: null,
  tools: [], toolsById: new Map(), toolSummary: new Map(), notes: [],
  ast: new Map(), lspResults: new Map(), skeletonTotal: 0, lastSeq: -1, t: 0,
  selected: null, highlight: new Set(), groupSlot: new Map(), verification: null,
};

const listeners = { graph: () => {}, panels: () => {}, feed: () => {} };
const touch = (graph = true) => { if (graph) listeners.graph(); listeners.panels(); };

function node(id, extra = {}) {
  let n = S.nodes.get(id);
  if (!n) {
    n = { id, path: id, group: id.includes('/') ? id.split('/')[0] : '.', label: id.split('/').pop(), inGraph: false, kind: 'file', fanIn: 0, fanOut: 0, born: performance.now(), roles: [] };
    S.nodes.set(id, n);
  }
  return Object.assign(n, extra);
}

// File-level edge; calls fold their symbol pairs into it with the strongest provenance seen.
function edge(kind, source, target, w) {
  const key = `${kind}|${source}|${target}`;
  let e = S.edges.get(key);
  if (!e) {
    e = { key, kind, source, target, lines: new Set(), count: 0, confidence: w.confidence ?? 'high', via: w.via ?? 'graph', test: true, born: performance.now(), crossChunk: !!w.crossChunk, fromStitch: !!w.fromStitch, counted: kind === 'imports' && !w.test };
    S.edges.set(key, e);
    if (e.counted) { node(source).fanOut++; node(target).fanIn++; }
  }
  e.count += w.count ?? 1;
  e.test &&= !!w.test;
  for (const l of w.lines ?? (w.line ? [w.line] : [])) e.lines.add(l);
  if ((VIA_RANK[w.via] ?? 0) > (VIA_RANK[e.via] ?? 0)) e.via = w.via;
  return e;
}

function addCall(c) {
  const key = `${c.source}>${c.target}`;
  const cur = S.calls.get(key) ?? { source: c.source, target: c.target, lines: new Set(), via: c.via ?? 'graph', confidence: c.confidence, crossChunk: !!c.crossChunk, fromStitch: !!c.fromStitch, file: fileOf(c.source) };
  for (const l of c.lines ?? (c.line ? [c.line] : [])) cur.lines.add(l);
  if ((VIA_RANK[c.via] ?? 0) > (VIA_RANK[cur.via] ?? 0)) cur.via = c.via;
  S.calls.set(key, cur);
  const [a, b] = [fileOf(c.source), fileOf(c.target)];
  if (a !== b) edge('calls', a, b, { ...c, lines: c.lines ?? (c.line ? [c.line] : []) });
}

// A stitched event carries the complete set of edges between chunks: it replaces the previous one's contribution
// (a refresh re-sends it), so counts and lines are never added twice.
const endpoint = (x) => x?.id ?? x;
function dropStitched() {
  for (const [k, e] of S.edges) {
    if (!e.fromStitch) continue;
    if (e.counted) { node(endpoint(e.source)).fanOut--; node(endpoint(e.target)).fanIn--; }
    S.edges.delete(k);
  }
  for (const [k, c] of S.calls) if (c.fromStitch) S.calls.delete(k);
}

function onRows(e) {
  if (e.kind === 'files') for (const f of e.rows) node(f.id, { inGraph: true, language: f.language, roles: f.roles, chunk: e.chunk });
  else if (e.kind === 'symbols') for (const s of e.rows) {
    const list = S.symbols.get(s.file) ?? [];
    list.push(s);
    S.symbols.set(s.file, list);
  } else for (const w of e.rows) {
    if (w.edge === 'imports' && !w.to.startsWith('pkg:')) edge('imports', w.from, w.to, w);
    else if (w.edge === 'calls') addCall({ source: w.from, target: w.to, line: w.line, confidence: w.confidence, via: 'graph' });
  }
  const c = S.chunks.get(e.chunk);
  if (c) { c.rows ??= {}; c.rows[e.kind] = (c.rows[e.kind] ?? 0) + e.rows.length; }
}

function onTool(e) {
  const cur = S.toolsById.get(e.id);
  if (!cur) { S.toolsById.set(e.id, e); S.tools.push(e); } else Object.assign(cur, e);
  const s = S.toolSummary.get(e.tool) ?? { tool: e.tool, calls: 0, pages: 0, rows: 0, ms: 0, max: 0, bytes: 0, errors: 0, running: 0 };
  if (e.state === 'start') { s.calls++; s.running++; if (e.page !== 'first') s.pages++; }
  else {
    s.running = Math.max(0, s.running - 1);
    s.ms += e.ms ?? 0; s.max = Math.max(s.max, e.ms ?? 0); s.bytes += e.bytes ?? 0; s.rows += e.rows ?? 0;
    if (e.state === 'error' || e.error) s.errors++;
  }
  S.toolSummary.set(e.tool, s);
}

const feed = (text, level = 'info') => { if (!S.replaying) listeners.feed(text, level); };

// Each listed file's planned chunk (deepest chunk path containing it), so the graph can show what is being ingested.
// Group colors follow the entity, not its rank: the seven largest groups (packages, or folders for one chunk) at plan time keep slots 1-7 for the
// whole session, every other folder is "Other" (the validated palette has seven adjacent-safe slots).
function assignSlots() {
  if (S.groupSlot.size) return;
  const sizes = new Map();
  for (const n of S.nodes.values()) sizes.set(n.group, (sizes.get(n.group) ?? 0) + 1);
  [...sizes].toSorted((a, b) => b[1] - a[1]).slice(0, 7).forEach(([g], i) => S.groupSlot.set(g, i + 1));
}

function assignChunks() {
  const chunks = [...S.chunks.values()].filter((c) => c.state !== 'split').toSorted((a, b) => b.path.length - a.path.length);
  const label = (c) => c.package?.split('/').pop() ?? (c.path === '.' ? S.run?.name ?? 'root' : c.path.split('/').slice(-2).join('/'));
  for (const n of S.nodes.values()) {
    const c = chunks.find((x) => x.path === '.' || n.path.startsWith(`${x.path}/`));
    n.planChunk = c?.id;
    // With several chunks, a node clusters and colors by its package (chunk); one chunk keeps top-level folders.
    if (c && chunks.length > 1) n.group = label(c);
  }
}

function chunkFeed(c, e) {
  if (e.state === 'done') feed(`${c.path ?? e.id}: ${e.counts?.files ?? 0} files, ${e.counts?.imports ?? 0} imports, ${e.counts?.calls ?? 0} calls`);
  else if (e.state === 'split') feed(`${c.path} too broad for one snapshot: split into ${e.into.length}`, 'warn');
  else if (e.state === 'error') feed(`${c.path}: ${e.error}`, 'bad');
}

function onChunk(e) {
  const c = S.chunks.get(e.id) ?? { id: e.id };
  if (e.state === 'ingesting' && c.state !== 'ingesting') c.startT = e.t;
  Object.assign(c, { state: e.state, error: e.error });
  for (const k of ['ms', 'counts', 'snapshot', 'build']) if (e[k] != null) c[k] = e[k];
  for (const p of e.into ?? []) S.chunks.set(p.id, { ...p, state: 'queued' });
  S.chunks.set(e.id, c);
  if (e.into) assignChunks();
  chunkFeed(c, e);
}

const handlers = {
  chat: (e) => chat.add(e),
  query: (e) => query.add(e),
  wiki: (e) => wiki.add(e),
  wikiPage: (e) => wiki.add(e),
  run: (e) => { S.run = e; S.root = e.root; document.title = `${e.name} · Graph live`; },
  phase: (e) => {
    const p = S.phases.get(e.id) ?? { id: e.id };
    if (e.state === 'start') p.start = e.t;
    Object.assign(p, { state: e.state === 'start' ? 'running' : e.state, ms: e.state === 'start' ? null : e.t - (p.start ?? e.t), note: e.note, info: e });
    S.phases.set(e.id, p);
  },
  skeleton: (e) => { for (const p of e.files) node(p); if (e.total) S.skeletonTotal = e.total; },
  plan: (e) => { for (const c of e.chunks) S.chunks.set(c.id, { ...c, state: 'queued' }); assignChunks(); assignSlots(); feed(`${e.chunks.length} chunk${e.chunks.length > 1 ? 's' : ''} planned`); },
  chunk: onChunk,
  rows: onRows,
  progress: (e) => S.progress.set(e.phase, e),
  stitched: (e) => {
    S.stitched = e;
    dropStitched();
    for (const p of e.packages) node(p.id, { kind: 'package', inGraph: true, label: p.label, chunk: p.chunk, manifest: p.manifest });
    for (const i of e.imports) if (S.nodes.has(i.source)) edge('imports', i.source, i.target, { ...i, via: 'stitched', fromStitch: true });
    for (const c of e.calls) addCall({ ...c, via: 'stitched', fromStitch: true });
    feed(`stitched ${e.imports.length} imports and ${e.calls.length} calls between chunks`);
  },
  lsp: (e) => {
    for (const u of e.update) addCall({ source: u.source, target: u.target, via: u.via });
    for (const a of e.added) addCall(a);
    S.lspStats = e.stats;
  },
  verify: (e) => {
    for (const r of e.rows) { const n = S.nodes.get(r.id); if (n) n.check = r; }
    S.verification = e.summary;
  },
  metrics: (e) => {
    S.metrics = e;
    e.ids.forEach((id, i) => { const n = S.nodes.get(id); if (n) n.metrics = Object.fromEntries(Object.entries(e.columns).map(([k, col]) => [k, col[i]])); });
    // Every drawn edge between two files takes that pair's weight (graph-model.mjs rule).
    const w = new Map(e.edges.source.map((u, k) => [`${e.ids[u]}\0${e.ids[e.edges.target[k]]}`, e.edges.weight[k]]));
    for (const x of S.edges.values()) x.w = w.get(`${x.source.id ?? x.source}\0${x.target.id ?? x.target}`);
    graph.reheat();
    feed(`metrics: ${e.stats.nodes} nodes, ${e.stats.cycles} cycles, ${e.stats.communities} communities`);
  },
  tool: onTool,
  note: (e) => { S.notes.push(e); feed(e.text, e.level); },
  ast: (e) => S.ast.set(e.file, e),
  lspResult: (e) => S.lspResults.set(JSON.stringify(e.req), e),
  done: (e) => { S.done = e; feed(`done in ${(e.ms / 1000).toFixed(1)} s`, 'ok'); },
  error: (e) => { S.error = e; feed(e.message, 'bad'); },
  // A refresh rebuilds this chunk: drop the graph data of its files (nodes stay, so positions stay); its rows follow.
  'chunk-reset': (e) => {
    const files = new Set(e.files);
    const touched = new Set(e.files);
    for (const [k, x] of S.edges) {
      const [a, b] = [fileOf(x.source.id ?? x.source), fileOf(x.target.id ?? x.target)];
      if (files.has(a)) { S.edges.delete(k); touched.add(a); touched.add(b); }
    }
    for (const [k, c] of S.calls) if (files.has(fileOf(c.source))) S.calls.delete(k);
    for (const f of files) { S.symbols.delete(f); const n = S.nodes.get(f); if (n) n.inGraph = false; }
    const c = S.chunks.get(e.id);
    if (c) c.rows = {};
    handlers['reset-degrees'](touched);
  },
  // Import degrees of the touched files from the edges left, counting production imports as edge() does.
  'reset-degrees': (touched) => {
    for (const id of touched) { const n = S.nodes.get(id); if (n) { n.fanIn = 0; n.fanOut = 0; } }
    for (const x of S.edges.values()) {
      if (x.kind !== 'imports' || x.test) continue;
      const [a, b] = [x.source.id ?? x.source, x.target.id ?? x.target];
      if (touched.has(a)) node(a).fanOut++;
      if (touched.has(b)) node(b).fanIn++;
    }
  },
  refreshed: (e) => {
    const n = e.chunks.length, s = `${(e.ms / 1000).toFixed(1)} s`;
    const files = `${e.changed?.length ?? 0} changed, ${e.added?.length ?? 0} added, ${e.removed?.length ?? 0} removed files`;
    feed(n ? `refreshed ${n} chunk${n > 1 ? 's' : ''} (${e.chunks.join(', ')}): ${files} in ${s}` : `refresh: nothing changed (${s})`, n ? 'ok' : 'info');
    touch();
  },
};

const GRAPH_EVENTS = new Set(['skeleton', 'plan', 'rows', 'stitched', 'lsp', 'chunk', 'verify', 'metrics']);
function apply(events, replay = false) {
  let graph = false;
  S.replaying = replay;
  for (const e of events) {
    if (e.seq <= S.lastSeq) continue;
    S.lastSeq = e.seq;
    S.t = Math.max(S.t, e.t ?? 0);
    handlers[e.type]?.(e);
    graph ||= GRAPH_EVENTS.has(e.type);
  }
  S.replaying = false;
  S.clockAt = performance.now();
  touch(graph);
}

let socket, retry = 500;
// Binary frames are gzipped batches; decoding is asynchronous, so one promise chain keeps them in arrival order.
let inbox = Promise.resolve();
const decode = async (data) => (typeof data === 'string' ? data : new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'))).text());
const receive = (text) => { const msg = JSON.parse(text); apply(msg.type === 'batch' ? msg.events : [msg], Boolean(msg.replay)); };
export function send(op) {
  if (socket?.readyState === 1) socket.send(JSON.stringify(op));
}
function connect() {
  const dot = document.getElementById('conn');
  socket = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws?t=${encodeURIComponent(token)}`);
  socket.addEventListener('open', () => { retry = 500; dot.className = 'dot live'; dot.title = 'connected'; send({ op: 'hello', since: S.lastSeq + 1 }); });
  socket.binaryType = 'arraybuffer';
  socket.addEventListener('message', (m) => { inbox = inbox.then(() => decode(m.data)).then(receive, (error) => feed(`event batch could not be read: ${error.message}`, 'bad')); });
  socket.addEventListener('close', () => { dot.className = 'dot down'; dot.title = 'disconnected: retrying'; setTimeout(connect, retry); retry = Math.min(retry * 2, 8000); });
}

// Requests answered over the socket; the answer arrives as an event and lands in the store.
export const ask = {
  ast: (file) => { if (!S.ast.has(file)) { S.ast.set(file, { state: 'start' }); send({ op: 'ast', file }); } },
  lsp: (req) => { const k = JSON.stringify(req); if (!S.lspResults.has(k)) { S.lspResults.set(k, { state: 'start', req }); send({ op: 'lsp', ...req }); } touch(false); },
};

export function select(id, { focus = false } = {}) {
  S.selected = id && S.nodes.has(id) ? id : null;
  if (S.selected) { ask.ast(S.selected); panels.tab('node'); if (focus) graph.focus(S.selected); }
  // Other views (the chat drawer) follow the selection without importing this module's internals.
  document.dispatchEvent(new CustomEvent('gv:select', { detail: S.selected }));
  graph.redraw();
  touch(false);
}
export function highlight(ids = []) {
  S.highlight = new Set(ids.filter((id) => S.nodes.has(id)));
  if (S.highlight.size) graph.focusAll([...S.highlight]);
  graph.redraw();
}

const graph = createGraph(S, { select, hover: () => {} });
const panels = createPanels(S, { select, highlight, ask, graph });
listeners.graph = graph.sync;
listeners.panels = panels.schedule;
listeners.feed = panels.feed;
const chat = createChat(S, { send, select, highlight });
const query = createQuery(S, { send, select, highlight });
const wiki = createWiki(S, { send, select, highlight });

try { const th = localStorage.getItem('gv-theme'); if (th) document.documentElement.dataset.theme = th; } catch { /* storage may be blocked */ }
document.getElementById('theme').addEventListener('click', () => {
  const dark = document.documentElement.dataset.theme ? document.documentElement.dataset.theme === 'dark' : !matchMedia('(prefers-color-scheme: light)').matches;
  document.documentElement.dataset.theme = dark ? 'light' : 'dark';
  try { localStorage.setItem('gv-theme', document.documentElement.dataset.theme); } catch { /* per-viewer convenience only */ }
  graph.repaint();
  panels.render();
});
document.getElementById('cancel').addEventListener('click', () => send({ op: 'cancel' }));
connect();
setInterval(() => { if (!S.done && !S.error) touch(false); }, 1000);
