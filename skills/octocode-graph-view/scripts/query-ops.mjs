// Graph questions over the weighted file graph of one data.json (query-index.mjs): who depends on a file, what it
// depends on, the strongest path between two files, its cycle and community, symbol callers and callees, rankings
// and text search. Every op returns complete rows; nothing is cut to a page size.
import { QueryError, edgeOf, evidenceLine, evidenceOf, fileOf, indexOf, resolveFile, resolveSymbol } from './query-index.mjs';
const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

const round = (x, digits = 3) => Number(x.toFixed(digits));
const byId = (a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
const plural = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}`;
const metricOf = (I, id, key) => I.file.get(id)?.metrics?.[key];

// A file row: id, label, chunk and location; `line` (when known) points at the evidence inside the file.
export function fileRow(I, id, { line, ...extra } = {}) {
  const f = I.file.get(id);
  return { id, label: f?.label ?? id.split('/').pop(), chunk: f?.chunk ?? null, loc: { path: id, line: line ?? f?.loc?.line ?? 1 }, ...extra };
}

function countBy(rows, key) {
  const counts = {};
  for (const r of rows) counts[r[key] ?? '-'] = (counts[r[key] ?? '-'] ?? 0) + 1;
  return counts;
}

// Breadth-first walk to maxDepth along dependencies (out) or dependents (in). A node reached at depth d keeps the
// heaviest edge from depth d-1 that reaches it.
function walk(I, start, { dir, maxDepth }) {
  const adj = dir === 'in' ? I.G.in : I.G.out;
  const s = I.G.index.get(start);
  const depth = new Map([[s, 0]]);
  const via = new Map();
  let frontier = [s];
  for (let d = 1; frontier.length && d <= maxDepth; d++) {
    const next = [];
    for (const u of frontier) for (let k = adj.offsets[u]; k < adj.offsets[u + 1]; k++) {
      const v = adj.targets[k], w = adj.weights[k];
      const seen = depth.get(v);
      if (seen === undefined) { depth.set(v, d); via.set(v, [u, w]); next.push(v); }
      else if (seen === d && w > via.get(v)[1]) via.set(v, [u, w]);
    }
    frontier = next;
  }
  return { depth, via };
}

// Rows of a walk: depth, the weight of the edge that reached the file, the file it reached through, and that edge's
// evidence line in the depending file.
function walkRows(I, start, { dir, maxDepth }) {
  const { depth, via } = walk(I, start, { dir, maxDepth });
  const found = [];
  const edges = [];
  for (const [v, [u, w]] of via) {
    const [id, through] = [I.ids[v], I.ids[u]];
    const [from, to] = dir === 'in' ? [id, through] : [through, id];
    edges.push([from, to]);
    found.push(fileRow(I, id, { depth: depth.get(v), weight: round(w), through, line: dir === 'in' ? evidenceLine(I, from, to) : undefined }));
  }
  const rows = found.toSorted((a, b) => a.depth - b.depth || b.weight - a.weight || byId(a, b));
  return { rows, edges, nodes: [start, ...rows.map((r) => r.id)] };
}

// Default walks follow strong edges, while metrics count every production edge. So an answer states what `--weak` would
// add: "nothing depends on it" must never be read as a fact the graph's own metrics contradict.
function weakView(I) {
  return I.weak || I.imports ? null : indexOf(I.D, { tests: I.tests, weak: true });
}
function weakAdds(I, id, { dir, maxDepth }, found) {
  const W = weakView(I);
  return W ? walk(W, id, { dir, maxDepth }).via.size - found : 0;
}
const weakNote = (n) => (n > 0 ? `; --weak adds ${plural(n, 'file')} reached only through name-bound calls` : '');

function depthText(byDepth) {
  return Object.entries(byDepth).map(([d, n]) => `depth ${d}: ${n}`).join(', ');
}

export function impact(I, { args, depth }) {
  const id = resolveFile(I, args[0]);
  const r = walkRows(I, id, { dir: 'in', maxDepth: depth ?? Infinity });
  const weak = weakAdds(I, id, { dir: 'in', maxDepth: depth ?? Infinity }, r.rows.length);
  const stats = { byDepth: countBy(r.rows, 'depth'), byChunk: countBy(r.rows, 'chunk'), weakAdds: weak };
  const chunks = Object.keys(stats.byChunk).length;
  const summary = r.rows.length
    ? `${plural(r.rows.length, 'file')} ${r.rows.length === 1 ? 'depends' : 'depend'} on ${id}${depth ? ` within depth ${depth}` : ' transitively'} (${depthText(stats.byDepth)}) across ${plural(chunks, 'chunk')}`
    : `nothing depends on ${id}`;
  return { ...r, stats, summary: `${summary}${weakNote(weak)}` };
}

function neighbors(I, { args, depth }, dir) {
  const id = resolveFile(I, args[0]);
  const maxDepth = depth ?? 1;
  const r = walkRows(I, id, { dir, maxDepth });
  const one = r.rows.length === 1;
  const what = dir === 'in' ? (one ? 'depends on' : 'depend on') : one ? 'is a dependency of' : 'are dependencies of';
  const weak = weakAdds(I, id, { dir, maxDepth }, r.rows.length);
  const stats = { byDepth: countBy(r.rows, 'depth'), byChunk: countBy(r.rows, 'chunk'), weakAdds: weak };
  return { ...r, stats, summary: `${plural(r.rows.length, 'file')} ${what} ${id} within depth ${maxDepth === Infinity ? 'all' : maxDepth}${weakNote(weak)}` };
}
export const deps = (I, q) => neighbors(I, q, 'out');
export const dependents = (I, q) => neighbors(I, q, 'in');

// A binary min-heap of [cost, node].
function heapPush(h, item) {
  h.push(item);
  for (let i = h.length - 1; i > 0;) {
    const p = (i - 1) >> 1;
    if (h[p][0] <= h[i][0]) break;
    [h[p], h[i]] = [h[i], h[p]];
    i = p;
  }
}
function heapPop(h) {
  const first = h[0];
  const last = h.pop();
  if (h.length) {
    h[0] = last;
    for (let i = 0; ;) {
      const [l, r] = [2 * i + 1, 2 * i + 2];
      let m = i;
      if (l < h.length && h[l][0] < h[m][0]) m = l;
      if (r < h.length && h[r][0] < h[m][0]) m = r;
      if (m === i) break;
      [h[m], h[i]] = [h[i], h[m]];
      i = m;
    }
  }
  return first;
}

// Dijkstra with cost 1/weight: the strongest dependency path s → t (node indices), or null.
function strongest(I, s, t) {
  const { n, out } = I.G;
  const dist = new Float64Array(n).fill(Infinity), prev = new Int32Array(n).fill(-1);
  dist[s] = 0;
  const heap = [[0, s]];
  while (heap.length) {
    const [d, u] = heapPop(heap);
    if (d > dist[u]) continue;
    if (u === t) break;
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
      const v = out.targets[k], c = d + 1 / Math.max(out.weights[k], 1e-9);
      if (c < dist[v]) { dist[v] = c; prev[v] = u; heapPush(heap, [c, v]); }
    }
  }
  if (dist[t] === Infinity) return null;
  const route = [t];
  while (route[0] !== s) route.unshift(prev[route[0]]);
  return { path: route.map((i) => I.ids[i]), cost: dist[t] };
}

// One row per file on the path; each carries the hop to the next file: weight, import lines and symbol calls.
function pathRows(I, route, direction) {
  return route.map((id, i) => {
    const next = route[i + 1];
    if (!next) return fileRow(I, id, { step: i, direction });
    const ev = evidenceOf(I, id, next);
    const weight = edgeOf(I, id, next)?.weight ?? 0;
    return fileRow(I, id, { step: i, direction, next, weight: round(weight), line: evidenceLine(I, id, next), evidence: { imports: [...new Set(ev.imports)].toSorted((a, b) => a - b), calls: ev.calls } });
  });
}

function pathResult(I, found, direction) {
  const rows = pathRows(I, found.path, direction);
  const edges = found.path.slice(1).map((id, i) => [found.path[i], id]);
  const total = round(rows.reduce((a, r) => a + (r.weight ?? 0), 0));
  return { rows, edges, nodes: found.path, stats: { hops: edges.length, weight: total, cost: round(found.cost, 6), direction } };
}

export function path(I, { args }) {
  if (args.length < 2) throw new QueryError('path needs two files: path <from> <to>');
  const [a, b] = [resolveFile(I, args[0]), resolveFile(I, args[1])];
  const [s, t] = [I.G.index.get(a), I.G.index.get(b)];
  const found = strongest(I, s, t);
  if (found) {
    const r = pathResult(I, found, 'forward');
    return { ...r, found: true, summary: `${a} → ${b}: ${plural(r.stats.hops, 'hop')}, weight ${r.stats.weight} (strongest path, cost Σ 1/weight ${r.stats.cost})` };
  }
  const back = strongest(I, t, s);
  const W = weakView(I);
  const weakPath = W ? strongest(W, s, t) : null;
  const weakText = weakPath ? `; --weak finds one of ${plural(weakPath.path.length - 1, 'hop')} through name-bound calls` : '';
  if (!back) return { found: false, rows: [], edges: [], nodes: [a, b], stats: { hops: 0, weakHops: weakPath ? weakPath.path.length - 1 : null }, summary: `no path ${a} → ${b}, and none ${b} → ${a}${weakText}` };
  const r = pathResult(I, back, 'reverse');
  return { ...r, found: false, summary: `no path ${a} → ${b}; the reverse ${b} → ${a} exists: ${plural(r.stats.hops, 'hop')}, weight ${r.stats.weight}${weakText}` };
}

function ringTo(prev, s, u) {
  const order = [u];
  while (order[0] !== s) order.unshift(prev.get(order[0]));
  return order;
}

// The shortest cycle through s inside its component: breadth-first from s until an edge returns to s.
function cycleThrough(I, s, members) {
  const { out } = I.G;
  const prev = new Map([[s, -1]]);
  for (let frontier = [s]; frontier.length;) {
    const next = [];
    for (const u of frontier) for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
      const v = out.targets[k];
      if (v === s) return ringTo(prev, s, u);
      if (members.has(v) && !prev.has(v)) { prev.set(v, u); next.push(v); }
    }
    frontier = next;
  }
  return null;
}

// The strongly connected set of `s` and one ring through it (null when s is on no cycle).
function cycleOf(I, s) {
  const { comp, comps } = I.components();
  const members = new Set(comps[comp[s]]);
  return { members, order: cycleThrough(I, s, members) };
}

// No cycle over the walked edges: say whether `--weak` finds one, since metrics count those edges.
function noCycle(I, id, s) {
  const W = weakView(I);
  const weak = W ? cycleOf(W, s) : null;
  const size = weak?.order ? weak.members.size : null;
  const known = size ? (I.D.metrics?.cycles ?? []).find((c) => c.members.includes(id)) : null;
  const note = size ? `; --weak finds it in ${known ? `cycle ${known.id}` : 'a cycle'} of ${plural(size, 'file')} through name-bound calls` : '';
  return { rows: [], edges: [], nodes: [id], stats: { size: 1, weakSize: size }, summary: `${id} is in no dependency cycle${note}` };
}

export function cycle(I, { args }) {
  const id = resolveFile(I, args[0]);
  const s = I.G.index.get(id);
  const { members, order } = cycleOf(I, s);
  if (!order) return noCycle(I, id, s);
  const ids = order.map((i) => I.ids[i]);
  const ring = ids.map((u, i) => [u, ids[(i + 1) % ids.length]]);
  const rows = ring.map(([u, v], i) => fileRow(I, u, { order: i, next: v, weight: round(edgeOf(I, u, v)?.weight ?? 0), line: evidenceLine(I, u, v) }));
  const inRing = new Set(ids);
  const rest = [...members].map((i) => I.ids[i]).filter((u) => !inRing.has(u))
    .toSorted((a, b) => (metricOf(I, b, 'pagerank') ?? 0) - (metricOf(I, a, 'pagerank') ?? 0) || (a < b ? -1 : 1))
    .map((u) => fileRow(I, u, { order: null, pagerank: metricOf(I, u, 'pagerank') ?? null }));
  const known = I.tests ? null : (I.D.metrics?.cycles ?? []).find((c) => c.members.includes(id));
  const stats = { size: members.size, ring: ids.length, cycle: known?.id ?? null, weight: known?.weight ?? null, chunks: [...new Set([...members].map((i) => I.file.get(I.ids[i])?.chunk ?? '-'))].toSorted(byText) };
  const label = known ? `cycle ${known.id}` : 'a cycle';
  return { rows: [...rows, ...rest], edges: ring, nodes: [...members].map((i) => I.ids[i]), stats, summary: `${id} is in ${label} of ${plural(members.size, 'file')} across ${plural(stats.chunks.length, 'chunk')}; one ring through it has ${plural(ids.length, 'file')}` };
}

function communityId(I, arg) {
  if (/^\d+$/.test(String(arg ?? ''))) return Number(arg);
  const id = resolveFile(I, arg);
  const c = metricOf(I, id, 'community');
  if (c == null) throw new QueryError(`${id} has no community: metrics were not computed for it`);
  return c;
}

// Weight from the community's members to each other community (out) and back (in).
function communityLinks(I, inside, id) {
  const links = new Map();
  const add = (c, key, w) => {
    const x = links.get(c) ?? { community: c, out: 0, in: 0 };
    x[key] += w;
    links.set(c, x);
  };
  for (const u of inside) {
    const i = I.G.index.get(u);
    for (const [adj, key] of [[I.G.out, 'out'], [I.G.in, 'in']]) for (let k = adj.offsets[i]; k < adj.offsets[i + 1]; k++) {
      const c = metricOf(I, I.ids[adj.targets[k]], 'community');
      if (c != null && c !== id) add(c, key, adj.weights[k]);
    }
  }
  const label = new Map((I.D.metrics?.communities ?? []).map((c) => [c.id, c.label]));
  return [...links.values()].map((x) => ({ ...x, label: label.get(x.community) ?? null, out: round(x.out), in: round(x.in), weight: round(x.out + x.in) }))
    .toSorted((a, b) => b.weight - a.weight || a.community - b.community);
}

export function community(I, { args }) {
  const id = communityId(I, args[0]);
  const c = (I.D.metrics?.communities ?? []).find((x) => x.id === id);
  if (!c) throw new QueryError(`no community ${id}: data.json lists ${I.D.metrics?.communities?.length ?? 0}`);
  const members = c.members.filter((m) => I.file.has(m)).toSorted((a, b) => (metricOf(I, b, 'pagerank') ?? 0) - (metricOf(I, a, 'pagerank') ?? 0) || (a < b ? -1 : 1));
  const rows = members.map((m, rank) => fileRow(I, m, { rank: rank + 1, pagerank: metricOf(I, m, 'pagerank') ?? null }));
  const links = communityLinks(I, members, id);
  const best = links[0] ? `; strongest link: community ${links[0].community}${links[0].label ? ` (${links[0].label})` : ''}, weight ${links[0].weight}` : '';
  return { rows, edges: [], nodes: members, links, stats: { id, label: c.label, size: c.size, internal: c.internal, external: c.external, links: links.length }, summary: `community ${id} (${c.label}): ${plural(members.length, 'file')}, linked to ${links.length} other communit${links.length === 1 ? 'y' : 'ies'}${best}` };
}

// Symbol-level calls into (callers) or out of (callees) one symbol, with provenance; rows point at the other file.
function symbolCalls(I, { args }, dir) {
  const sym = resolveSymbol(I, args[0]);
  const { byId: symbols } = I.symbols();
  const [mine, other] = dir === 'callers' ? ['target', 'source'] : ['source', 'target'];
  const calls = (I.D.calls ?? []).filter((c) => c[mine] === sym && (I.tests || !c.test));
  const rows = calls.map((c) => {
    const s = symbols.get(c[other]);
    const lines = c.lines ?? (c.line ? [c.line] : []);
    // A caller row opens at the call site; a callee row at the callee's declaration (else the call site).
    const line = dir === 'callers' ? lines[0] : s?.line ?? undefined;
    return fileRow(I, fileOf(c[other]), { line, symbol: c[other], name: s?.name ?? c[other].split('#').pop(), lines, via: c.via ?? 'graph', confidence: c.confidence ?? null, count: c.count ?? 1, test: Boolean(c.test) });
  }).toSorted((a, b) => byId(a, b) || a.loc.line - b.loc.line || (a.symbol < b.symbol ? -1 : 1));
  const file = fileOf(sym);
  const pairs = new Set(rows.map((r) => (dir === 'callers' ? `${r.id}\0${file}` : `${file}\0${r.id}`)));
  const edges = [...pairs].map((k) => k.split('\0')).filter(([u, v]) => u !== v);
  const via = countBy(rows, 'via');
  const files = new Set(rows.map((r) => r.id));
  return { symbol: sym, rows, edges, nodes: [file, ...files], stats: { calls: rows.length, files: files.size, via }, summary: `${plural(rows.length, dir === 'callers' ? 'caller' : 'callee')} of ${sym} in ${plural(files.size, 'file')} (${Object.entries(via).map(([k, n]) => `${k} ${n}`).join(', ') || 'none'})` };
}
export const callers = (I, q) => symbolCalls(I, q, 'callers');
export const callees = (I, q) => symbolCalls(I, q, 'callees');

export const METRICS = { pagerank: 'pagerank', betweenness: 'betweenness', inweight: 'inWeight', outweight: 'outWeight', core: 'core', depth: 'depth' };

// Files ranked by one metric, highest first (ties by path). n: a count or `all`; default 20, said in the summary.
export function top(I, { args }) {
  const key = METRICS[String(args[0] ?? '').toLowerCase()];
  if (!key) throw new QueryError(`top needs a metric: ${Object.values(METRICS).join(', ')}`);
  const ranked = I.ids.filter((id) => typeof metricOf(I, id, key) === 'number');
  if (!ranked.length) throw new QueryError(`no file has ${key}: metrics were not computed for this result`);
  const all = String(args[1] ?? '').toLowerCase() === 'all';
  const n = all ? ranked.length : Math.max(1, Number.parseInt(args[1] ?? '20', 10) || 20);
  const order = ranked.toSorted((a, b) => metricOf(I, b, key) - metricOf(I, a, key) || (a < b ? -1 : 1)).slice(0, n);
  const rows = order.map((id, i) => fileRow(I, id, { rank: i + 1, value: metricOf(I, id, key) }));
  const more = ranked.length > rows.length ? `; \`top ${key} all\` ranks all ${ranked.length}` : '';
  return { metric: key, rows, edges: [], nodes: order, stats: { metric: key, shown: rows.length, ranked: ranked.length }, summary: `top ${rows.length} of ${ranked.length} files by ${key}${more}` };
}

// Files whose path and symbols whose name contain the text (case-insensitive); every match.
export function find(I, { args }) {
  const text = args.join(' ').trim();
  if (!text) throw new QueryError('find needs text');
  const q = text.toLowerCase();
  const files = I.ids.filter((id) => id.toLowerCase().includes(q)).map((id) => fileRow(I, id, { kind: 'file' }));
  const symbols = [...I.symbols().byId.values()]
    .filter((s) => I.file.has(s.file) && s.name.toLowerCase().includes(q))
    .toSorted((a, b) => (a.file < b.file ? -1 : a.file > b.file ? 1 : (a.line ?? 0) - (b.line ?? 0)))
    .map((s) => fileRow(I, s.file, { line: s.line ?? undefined, kind: s.kind, symbol: s.id, name: s.name }));
  const nodes = [...new Set([...files, ...symbols].map((r) => r.id))];
  return { rows: [...files, ...symbols], edges: [], nodes, stats: { files: files.length, symbols: symbols.length }, summary: `${plural(files.length, 'file')} and ${plural(symbols.length, 'symbol')} match "${text}"` };
}
