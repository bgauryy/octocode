// Live viewer: one WebSocket, every event folded into the store, the graph and panels redrawn per frame.
import type { FileMetrics } from '../runtime/types.ts';
import { orderedInbox } from './live-connection.ts';
import { createGraph } from './live-graph.ts';
import { createPanels } from './live-panels.ts';
import { createChat } from './live-chat.ts';
import { createQuery } from './live-query.ts';
import { createWiki } from './live-wiki.ts';
import { byId } from './live-dom.ts';
import type {
  AskApi,
  CallWitness,
  ChunkState,
  EdgeKind,
  EdgeWitness,
  EventType,
  FeedLevel,
  LiveEvent,
  LiveState,
  LoggedEvent,
  MetricsPayload,
  Op,
  PhaseState,
  QueryAnswer,
  ViewCall,
  ViewNode,
  Via,
} from './live-state.ts';
import { fileOf } from '../ids.ts';

const token = new URLSearchParams(location.search).get('t');
const VIA_RANK: Record<Via, number> = {
  both: 4,
  lsp: 3,
  graph: 2,
  stitched: 1,
};
// An unknown provenance ranks below every known one.
const rank = (via: Via) => VIA_RANK[via] ?? 0;

export const S: LiveState = {
  offline: Boolean(document.getElementById('saved-events')),
  run: null,
  root: '',
  phases: new Map(),
  chunks: new Map(),
  progress: new Map(),
  nodes: new Map(),
  edges: new Map(),
  calls: new Map(),
  symbols: new Map(),
  metrics: null,
  stitched: null,
  lspStats: null,
  done: null,
  error: null,
  tools: [],
  toolsById: new Map(),
  toolSummary: new Map(),
  notes: [],
  ast: new Map(),
  lspResults: new Map(),
  skeletonTotal: 0,
  lastSeq: -1,
  t: 0,
  selected: null,
  highlight: new Set(),
  groupSlot: new Map(),
  verification: null,
};

const listeners: {
  graph: () => void;
  panels: () => void;
  feed: (text: string, level: FeedLevel) => void;
} = { graph: () => {}, panels: () => {}, feed: () => {} };
const touch = (graph = true) => {
  if (graph) listeners.graph();
  listeners.panels();
};

function node(id: string, extra: Partial<ViewNode> = {}) {
  let n = S.nodes.get(id);
  if (!n) {
    n = {
      id,
      path: id,
      group: id.includes('/') ? id.split('/')[0] : '.',
      label: id.split('/').pop() ?? id,
      inGraph: false,
      kind: 'file',
      fanIn: 0,
      fanOut: 0,
      born: performance.now(),
      roles: [],
    };
    S.nodes.set(id, n);
  }
  return Object.assign(n, extra);
}

// File-level edge; calls fold their symbol pairs into it with the strongest provenance seen.
function edge(kind: EdgeKind, source: string, target: string, w: EdgeWitness) {
  const key = `${kind}|${source}|${target}`;
  let e = S.edges.get(key);
  if (!e) {
    e = {
      key,
      kind,
      source,
      target,
      lines: new Set(),
      count: 0,
      confidence: w.confidence ?? 'high',
      via: w.via ?? 'graph',
      test: true,
      born: performance.now(),
      crossChunk: !!w.crossChunk,
      fromStitch: !!w.fromStitch,
      counted: kind === 'imports' && !w.test,
    };
    S.edges.set(key, e);
    if (e.counted) {
      node(source).fanOut++;
      node(target).fanIn++;
    }
  }
  e.count += w.count ?? 1;
  e.test &&= !!w.test;
  for (const l of w.lines ?? (w.line ? [w.line] : [])) e.lines.add(l);
  if (w.via && rank(w.via) > rank(e.via)) e.via = w.via;
  return e;
}

function addCall(c: CallWitness) {
  const key = `${c.source}>${c.target}`;
  const cur: ViewCall = S.calls.get(key) ?? {
    source: c.source,
    target: c.target,
    lines: new Set(),
    via: c.via ?? 'graph',
    confidence: c.confidence,
    crossChunk: !!c.crossChunk,
    fromStitch: !!c.fromStitch,
    file: fileOf(c.source),
  };
  for (const l of c.lines ?? (c.line ? [c.line] : [])) cur.lines.add(l);
  if (c.via && rank(c.via) > rank(cur.via)) cur.via = c.via;
  S.calls.set(key, cur);
  const [a, b] = [fileOf(c.source), fileOf(c.target)];
  if (a !== b)
    edge('calls', a, b, { ...c, lines: c.lines ?? (c.line ? [c.line] : []) });
}

// A stitched event carries the complete set of edges between chunks: it replaces the previous one's contribution
// (a refresh re-sends it), so counts and lines are never added twice.
function dropStitched() {
  for (const [k, e] of S.edges) {
    if (!e.fromStitch) continue;
    if (e.counted) {
      node(e.source).fanOut--;
      node(e.target).fanIn--;
    }
    S.edges.delete(k);
  }
  for (const [k, c] of S.calls) if (c.fromStitch) S.calls.delete(k);
}

function onRows(e: LiveEvent<'rows'>) {
  if (e.kind === 'files')
    for (const f of e.rows)
      node(f.id, {
        inGraph: true,
        language: f.language,
        roles: f.roles,
        chunk: e.chunk,
      });
  else if (e.kind === 'symbols')
    for (const s of e.rows) {
      const list = S.symbols.get(s.file) ?? [];
      list.push(s);
      S.symbols.set(s.file, list);
    }
  else
    for (const w of e.rows) {
      if (w.edge === 'imports' && !w.to.startsWith('pkg:'))
        edge('imports', w.from, w.to, w);
      else if (w.edge === 'calls')
        addCall({
          source: w.from,
          target: w.to,
          line: w.line,
          confidence: w.confidence,
          via: 'graph',
        });
    }
  const c = S.chunks.get(e.chunk);
  if (c) {
    c.rows ??= {};
    c.rows[e.kind] = (c.rows[e.kind] ?? 0) + e.rows.length;
  }
}

function onTool(e: LiveEvent<'tool'>) {
  const cur = S.toolsById.get(e.id);
  if (!cur) {
    S.toolsById.set(e.id, e);
    S.tools.push(e);
  } else Object.assign(cur, e);
  const s = S.toolSummary.get(e.tool) ?? {
    tool: e.tool,
    calls: 0,
    pages: 0,
    rows: 0,
    ms: 0,
    max: 0,
    bytes: 0,
    errors: 0,
    running: 0,
  };
  if (e.state === 'start') {
    s.calls++;
    s.running++;
    if (e.page !== 'first') s.pages++;
  } else {
    s.running = Math.max(0, s.running - 1);
    s.ms += e.ms ?? 0;
    s.max = Math.max(s.max, e.ms ?? 0);
    s.bytes += e.bytes ?? 0;
    s.rows += e.rows ?? 0;
    if (e.state === 'error' || e.error) s.errors++;
  }
  S.toolSummary.set(e.tool, s);
}

const feed = (text: string, level: FeedLevel = 'info') => {
  if (!S.replaying) listeners.feed(text, level);
};

// Each listed file's planned chunk (deepest chunk path containing it), so the graph can show what is being ingested.
// Group colors follow the entity, not its rank: the seven largest groups (packages, or folders for one chunk) at plan time keep slots 1-7 for the
// whole session, every other folder is "Other" (the validated palette has seven adjacent-safe slots).
function assignSlots() {
  if (S.groupSlot.size) return;
  const sizes = new Map<string, number>();
  for (const n of S.nodes.values())
    sizes.set(n.group, (sizes.get(n.group) ?? 0) + 1);
  [...sizes]
    .toSorted((a, b) => b[1] - a[1])
    .slice(0, 7)
    .forEach(([g], i) => S.groupSlot.set(g, i + 1));
}

function assignChunks() {
  const chunks = [...S.chunks.values()]
    .filter(c => c.state !== 'split')
    .toSorted((a, b) => b.path.length - a.path.length);
  const label = (c: ChunkState) =>
    c.package?.split('/').pop() ??
    (c.path === '.'
      ? (S.run?.name ?? 'root')
      : c.path.split('/').slice(-2).join('/'));
  for (const n of S.nodes.values()) {
    const c = chunks.find(
      x => x.path === '.' || n.path.startsWith(`${x.path}/`)
    );
    n.planChunk = c?.id;
    // With several chunks, a node clusters and colors by its package (chunk); one chunk keeps top-level folders.
    if (c && chunks.length > 1) n.group = label(c);
  }
}

function chunkFeed(c: ChunkState, e: LiveEvent<'chunk'>) {
  if (e.state === 'done')
    feed(
      `${c.path ?? e.id}: ${e.counts?.files ?? 0} files, ${e.counts?.imports ?? 0} imports, ${e.counts?.calls ?? 0} calls`
    );
  else if (e.state === 'split')
    feed(
      `${c.path} too broad for one snapshot: split into ${e.into.length}`,
      'warn'
    );
  else if (e.state === 'error') feed(`${c.path}: ${e.error}`, 'bad');
}

function onChunk(e: LiveEvent<'chunk'>) {
  // A chunk the plan never listed is known by its id alone.
  const c = S.chunks.get(e.id) ?? ({ id: e.id } as ChunkState);
  if (e.state === 'ingesting' && c.state !== 'ingesting') c.startT = e.t;
  Object.assign(c, { state: e.state, error: e.error });
  if (e.ms != null) c.ms = e.ms;
  if (e.counts != null) c.counts = e.counts;
  if (e.snapshot != null) c.snapshot = e.snapshot;
  if (e.build != null) c.build = e.build;
  for (const p of e.into ?? []) S.chunks.set(p.id, { ...p, state: 'queued' });
  S.chunks.set(e.id, c);
  if (e.into) assignChunks();
  chunkFeed(c, e);
}

function onMetrics(e: MetricsPayload) {
  S.lspStats = e.callStats;
  S.metrics = e;
  const columns = Object.entries(e.columns);
  e.ids.forEach((id, i) => {
    const n = S.nodes.get(id);
    // One value per column, and the columns are FileMetrics' keys.
    if (n)
      n.metrics = Object.fromEntries(
        columns.map(([k, col]) => [k, col[i]])
      ) as unknown as FileMetrics;
  });
  // Every drawn edge between two files takes that pair's weight (graph-model.ts rule).
  const w = new Map(
    e.edges.source.map((u, k) => [
      `${e.ids[u]}\0${e.ids[e.edges.target[k]]}`,
      e.edges.weight[k],
    ])
  );
  for (const x of S.edges.values()) x.w = w.get(`${x.source}\0${x.target}`);
  feed(
    `metrics: ${e.stats.nodes} nodes, ${e.stats.cycles} cycles, ${e.stats.communities} communities`
  );
}

// Import degrees of the touched files from the edges left, counting production imports as edge() does.
function resetDegrees(touched: Set<string>) {
  for (const id of touched) {
    const n = S.nodes.get(id);
    if (n) {
      n.fanIn = 0;
      n.fanOut = 0;
    }
  }
  for (const x of S.edges.values()) {
    if (x.kind !== 'imports' || x.test) continue;
    const [a, b] = [x.source, x.target];
    if (touched.has(a)) node(a).fanOut++;
    if (touched.has(b)) node(b).fanIn++;
  }
}

type Handlers = { [K in EventType]: (e: LiveEvent<K>) => void };
const handlers: Handlers = {
  chat: e => chat.add(e),
  query: e => query.add(e),
  wiki: e => wiki.add(e),
  wikiPage: e => wiki.add(e),
  run: e => {
    S.run = e;
    S.root = e.root;
    document.title = `${e.name} · Graph live`;
  },
  phase: e => {
    if (e.lspState && S.lspStats)
      S.lspStats = { ...S.lspStats, lsp: e.lspState };
    const p: Pick<PhaseState, 'id' | 'start'> = S.phases.get(e.id) ?? {
      id: e.id,
    };
    if (e.state === 'start') p.start = e.t;
    S.phases.set(
      e.id,
      Object.assign(p, {
        state: e.state === 'start' ? 'running' : e.state,
        ms: e.state === 'start' ? null : e.t - (p.start ?? e.t),
        note: e.note,
        info: e,
      })
    );
  },
  skeleton: e => {
    for (const p of e.files) node(p);
    if (e.total) S.skeletonTotal = e.total;
  },
  plan: e => {
    for (const c of e.chunks) S.chunks.set(c.id, { ...c, state: 'queued' });
    assignChunks();
    assignSlots();
    feed(`${e.chunks.length} chunk${e.chunks.length > 1 ? 's' : ''} planned`);
  },
  chunk: onChunk,
  rows: onRows,
  progress: e => S.progress.set(e.phase, e),
  stitched: e => {
    S.stitched = e;
    dropStitched();
    for (const p of e.packages)
      node(p.id, {
        kind: 'package',
        inGraph: true,
        label: p.label,
        chunk: p.chunk,
        manifest: p.manifest,
      });
    for (const i of e.imports)
      if (S.nodes.has(i.source))
        edge('imports', i.source, i.target, {
          ...i,
          via: 'stitched',
          fromStitch: true,
        });
    for (const c of e.calls)
      addCall({ ...c, via: 'stitched', fromStitch: true });
    feed(
      `stitched ${e.imports.length} imports and ${e.calls.length} calls between chunks`
    );
  },
  'graph-ready': e => {
    onMetrics(e);
    feed(
      e.callStats.lsp === 'pending'
        ? 'Graph ready for queries. LSP confirmation pending.'
        : 'Graph ready for queries.'
    );
  },
  lsp: e => {
    for (const u of e.update) {
      const call = S.calls.get(`${u.source}>${u.target}`);
      if (call) call.via = u.via;
      const pair = S.edges.get(`calls|${fileOf(u.source)}|${fileOf(u.target)}`);
      if (pair && VIA_RANK[u.via] > VIA_RANK[pair.via]) pair.via = u.via;
    }
    for (const a of e.added) addCall(a);
    S.lspStats = e.stats;
  },
  verify: e => {
    for (const r of e.rows) {
      const n = S.nodes.get(r.id);
      if (n) n.check = r;
    }
    S.verification = e.summary;
  },
  metrics: onMetrics,
  tool: onTool,
  note: e => {
    S.notes.push(e);
    feed(e.text, e.level);
  },
  ast: e => S.ast.set(e.file, e),
  lspResult: e => S.lspResults.set(JSON.stringify(e.req), e),
  done: e => {
    S.done = e;
    feed(`done in ${(e.ms / 1000).toFixed(1)} s`, 'ok');
  },
  error: e => {
    S.error = e;
    feed(e.message, 'bad');
  },
  // A refresh rebuilds this chunk: drop the graph data of its files (nodes stay, so positions stay); its rows follow.
  'chunk-reset': e => {
    const files = new Set(e.files);
    const touched = new Set(e.files);
    for (const [k, x] of S.edges) {
      const [a, b] = [fileOf(x.source), fileOf(x.target)];
      if (files.has(a)) {
        S.edges.delete(k);
        touched.add(a);
        touched.add(b);
      }
    }
    for (const [k, c] of S.calls)
      if (files.has(fileOf(c.source))) S.calls.delete(k);
    for (const f of files) {
      S.symbols.delete(f);
      const n = S.nodes.get(f);
      if (n) n.inGraph = false;
    }
    const c = S.chunks.get(e.id);
    if (c) c.rows = {};
    resetDegrees(touched);
  },
  refreshed: e => {
    const n = e.chunks.length,
      s = `${(e.ms / 1000).toFixed(1)} s`;
    const files = `${e.changed?.length ?? 0} changed, ${e.added?.length ?? 0} added, ${e.removed?.length ?? 0} removed files`;
    feed(
      n
        ? `refreshed ${n} chunk${n > 1 ? 's' : ''} (${e.chunks.join(', ')}): ${files} in ${s}`
        : `refresh: nothing changed (${s})`,
      n ? 'ok' : 'info'
    );
    touch();
  },
};
// The server and a saved log are this package's own producers (src/runtime): an event of a type the viewer
// handles carries that type's payload.
const isHandled = (e: LoggedEvent): e is LoggedEvent & LiveEvent =>
  Object.hasOwn(handlers, e.type);
function dispatch<K extends EventType>(e: LiveEvent<K>) {
  const handle: Handlers[K] = handlers[e.type];
  handle(e);
}

const GRAPH_EVENTS = new Set([
  'skeleton',
  'plan',
  'stitched',
  'lsp',
  'chunk-reset',
  'graph-ready',
  'verify',
  'metrics',
]);
function apply(events: LoggedEvent[], replay = false) {
  let graph = false;
  S.replaying = replay;
  for (const e of events) {
    if (e.seq <= S.lastSeq) continue;
    S.t = Math.max(S.t, e.t ?? 0);
    if (isHandled(e)) {
      if (e.type === 'rows')
        graph ||= e.kind !== 'symbols' && e.rows.length > 0;
      if (e.type === 'chunk')
        graph ||= Boolean(e.into || S.chunks.get(e.id)?.state !== e.state);
      dispatch(e);
    }
    S.lastSeq = e.seq;
    graph ||= GRAPH_EVENTS.has(e.type);
  }
  S.replaying = false;
  S.clockAt = performance.now();
  touch(graph);
}

/** A frame from serve.ts: a batch of logged events (the replay after hello, or a burst) or one event. */
interface Batch {
  type: 'batch';
  events: LoggedEvent[];
  replay?: boolean;
}
const isBatch = (m: Batch | LoggedEvent): m is Batch => m.type === 'batch';

let socket: WebSocket | undefined,
  retry = 500;
// Binary frames are gzipped batches; decoding is asynchronous, so one promise chain keeps them in arrival order.
const decode = async (data: string | ArrayBuffer) =>
  typeof data === 'string'
    ? data
    : new Response(
        new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'))
      ).text();
const receive = (text: string) => {
  const msg = JSON.parse(text) as Batch | LoggedEvent;
  apply(isBatch(msg) ? msg.events : [msg], Boolean(msg.replay));
};
// Set by openSavedGraph before the saved page takes any input.
let offlineQuery: (text: string) => void;
export function openSavedGraph(onQuery: (text: string) => void) {
  offlineQuery = onQuery;
  return {
    events: (events: LoggedEvent[]) => apply(events, true),
    answer: (result: QueryAnswer) =>
      query.add({ ...result, type: 'query', from: 'viewer' }),
  };
}
export function send(op: Op) {
  if (S.offline && op.op === 'query') offlineQuery(op.text);
  if (socket?.readyState === 1) socket.send(JSON.stringify(op));
}
function connect() {
  const dot = byId('conn');
  const current = (socket = new WebSocket(
    `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws?t=${encodeURIComponent(String(token))}`
  ));
  socket.addEventListener('open', () => {
    retry = 500;
    dot.className = 'dot live';
    dot.title = 'connected';
    send({ op: 'hello', since: S.lastSeq + 1 });
  });
  socket.binaryType = 'arraybuffer';
  const inbox = orderedInbox(decode, receive, error => {
    feed(
      `Could not read the event batch. Reconnecting: ${(error as Error).message}`,
      'bad'
    );
    current.close();
  });
  current.addEventListener('message', (m: MessageEvent<string | ArrayBuffer>) =>
    inbox.push(m.data)
  );
  current.addEventListener('close', () => {
    inbox.close();
    dot.className = 'dot down';
    dot.title = 'disconnected: retrying';
    setTimeout(connect, retry);
    retry = Math.min(retry * 2, 8000);
  });
}

// Requests answered over the socket; the answer arrives as an event and lands in the store.
export const ask: AskApi = {
  ast: file => {
    if (!S.offline && !S.ast.has(file)) {
      S.ast.set(file, { state: 'start' });
      send({ op: 'ast', file });
    }
  },
  lsp: req => {
    const k = JSON.stringify(req);
    if (!S.lspResults.has(k)) {
      S.lspResults.set(k, { state: 'start', req });
      send({ op: 'lsp', ...req });
    }
    touch(false);
  },
};

export function select(
  id: string | null,
  { focus = false }: { focus?: boolean } = {}
) {
  S.selected = id && S.nodes.has(id) ? id : null;
  if (S.selected) {
    ask.ast(S.selected);
    panels.tab('node');
    if (focus) graph.focus(S.selected);
  }
  // Other views (the chat drawer) follow the selection without importing this module's internals.
  document.dispatchEvent(new CustomEvent('gv:select', { detail: S.selected }));
  graph.redraw();
  touch(false);
}
export function highlight(ids: string[] = []) {
  S.highlight = new Set(ids.filter(id => S.nodes.has(id)));
  if (S.highlight.size) graph.focusAll([...S.highlight]);
  graph.redraw();
}

const graph = createGraph(S, { select });
const panels = createPanels(S, { select, highlight, ask, graph });
listeners.graph = graph.sync;
listeners.panels = panels.schedule;
listeners.feed = panels.feed;
const chat = createChat(S, { send, select, highlight });
const query = createQuery(S, { send, select, highlight });
const wiki = createWiki(S, { send, select, highlight });

try {
  const th = localStorage.getItem('gv-theme');
  if (th) document.documentElement.dataset.theme = th;
} catch {
  /* storage may be blocked */
}
byId('theme').addEventListener('click', () => {
  const dark = document.documentElement.dataset.theme
    ? document.documentElement.dataset.theme === 'dark'
    : !matchMedia('(prefers-color-scheme: light)').matches;
  const theme = dark ? 'light' : 'dark';
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem('gv-theme', theme);
  } catch {
    /* per-viewer convenience only */
  }
  graph.repaint();
  panels.render();
});
byId('cancel').addEventListener('click', () => send({ op: 'cancel' }));
if (S.offline) {
  byId('conn').title = 'Saved graph';
  byId('cancel').hidden = true;
} else connect();
setInterval(() => {
  if (!S.done && !S.error) touch(false);
}, 1000);
