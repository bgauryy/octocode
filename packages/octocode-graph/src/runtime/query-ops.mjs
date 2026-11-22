// Graph questions over the weighted file graph of one data.json (query-index.mjs): who depends on a file, what it
// depends on, the strongest path between two files, its cycle and community, symbol callers and callees, rankings
// and text search. Every op returns complete rows; nothing is cut to a page size.
import { QueryError, edgeOf, evidenceLine, evidenceOf, fileOf, indexOf, resolveFile, resolveLocation, resolveSymbol } from './query-index.mjs';
import { PatternError, traceRelations } from '#graph-core/relations';
import { prerequisiteOrder } from '#graph-core/algorithms';
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
function walk(I, seeds, { dir, maxDepth }) {
  const adj = dir === 'in' ? I.G.in : I.G.out;
  let frontier = seeds.map(id => I.G.index.get(id));
  const depth = new Map(frontier.map(v => [v, 0]));
  const via = new Map();
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
function walkRows(I, seeds, { dir, maxDepth }) {
  const { depth, via } = walk(I, seeds, { dir, maxDepth });
  const found = [];
  const edges = [];
  for (const [v, [u, w]] of via) {
    const [id, through] = [I.ids[v], I.ids[u]];
    const [from, to] = dir === 'in' ? [id, through] : [through, id];
    edges.push([from, to]);
    found.push(fileRow(I, id, { depth: depth.get(v), weight: round(w), through, line: dir === 'in' ? evidenceLine(I, from, to) : undefined }));
  }
  const rows = found.toSorted((a, b) => a.depth - b.depth || b.weight - a.weight || byId(a, b));
  return { rows, edges, nodes: [...seeds, ...rows.map((r) => r.id)] };
}

// Default walks follow strong edges, while metrics count every production edge. So an answer states what `--weak` would
// add: "nothing depends on it" must never be read as a fact the graph's own metrics contradict.
function weakView(I) {
  return I.weak || I.imports ? null : indexOf(I.D, { tests: I.tests, weak: true });
}
function weakAdds(I, seeds, { dir, maxDepth }, found) {
  const W = weakView(I);
  return W ? walk(W, seeds, { dir, maxDepth }).via.size - found : 0;
}
const weakNote = (n) => (n > 0 ? `; --weak adds ${plural(n, 'file')} reached only through name-bound calls` : '');

function depthText(byDepth) {
  return Object.entries(byDepth).map(([d, n]) => `depth ${d}: ${n}`).join(', ');
}

function seedFiles(I, args) {
  if (!args.length) throw new QueryError('missing file argument');
  return [...new Set(args.map(arg => resolveFile(I, arg)))];
}

// File records carry the saved metadata. A source anchor selects a line in the working file.
export function fileRecords(I, { args }) {
  if (!args.length) throw new QueryError('files needs one or more file paths');
  const reads = new Map();
  const rows = args.map(arg => {
    const { id, line, column } = resolveLocation(I, arg);
    const file = I.file.get(id);
    const loc = { path: file.manifest ?? id, ...file.loc, line: line ?? file.loc?.line ?? 1, ...(column && { column }) };
    const sourcePath = loc.abs ?? loc.path;
    if (!reads.has(sourcePath)) reads.set(sourcePath, new Set());
    if (line) reads.get(sourcePath)?.add(loc.line);
    else reads.set(sourcePath, null); // A whole-file request already includes every cited line.
    return { ...file, loc };
  });
  const nodes = [...new Set(rows.map(row => row.id))];
  const sourceQueries = Array.from(reads, ([sourcePath, lines]) => lines
    ? { path: sourcePath, ranges: Array.from(lines, line => ({ startLine: line, endLine: line })) }
    : { path: sourcePath });
  return { rows, nodes, sourceQueries, summary: `${plural(nodes.length, 'file')} in the snapshot` };
}

export function impact(I, { args, depth }) {
  const seeds = seedFiles(I, args);
  const label = seeds.join(', ');
  const r = walkRows(I, seeds, { dir: 'in', maxDepth: depth ?? Infinity });
  const weak = weakAdds(I, seeds, { dir: 'in', maxDepth: depth ?? Infinity }, r.rows.length);
  const stats = { byDepth: countBy(r.rows, 'depth'), byChunk: countBy(r.rows, 'chunk'), weakAdds: weak };
  const chunks = Object.keys(stats.byChunk).length;
  const summary = r.rows.length
    ? `${plural(r.rows.length, 'file')} ${r.rows.length === 1 ? 'depends' : 'depend'} on ${label}${depth ? ` within depth ${depth}` : ' transitively'} (${depthText(stats.byDepth)}) across ${plural(chunks, 'chunk')}`
    : `nothing depends on ${label}`;
  return { ...r, stats, summary: `${summary}${weakNote(weak)}` };
}

function neighbors(I, { args, depth }, dir) {
  const seeds = seedFiles(I, args);
  const label = seeds.join(', ');
  const maxDepth = depth ?? 1;
  const r = walkRows(I, seeds, { dir, maxDepth });
  const one = r.rows.length === 1;
  const what = dir === 'in' ? (one ? 'depends on' : 'depend on') : one ? 'is a dependency of' : 'are dependencies of';
  const weak = weakAdds(I, seeds, { dir, maxDepth }, r.rows.length);
  const stats = { byDepth: countBy(r.rows, 'depth'), byChunk: countBy(r.rows, 'chunk'), weakAdds: weak };
  return { ...r, stats, summary: `${plural(r.rows.length, 'file')} ${what} ${label} within depth ${maxDepth === Infinity ? 'all' : maxDepth}${weakNote(weak)}` };
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
  return { path: routeIds(I, prev, s, t), cost: dist[t] };
}

function routeIds(I, prev, start, end) {
  const ids = [];
  for (let v = end; ; v = prev[v]) {
    ids.push(I.ids[v]);
    if (v === start) return ids.reverse();
  }
}

// Unit-cost BFS on the same filtered CSR that the weighted solver reads.
function fewestHops(I, s, t) {
  const { n, out } = I.G;
  const prev = new Int32Array(n).fill(-1);
  const queue = [s];
  prev[s] = s;
  for (let head = 0; head < queue.length && prev[t] < 0; head++) {
    const u = queue[head];
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
      const v = out.targets[k];
      if (prev[v] >= 0) continue;
      prev[v] = u;
      if (v === t) break;
      queue.push(v);
    }
  }
  if (prev[t] < 0) return null;
  const route = routeIds(I, prev, s, t);
  return { path: route, cost: route.length - 1 };
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

function pathResult(I, found, direction, objective) {
  const rows = pathRows(I, found.path, direction);
  const edges = found.path.slice(1).map((id, i) => [found.path[i], id]);
  const total = round(rows.reduce((a, r) => a + (r.weight ?? 0), 0));
  return { rows, edges, nodes: found.path, stats: { hops: edges.length, weight: total, cost: round(found.cost, 6), direction, objective } };
}

export function path(I, { args, hops }) {
  const solve = hops ? fewestHops : strongest;
  const objective = hops ? 'hops' : 'strength';
  if (args.length !== 2) throw new QueryError('path needs two files: path <from> <to>');
  const [a, b] = [resolveFile(I, args[0]), resolveFile(I, args[1])];
  const [s, t] = [I.G.index.get(a), I.G.index.get(b)];
  const found = solve(I, s, t);
  if (found) {
    const r = pathResult(I, found, 'forward', objective);
    return { ...r, found: true, summary: `${a} → ${b}: ${plural(r.stats.hops, 'hop')}, weight ${r.stats.weight} (${hops ? 'fewest hops' : `strongest path, cost Σ 1/weight ${r.stats.cost}`})` };
  }
  const back = solve(I, t, s);
  const W = weakView(I);
  const weakPath = W ? solve(W, s, t) : null;
  const weakText = weakPath ? `; --weak finds one of ${plural(weakPath.path.length - 1, 'hop')} through name-bound calls` : '';
  if (!back) return { found: false, rows: [], edges: [], nodes: [a, b], stats: { objective, hops: 0, weakHops: weakPath ? weakPath.path.length - 1 : null }, summary: `no path ${a} → ${b}, and none ${b} → ${a}${weakText}` };
  const r = pathResult(I, back, 'reverse', objective);
  return { ...r, found: false, summary: `no path ${a} → ${b}; the reverse ${b} → ${a} exists: ${plural(r.stats.hops, 'hop')}, weight ${r.stats.weight}${weakText}` };
}

function ringTo(prev, s, u) {
  const order = [u];
  while (u !== s) { u = prev.get(u); order.push(u); }
  return order.reverse();
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

// Stored cycle IDs identify exact member sets from the snapshot projection.
function matchingCycle(I, id, members) {
  const candidate = I.D.metrics?.cycles?.find(c => c.members.includes(id));
  return candidate?.size === members.size && candidate.members.every(u => members.has(I.G.index.get(u))) ? candidate : null;
}

function componentWeight({ out }, members) {
  let weight = 0;
  for (const u of members) for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
    if (members.has(out.targets[k])) weight += out.weights[k];
  }
  return round(weight);
}

// No cycle over the walked edges: say whether `--weak` finds one, since metrics count those edges.
function noCycle(I, id, s) {
  const W = weakView(I);
  const weak = W ? cycleOf(W, s) : null;
  const size = weak?.order ? weak.members.size : null;
  const known = size ? matchingCycle(W, id, weak.members) : null;
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
  const known = matchingCycle(I, id, members);
  const stats = { size: members.size, ring: ids.length, cycle: known?.id ?? null, weight: componentWeight(I.G, members), chunks: [...new Set([...members].map((i) => I.file.get(I.ids[i])?.chunk ?? '-'))].toSorted(byText) };
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
  const calls = I.callsBy(mine).get(sym) ?? [];
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

// Component numbers are local to this answer, increasing from prerequisites to dependents.
export function dependencyOrder(I, { args }) {
  if (args.length !== 1) throw new QueryError('order needs one file');
  const file = resolveFile(I, args[0]);
  const plan = prerequisiteOrder(I.G, I.components(), I.G.index.get(file));
  const rows = plan.groups.flatMap((group, component) => group.map(v => fileRow(I, I.ids[v], { component })));
  const edges = plan.crossings.map(([u, v]) => [I.ids[u], I.ids[v]]);
  const crossings = edges.map(([source, target]) => ({ source, target, evidence: evidenceOf(I, source, target) }));
  return { nodes: rows.map(r => r.id), rows, edges, crossings, stats: { groups: plan.groups.length },
    summary: `${file}: ${plural(rows.length, 'file')} in ${plural(plan.groups.length, 'dependency group')}, prerequisites first. Files in the same group move together.` };
}

export function trace(I, { args, depth }) {
  const [target, ...expression] = args;
  if (!target || !expression.length) throw new QueryError('trace needs a file or file#symbol and a relation pattern');
  if (I.imports) throw new QueryError('Choose IMPORTS in the trace pattern to restrict relation types');
  const index = I.relations();
  const seed = index.has(target) ? target : target.includes('#') ? resolveSymbol(I, target) : resolveFile(I, target);
  let found;
  try { found = traceRelations(index, seed, expression.join(' '), depth ?? Infinity); }
  catch (error) { if (error instanceof PatternError) throw new QueryError(error.message); throw error; }
  const sources = new Map();
  for (const edge of found.steps) if (!sources.has(edge.target) || edge.relation === 'DEFINES') sources.set(edge.target, edge);
  const rows = found.endpoints.map(endpoint => {
    const symbol = I.symbols().byId.get(endpoint);
    const origin = sources.get(endpoint);
    const sourcePath = I.file.has(endpoint) ? endpoint : symbol?.file ?? origin?.path;
    const line = symbol?.line ?? (origin?.path === sourcePath ? origin?.lines[0] : null);
    return { id: sourcePath ?? endpoint, endpoint, label: endpoint, loc: sourcePath ? { path: sourcePath, line: line ?? null } : null };
  });
  const files = new Set([fileOf(seed)]);
  const pairs = new Map();
  for (const step of found.steps) {
    const a = fileOf(step.source), b = fileOf(step.target);
    files.add(a); files.add(b);
    if (a !== b) pairs.set(JSON.stringify([a, b]), [a, b]);
  }
  return { rows, nodes: [...new Set([...files, ...rows.map(r => r.id)])].filter(id => I.file.has(id)), edges: [...pairs.values()].filter(([a,b]) => I.file.has(a) && I.file.has(b)), steps: found.steps, ...(I.D.enrichment && { revision: I.D.enrichment.revision, providers: I.D.enrichment.providers }), stats: { visitedStates: found.visited, maxHops: Number.isFinite(depth) ? depth : null },
    summary: `${plural(rows.length, 'endpoint')} reachable from ${seed} by ${expression.join(' ')} with ${found.steps.length === 1 ? '1 witness' : `${found.steps.length} witnesses`}` };
}
