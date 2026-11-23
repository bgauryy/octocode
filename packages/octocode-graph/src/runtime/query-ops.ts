// Graph questions over the weighted file graph of one data.json (query-index.ts): who depends on a file, what it
// depends on, the strongest path between two files, its cycle and community, symbol callers and callees, rankings
// and text search. Every op returns complete rows; nothing is cut to a page size.
import {
  QueryError,
  edgeOf,
  evidenceLine,
  evidenceOf,
  indexOf,
  resolveFile,
  resolveLocation,
  resolveSymbol,
  type CallEvidence,
  type PairEvidence,
  type QueryIndex,
} from './query-index.ts';
import { PatternError, traceRelations, type Relation } from '../relations.ts';
import {
  fewestHopPath,
  prerequisiteOrder,
  shortestRing,
} from '../algorithms.ts';
import type { Graph } from '../graph-model.ts';
import type { FileMetrics, Loc } from './types.ts';
import { absOf, fileOf } from '../ids.ts';

/** Parsed query arguments: positional words and the edge filters (query-api.ts `parseArgs`). */
export interface QueryArgs {
  args: string[];
  tests: boolean;
  imports: boolean;
  weak: boolean;
  depth: number | null;
  hops?: boolean;
}

/** Every row names a file (its id and location) plus the fields of its op. */
export interface Row {
  id: string;
  label: string;
  chunk?: string | null;
  loc: (Loc & { line?: number | null }) | null;
  /** Walks: hops from the seeds, the weight of the edge that reached it, the file it came through. */
  depth?: number;
  weight?: number;
  through?: string;
  /** Paths and rings: the file this row's hop leads to (a result's `next` is the follow-up Octocode call). */
  nextFile?: string;
  step?: number;
  order?: number | null;
  component?: number;
  /** Rankings. */
  rank?: number;
  value?: number | null;
  pagerank?: number | null;
  /** Symbols: id, name, kind and how the call is backed. */
  symbol?: string;
  name?: string;
  kind?: string;
  via?: string;
  confidence?: string | null;
  count?: number;
  /** search and about. */
  match?: string;
  callSites?: number;
  /** region: hops from the entry, hops to the outcome, and whether every modeled route passes it. */
  fromEntry?: number;
  toOutcome?: number;
  mustPass?: boolean;
  callers?: number;
  callerFiles?: number;
  transitiveCallers?: number;
  /** critical points: sizes of the parts a file's removal leaves. */
  parts?: number[];
  role?: string;
  endpoint?: string;
  /** path rows: the import lines and calls behind the hop. */
  evidence?: PairEvidence;
  [field: string]: unknown;
}

export type Pair = [string, string];

/** What every op answers: rows, the nodes and edges to highlight, stats and a one-line summary. */
export interface OpResult {
  rows: Row[];
  edges: Pair[];
  nodes: string[];
  summary: string;
  stats?: Record<string, unknown>;
  [field: string]: unknown;
}

export type Op = (I: QueryIndex, q: QueryArgs) => OpResult;

const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

const round = (x: number, digits = 3) => Number(x.toFixed(digits));
const byId = (a: { id: string }, b: { id: string }) =>
  a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
export const plural = (n: number, word: string) =>
  `${n} ${word}${n === 1 ? '' : 's'}`;
export const metricOf = (
  I: QueryIndex,
  id: string,
  key: keyof FileMetrics
): number | null | undefined => I.file.get(id)?.metrics?.[key];

// A file row: id, label, chunk and location; `line` (when known) points at the evidence inside the file.
export function fileRow(
  I: QueryIndex,
  id: string,
  { line, ...extra }: { line?: number | null; [field: string]: unknown } = {}
): Row {
  const f = I.file.get(id);
  return {
    id,
    label: f?.label ?? id.split('/').pop() ?? id,
    chunk: f?.chunk ?? null,
    loc: { path: id, line: line ?? 1 },
    ...extra,
  };
}

function countBy(rows: Row[], key: 'depth' | 'chunk' | 'via') {
  const counts: Record<string, number> = {};
  for (const r of rows) {
    const value = r[key] == null ? '-' : String(r[key]);
    counts[value] = (counts[value] ?? 0) + 1;
  }
  return counts;
}

type Direction = 'in' | 'out';
export interface WalkOptions {
  dir: Direction;
  maxDepth: number;
}

// Breadth-first walk to maxDepth along dependencies (out) or dependents (in). A node reached at depth d keeps the
// heaviest edge from depth d-1 that reaches it.
export function walk(
  I: Pick<QueryIndex, 'G'>,
  seeds: string[],
  { dir, maxDepth }: WalkOptions
) {
  const adj = dir === 'in' ? I.G.in : I.G.out;
  let frontier = seeds.map(id => nodeOf(I.G, id));
  const depth = new Map(frontier.map(v => [v, 0]));
  const via = new Map<number, [number, number]>();
  for (let d = 1; frontier.length && d <= maxDepth; d++) {
    const next: number[] = [];
    for (const u of frontier)
      for (let k = adj.offsets[u]; k < adj.offsets[u + 1]; k++) {
        const v = adj.targets[k],
          w = adj.weights[k];
        const seen = depth.get(v);
        if (seen === undefined) {
          depth.set(v, d);
          via.set(v, [u, w]);
          next.push(v);
        } else if (seen === d && w > (via.get(v)?.[1] ?? -Infinity))
          via.set(v, [u, w]);
      }
    frontier = next;
  }
  return { depth, via };
}

/** The node of a resolved file id (resolution only returns graph files). */
function fileNode(I: QueryIndex, id: string) {
  const file = I.file.get(id);
  if (!file) throw new QueryError(`${id} is not in the file graph`);
  return file;
}

/** The CSR index of a resolved file id (resolution only returns graph files). */
function nodeOf(G: Graph, id: string) {
  const v = G.index.get(id);
  if (v === undefined) throw new QueryError(`${id} is not in the file graph`);
  return v;
}

// Rows of a walk: depth, the weight of the edge that reached the file, the file it reached through, and that edge's
// evidence line in the depending file.
function walkRows(I: QueryIndex, seeds: string[], options: WalkOptions) {
  const { depth, via } = walk(I, seeds, options);
  const found: Row[] = [];
  const edges: Pair[] = [];
  for (const [v, [u, w]] of via) {
    const [id, through] = [I.ids[v], I.ids[u]];
    const [from, to] = options.dir === 'in' ? [id, through] : [through, id];
    edges.push([from, to]);
    found.push(
      fileRow(I, id, {
        depth: depth.get(v),
        weight: round(w),
        through,
        line: options.dir === 'in' ? evidenceLine(I, from, to) : undefined,
      })
    );
  }
  const rows = found.toSorted(
    (a, b) =>
      (a.depth ?? 0) - (b.depth ?? 0) ||
      (b.weight ?? 0) - (a.weight ?? 0) ||
      byId(a, b)
  );
  return { rows, edges, nodes: [...seeds, ...rows.map(r => r.id)] };
}

// Default walks follow strong edges, while metrics count every production edge. So an answer states what `--weak` would
// add: "nothing depends on it" must never be read as a fact the graph's own metrics contradict.
function weakView(I: QueryIndex) {
  return I.weak || I.imports
    ? null
    : indexOf(I.D, { tests: I.tests, weak: true });
}
function weakAdds(
  I: QueryIndex,
  seeds: string[],
  options: WalkOptions,
  found: number
) {
  const W = weakView(I);
  return W ? walk(W, seeds, options).via.size - found : 0;
}
const weakNote = (n: number) =>
  n > 0
    ? `; --weak adds ${plural(n, 'file')} reached only through name-bound calls`
    : '';

function depthText(byDepth: Record<string, number>) {
  return Object.entries(byDepth)
    .map(([d, n]) => `depth ${d}: ${n}`)
    .join(', ');
}

function seedFiles(I: QueryIndex, args: string[]) {
  if (!args.length) throw new QueryError('missing file argument');
  return [...new Set(args.map(arg => resolveFile(I, arg)))];
}

export interface SourceQuery {
  path: string;
  ranges?: { startLine: number; endLine: number }[];
}

// File records carry the saved metadata. A source anchor selects a line in the working file.
export function fileRecords(I: QueryIndex, { args }: QueryArgs): OpResult {
  if (!args.length) throw new QueryError('files needs one or more file paths');
  const reads = new Map<string, Set<number> | null>();
  const rows = args.map(arg => {
    const { id, line, column } = resolveLocation(I, arg);
    const file = fileNode(I, id);
    const loc = {
      path: file.manifest ?? id,
      line: line ?? 1,
      ...(column && { column }),
    };
    const sourcePath = absOf(I.root, loc.path);
    if (!reads.has(sourcePath)) reads.set(sourcePath, new Set());
    if (line) reads.get(sourcePath)?.add(loc.line);
    else reads.set(sourcePath, null); // A whole-file request already includes every cited line.
    return { ...file, loc };
  });
  const nodes = [...new Set(rows.map(row => row.id))];
  const sourceQueries: SourceQuery[] = Array.from(
    reads,
    ([sourcePath, lines]) =>
      lines
        ? {
            path: sourcePath,
            ranges: Array.from(lines, line => ({
              startLine: line,
              endLine: line,
            })),
          }
        : { path: sourcePath }
  );
  return {
    rows,
    edges: [],
    nodes,
    sourceQueries,
    summary: `${plural(nodes.length, 'file')} in the snapshot`,
  };
}

export function impact(I: QueryIndex, { args, depth }: QueryArgs): OpResult {
  const seeds = seedFiles(I, args);
  const label = seeds.join(', ');
  const maxDepth = depth ?? Infinity;
  const r = walkRows(I, seeds, { dir: 'in', maxDepth });
  const weak = weakAdds(I, seeds, { dir: 'in', maxDepth }, r.rows.length);
  const stats = {
    byDepth: countBy(r.rows, 'depth'),
    byChunk: countBy(r.rows, 'chunk'),
    weakAdds: weak,
  };
  const chunks = Object.keys(stats.byChunk).length;
  const summary = r.rows.length
    ? `${plural(r.rows.length, 'file')} ${r.rows.length === 1 ? 'depends' : 'depend'} on ${label}${depth ? ` within depth ${depth}` : ' transitively'} (${depthText(stats.byDepth)}) across ${plural(chunks, 'chunk')}`
    : `nothing depends on ${label}`;
  return { ...r, stats, summary: `${summary}${weakNote(weak)}` };
}

function neighbors(
  I: QueryIndex,
  { args, depth }: QueryArgs,
  dir: Direction
): OpResult {
  const seeds = seedFiles(I, args);
  const label = seeds.join(', ');
  const maxDepth = depth ?? 1;
  const r = walkRows(I, seeds, { dir, maxDepth });
  const one = r.rows.length === 1;
  const what =
    dir === 'in'
      ? one
        ? 'depends on'
        : 'depend on'
      : one
        ? 'is a dependency of'
        : 'are dependencies of';
  const weak = weakAdds(I, seeds, { dir, maxDepth }, r.rows.length);
  const stats = {
    byDepth: countBy(r.rows, 'depth'),
    byChunk: countBy(r.rows, 'chunk'),
    weakAdds: weak,
  };
  return {
    ...r,
    stats,
    summary: `${plural(r.rows.length, 'file')} ${what} ${label} within depth ${maxDepth === Infinity ? 'all' : maxDepth}${weakNote(weak)}`,
  };
}
export const deps: Op = (I, q) => neighbors(I, q, 'out');
export const dependents: Op = (I, q) => neighbors(I, q, 'in');

type HeapItem = [cost: number, node: number];

// A binary min-heap of [cost, node].
function heapPush(h: HeapItem[], item: HeapItem) {
  h.push(item);
  for (let i = h.length - 1; i > 0;) {
    const p = (i - 1) >> 1;
    if (h[p][0] <= h[i][0]) break;
    [h[p], h[i]] = [h[i], h[p]];
    i = p;
  }
}
function heapPop(h: HeapItem[]) {
  const first = h[0];
  const last = h.pop();
  if (h.length && last) {
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

interface Route {
  path: string[];
  cost: number;
}

// Dijkstra with cost 1/weight: the strongest dependency path s → t (node indices), or null.
function strongest(
  I: QueryIndex,
  s: number,
  t: number,
  blocked?: Uint8Array
): Route | null {
  const { n, out } = I.G;
  const dist = new Float64Array(n).fill(Infinity),
    prev = new Int32Array(n).fill(-1);
  dist[s] = 0;
  const heap: HeapItem[] = [[0, s]];
  while (heap.length) {
    const [d, u] = heapPop(heap);
    if (d > dist[u]) continue;
    if (u === t) break;
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
      const v = out.targets[k],
        c = d + 1 / Math.max(out.weights[k], 1e-9);
      if (blocked?.[v]) continue;
      if (c < dist[v]) {
        dist[v] = c;
        prev[v] = u;
        heapPush(heap, [c, v]);
      }
    }
  }
  if (dist[t] === Infinity) return null;
  return { path: routeIds(I, prev, s, t), cost: dist[t] };
}

function routeIds(I: QueryIndex, prev: Int32Array, start: number, end: number) {
  const ids: string[] = [];
  for (let v = end; ; v = prev[v]) {
    ids.push(I.ids[v]);
    if (v === start) return ids.reverse();
  }
}

// Unit-cost BFS on the same filtered CSR that the weighted solver reads.
function fewestHops(
  I: QueryIndex,
  s: number,
  t: number,
  blocked?: Uint8Array
): Route | null {
  const route = fewestHopPath(I.G, s, t, blocked);
  if (!route) return null;
  return { path: route.map(v => I.ids[v]), cost: route.length - 1 };
}

// One row per file on the path; each carries the hop to the next file: weight, import lines and symbol calls.
export function pathRows(I: QueryIndex, route: string[], direction: string) {
  return route.map((id, i) => {
    const next = route[i + 1];
    if (!next) return fileRow(I, id, { step: i, direction });
    const ev = evidenceOf(I, id, next);
    const weight = edgeOf(I, id, next)?.weight ?? 0;
    return fileRow(I, id, {
      step: i,
      direction,
      nextFile: next,
      weight: round(weight),
      line: evidenceLine(I, id, next),
      evidence: {
        imports: [...new Set(ev.imports)].toSorted((a, b) => a - b),
        calls: ev.calls,
      },
    });
  });
}

function pathResult(
  I: QueryIndex,
  found: Route,
  direction: string,
  objective: string
) {
  const rows = pathRows(I, found.path, direction);
  const edges = found.path.slice(1).map((id, i): Pair => [found.path[i], id]);
  const total = round(rows.reduce((a, r) => a + (r.weight ?? 0), 0));
  return {
    rows,
    edges,
    nodes: found.path,
    stats: {
      hops: edges.length,
      weight: total,
      cost: round(found.cost, 6),
      direction,
      objective,
    },
  };
}

// Files a `--avoid` list names: a file (any form a query accepts) or a directory (every file under it).
function avoided(I: QueryIndex, names: string[]) {
  const files = new Set<string>();
  for (const name of names) {
    const dir = `${name.replace(/^\.\//, '').replace(/\/+$/, '')}/`;
    const under = I.ids.filter(id => id.startsWith(dir));
    if (under.length) for (const id of under) files.add(id);
    else files.add(resolveFile(I, name));
  }
  return files;
}

export function path(I: QueryIndex, { args, hops }: QueryArgs): OpResult {
  const solve = hops ? fewestHops : strongest;
  const objective = hops ? 'hops' : 'strength';
  const at = args.indexOf('--avoid');
  const ends = at < 0 ? args : args.slice(0, at);
  if (ends.length !== 2)
    throw new QueryError(
      'path needs two files: path <from> <to> [--avoid <file|dir>...]'
    );
  const [a, b] = [resolveFile(I, ends[0]), resolveFile(I, ends[1])];
  const [s, t] = [nodeOf(I.G, a), nodeOf(I.G, b)];
  if (at >= 0) return avoidingPath(I, a, b, args.slice(at + 1), hops);
  const found = solve(I, s, t);
  if (found) {
    const r = pathResult(I, found, 'forward', objective);
    return {
      ...r,
      found: true,
      summary: `${a} → ${b}: ${plural(r.stats.hops, 'hop')}, weight ${r.stats.weight} (${hops ? 'fewest hops' : `strongest path, cost Σ 1/weight ${r.stats.cost}`})`,
    };
  }
  const back = solve(I, t, s);
  const W = weakView(I);
  const weakPath = W ? solve(W, s, t) : null;
  const weakText = weakPath
    ? `; --weak finds one of ${plural(weakPath.path.length - 1, 'hop')} through name-bound calls`
    : '';
  if (!back)
    return {
      found: false,
      rows: [],
      edges: [],
      nodes: [a, b],
      stats: {
        objective,
        hops: 0,
        weakHops: weakPath ? weakPath.path.length - 1 : null,
      },
      summary: `no path ${a} → ${b}, and none ${b} → ${a}${weakText}`,
    };
  const r = pathResult(I, back, 'reverse', objective);
  return {
    ...r,
    found: false,
    summary: `no path ${a} → ${b}; the reverse ${b} → ${a} exists: ${plural(r.stats.hops, 'hop')}, weight ${r.stats.weight}${weakText}`,
  };
}

// An architecture rule as a path question: a route from `a` to `b` that passes none of the avoided files breaks a
// rule that every such route must cross them ("the CLI reaches storage only through the service layer").
function avoidingPath(
  I: QueryIndex,
  a: string,
  b: string,
  names: string[],
  hops = false
): OpResult {
  if (!names.length)
    throw new QueryError('--avoid needs one or more files or directories');
  const files = avoided(I, names);
  if (files.has(a) || files.has(b))
    throw new QueryError(
      '--avoid cannot name the start or the end of the path'
    );
  const blocked = new Uint8Array(I.G.n);
  for (const id of files) {
    const v = I.G.index.get(id);
    if (v !== undefined) blocked[v] = 1;
  }
  const objective = hops ? 'hops' : 'strength';
  const found = (hops ? fewestHops : strongest)(
    I,
    nodeOf(I.G, a),
    nodeOf(I.G, b),
    blocked
  );
  const avoid = { avoided: files.size, avoidNames: names };
  if (!found)
    return {
      found: false,
      rows: [],
      edges: [],
      nodes: [a, b],
      stats: { objective, hops: 0, ...avoid },
      summary: `every route ${a} → ${b} passes through the avoided ${plural(files.size, 'file')} (${names.join(', ')}): none bypasses them`,
    };
  const r = pathResult(I, found, 'forward', objective);
  return {
    ...r,
    found: true,
    stats: { ...r.stats, ...avoid },
    summary: `${a} → ${b} bypasses the avoided ${plural(files.size, 'file')} (${names.join(', ')}): ${plural(r.stats.hops, 'hop')}; each row cites the crossing`,
  };
}

// The strongly connected set of `s` and one ring through it (null when s is on no cycle).
function cycleOf(I: QueryIndex, s: number) {
  const { comp, comps } = I.components();
  const members = new Set(comps[comp[s]]);
  return { members, order: shortestRing(I.G, s, members) };
}

// Stored cycle IDs identify exact member sets from the snapshot projection.
function matchingCycle(I: QueryIndex, id: string, members: Set<number>) {
  const candidate = I.D.metrics?.cycles?.find(c => c.members.includes(id));
  return candidate?.size === members.size &&
    candidate.members.every(u => members.has(I.G.index.get(u) ?? -1))
    ? candidate
    : null;
}

function componentWeight({ out }: Graph, members: Set<number>) {
  let weight = 0;
  for (const u of members)
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
      if (members.has(out.targets[k])) weight += out.weights[k];
    }
  return round(weight);
}

// No cycle over the walked edges: say whether `--weak` finds one, since metrics count those edges.
function noCycle(I: QueryIndex, id: string, s: number): OpResult {
  const W = weakView(I);
  const weak = W ? cycleOf(W, s) : null;
  const size = weak?.order ? weak.members.size : null;
  const known = W && weak && size ? matchingCycle(W, id, weak.members) : null;
  const note = size
    ? `; --weak finds it in ${known ? `cycle ${known.id}` : 'a cycle'} of ${plural(size, 'file')} through name-bound calls`
    : '';
  return {
    rows: [],
    edges: [],
    nodes: [id],
    stats: { size: 1, weakSize: size },
    summary: `${id} is in no dependency cycle${note}`,
  };
}

const byPagerank = (I: QueryIndex) => (a: string, b: string) =>
  (metricOf(I, b, 'pagerank') ?? 0) - (metricOf(I, a, 'pagerank') ?? 0) ||
  (a < b ? -1 : 1);

// Every dependency cycle the build measured (metrics.cycles), largest first, each with all its members; `cycle <file>`
// then walks one ring through a member.
function cycleList(I: QueryIndex): OpResult {
  const cycles = (I.D.metrics?.cycles ?? []).toSorted(
    (a, b) => b.size - a.size || b.weight - a.weight || a.id - b.id
  );
  const rows = cycles.map(c => ({
    id: c.members[0],
    label: `cycle ${c.id}`,
    loc: { path: c.members[0] },
    cycleId: c.id,
    weight: c.weight,
    chunks: c.chunks,
    languages: c.languages,
    members: c.members,
  }));
  const files = new Set(cycles.flatMap(c => c.members));
  return {
    rows,
    edges: [],
    nodes: [...files],
    stats: { cycles: cycles.length, files: files.size },
    summary: cycles.length
      ? `${plural(cycles.length, 'dependency cycle')} over ${plural(files.size, 'file')}, largest first; \`cycle <file>\` walks one ring through a member`
      : 'no dependency cycle in this graph',
  };
}

export function cycle(I: QueryIndex, { args }: QueryArgs): OpResult {
  if (!args.length) return cycleList(I);
  const id = resolveFile(I, args[0]);
  const s = nodeOf(I.G, id);
  const { members, order } = cycleOf(I, s);
  if (!order) return noCycle(I, id, s);
  const ids = order.map(i => I.ids[i]);
  const ring = ids.map((u, i): Pair => [u, ids[(i + 1) % ids.length]]);
  const rows = ring.map(([u, v], i) =>
    fileRow(I, u, {
      order: i,
      nextFile: v,
      weight: round(edgeOf(I, u, v)?.weight ?? 0),
      line: evidenceLine(I, u, v),
    })
  );
  const inRing = new Set(ids);
  const rest = [...members]
    .map(i => I.ids[i])
    .filter(u => !inRing.has(u))
    .toSorted(byPagerank(I))
    .map(u =>
      fileRow(I, u, {
        order: null,
        pagerank: metricOf(I, u, 'pagerank') ?? null,
      })
    );
  const known = matchingCycle(I, id, members);
  const stats = {
    size: members.size,
    ring: ids.length,
    cycle: known?.id ?? null,
    weight: componentWeight(I.G, members),
    chunks: [
      ...new Set([...members].map(i => I.file.get(I.ids[i])?.chunk ?? '-')),
    ].toSorted(byText),
  };
  const label = known ? `cycle ${known.id}` : 'a cycle';
  return {
    rows: [...rows, ...rest],
    edges: ring,
    nodes: [...members].map(i => I.ids[i]),
    stats,
    summary: `${id} is in ${label} of ${plural(members.size, 'file')} across ${plural(stats.chunks.length, 'chunk')}; one ring through it has ${plural(ids.length, 'file')}`,
  };
}

function communityId(I: QueryIndex, arg: string | undefined) {
  if (/^\d+$/.test(arg ?? '')) return Number(arg);
  const id = resolveFile(I, arg);
  const c = metricOf(I, id, 'community');
  if (c == null)
    throw new QueryError(
      `${id} has no community: metrics were not computed for it`
    );
  return c;
}

interface CommunityLink {
  community: number;
  label: string | null;
  out: number;
  in: number;
  weight: number;
}

// Weight from the community's members to each other community (out) and back (in).
function communityLinks(
  I: QueryIndex,
  inside: string[],
  id: number
): CommunityLink[] {
  const links = new Map<
    number,
    { community: number; out: number; in: number }
  >();
  const add = (c: number, key: 'out' | 'in', w: number) => {
    const x = links.get(c) ?? { community: c, out: 0, in: 0 };
    x[key] += w;
    links.set(c, x);
  };
  for (const u of inside) {
    const i = nodeOf(I.G, u);
    const sides: [Graph['out'], 'out' | 'in'][] = [
      [I.G.out, 'out'],
      [I.G.in, 'in'],
    ];
    for (const [adj, key] of sides)
      for (let k = adj.offsets[i]; k < adj.offsets[i + 1]; k++) {
        const c = metricOf(I, I.ids[adj.targets[k]], 'community');
        if (c != null && c !== id) add(c, key, adj.weights[k]);
      }
  }
  const label = new Map(
    (I.D.metrics?.communities ?? []).map(c => [c.id, c.label])
  );
  return [...links.values()]
    .map(x => ({
      ...x,
      label: label.get(x.community) ?? null,
      out: round(x.out),
      in: round(x.in),
      weight: round(x.out + x.in),
    }))
    .toSorted((a, b) => b.weight - a.weight || a.community - b.community);
}

export function community(I: QueryIndex, { args }: QueryArgs): OpResult {
  const id = communityId(I, args[0]);
  const c = (I.D.metrics?.communities ?? []).find(x => x.id === id);
  if (!c)
    throw new QueryError(
      `no community ${id}: data.json lists ${I.D.metrics?.communities?.length ?? 0}`
    );
  const members = c.members.filter(m => I.file.has(m)).toSorted(byPagerank(I));
  const rows = members.map((m, rank) =>
    fileRow(I, m, {
      rank: rank + 1,
      pagerank: metricOf(I, m, 'pagerank') ?? null,
    })
  );
  const links = communityLinks(I, members, id);
  const best = links[0]
    ? `; strongest link: community ${links[0].community}${links[0].label ? ` (${links[0].label})` : ''}, weight ${links[0].weight}`
    : '';
  return {
    rows,
    edges: [],
    nodes: members,
    links,
    stats: {
      id,
      label: c.label,
      size: c.size,
      internal: c.internal,
      external: c.external,
      links: links.length,
    },
    summary: `community ${id} (${c.label}): ${plural(members.length, 'file')}, linked to ${links.length} other communit${links.length === 1 ? 'y' : 'ies'}${best}`,
  };
}

// Symbol-level calls into (callers) or out of (callees) one symbol, with provenance; rows point at the other file.
function symbolCalls(
  I: QueryIndex,
  { args }: QueryArgs,
  dir: 'callers' | 'callees'
): OpResult {
  const sym = resolveSymbol(I, args[0]);
  const { byId: symbols } = I.symbols();
  const [mine, other] =
    dir === 'callers'
      ? (['target', 'source'] as const)
      : (['source', 'target'] as const);
  const calls = I.callsBy(mine).get(sym) ?? [];
  const rows = calls
    .map(c => {
      const s = symbols.get(c[other]);
      const lines = c.lines ?? (c.line ? [c.line] : []);
      // A caller row opens at the call site; a callee row at the callee's declaration (else the call site).
      const line = dir === 'callers' ? lines[0] : (s?.line ?? undefined);
      return fileRow(I, fileOf(c[other]), {
        line,
        symbol: c[other],
        name: s?.name ?? c[other].split('#').pop(),
        lines,
        via: c.via ?? 'graph',
        confidence: c.confidence ?? null,
        count: c.count ?? 1,
        test: c.test,
      });
    })
    .toSorted(
      (a, b) =>
        byId(a, b) ||
        (a.loc?.line ?? 0) - (b.loc?.line ?? 0) ||
        ((a.symbol ?? '') < (b.symbol ?? '') ? -1 : 1)
    );
  const file = fileOf(sym);
  const pairs = new Set(
    rows.map(r => (dir === 'callers' ? `${r.id}\0${file}` : `${file}\0${r.id}`))
  );
  const edges = [...pairs]
    .map(k => k.split('\0') as Pair)
    .filter(([u, v]) => u !== v);
  const via = countBy(rows, 'via');
  const files = new Set(rows.map(r => r.id));
  const coverage = dir === 'callers' ? callerCoverage(I, sym) : null;
  return {
    symbol: sym,
    rows,
    edges,
    nodes: [file, ...files],
    stats: { calls: rows.length, files: files.size, via },
    // Every caller list is syntax evidence: its lspSearch confirmation comes with it; warnings only when they exist.
    ...(coverage?.warnings.length && { warnings: coverage.warnings }),
    ...(coverage?.partialReasons && {
      isPartial: true,
      partialReasons: coverage.partialReasons,
    }),
    ...(coverage?.next && { next: coverage.next }),
    summary: `${plural(rows.length, dir === 'callers' ? 'caller' : 'callee')} of ${sym} in ${plural(files.size, 'file')} (${
      Object.entries(via)
        .map(([k, n]) => `${k} ${n}`)
        .join(', ') || 'none'
    })${coverage?.partialReasons ? `; may be incomplete (${coverage.partialReasons.join(', ')}): see warnings` : ''}`,
  };
}

/** The Octocode calls an answer hands over: read the cited source, or confirm a caller list with LSP. */
export interface NextCalls {
  read?: {
    tool: 'localFetch';
    input: {
      queries: {
        path: string;
        ranges?: { startLine: number; endLine: number }[];
      }[];
    };
  };
  confirm?: {
    tool: 'lspSearch';
    input: {
      queries: {
        operation: 'callers';
        path: string;
        symbolName: string;
        line: number;
      }[];
    };
  };
}

/** The absolute path localFetch reads for a graph file. */
export const sourcePath = (I: QueryIndex, file: string) => absOf(I.root, file);

/** A localFetch call for a file, or for a symbol's declaration span (its line to its end line). */
export function readCall(
  I: QueryIndex,
  id: string
): NonNullable<NextCalls['read']> {
  const info = id.includes('#') ? I.symbols().byId.get(id) : undefined;
  const source = sourcePath(I, fileOf(id));
  const startLine = info?.line ?? undefined;
  return {
    tool: 'localFetch',
    input: {
      queries: [
        startLine
          ? {
              path: source,
              ranges: [
                {
                  startLine,
                  endLine: Math.max(startLine, info?.endLine ?? startLine),
                },
              ],
            }
          : { path: source },
      ],
    },
  };
}

// What a callers answer cannot see, stated with it. Calls from other chunks bind only through stitched imports and
// the LSP probes of the build (the engine records no call site for some calls of a name imported from another
// package), and calls written inside Rust macro invocations are visible only to the language server. `confirm` asks
// the language server for the same callers.
/** What a callers answer can and cannot see: warnings, why it may be incomplete, and the call that confirms it. */
interface CallerCoverage {
  warnings: string[];
  partialReasons?: string[];
  next?: { confirm: NextCalls['confirm'] };
}
function callerCoverage(I: QueryIndex, sym: string): CallerCoverage {
  const info = I.symbols().byId.get(sym);
  const file = I.file.get(fileOf(sym));
  // Every callable was probed for its callees and every probe answered: each caller the language server knows is here,
  // inside macros and on typed receivers too. Any narrower build cannot show it has every caller.
  const stats = I.D.callStats;
  if (stats?.scope === 'all callable declarations' && stats.lsp === 'ok')
    return { warnings: [] };
  const exported = (I.D.symbols ?? []).find(s => s.id === sym)?.exported;
  const warnings: string[] = [];
  if ((I.D.chunks?.length ?? 0) > 1 && exported !== false)
    warnings.push(
      `callers in other chunks are bound through imports and crate paths (resolution stitched-import) and confirmed by the build's LSP probes (${I.D.callStats?.scope ?? 'no LSP'}); a call on a receiver whose type comes from another chunk is missing: confirm with lspSearch callers`
    );
  if (file?.language === 'rust' || fileOf(sym).endsWith('.rs'))
    warnings.push(
      'calls inside Rust macros other than std forwarding macros (assert_eq!, format!), json! values and join!/try_join! arguments (a macro_rules! body, select!, a proc macro) are visible only to the language server'
    );
  const confirm: NextCalls['confirm'] =
    info?.line != null
      ? {
          tool: 'lspSearch',
          input: {
            queries: [
              {
                operation: 'callers',
                path: sourcePath(I, fileOf(sym)),
                symbolName: info.name.split('.').pop() ?? info.name,
                line: info.line,
              },
            ],
          },
        }
      : undefined;
  return {
    warnings,
    partialReasons: ['callersNotProbed'],
    ...(confirm && { next: { confirm } }),
  };
}
export const callers: Op = (I, q) => symbolCalls(I, q, 'callers');
export const callees: Op = (I, q) => symbolCalls(I, q, 'callees');

export const METRICS = {
  pagerank: 'pagerank',
  betweenness: 'betweenness',
  inweight: 'inWeight',
  outweight: 'outWeight',
  core: 'core',
  depth: 'depth',
} as const satisfies Record<string, keyof FileMetrics>;

const metricKey = (name: string) =>
  Object.hasOwn(METRICS, name) ? METRICS[name as keyof typeof METRICS] : null;

// Files ranked by one metric, highest first (ties by path). n: a count or `all`; default 20, said in the summary.
export function top(I: QueryIndex, { args }: QueryArgs): OpResult {
  const key = metricKey((args[0] ?? '').toLowerCase());
  if (!key)
    throw new QueryError(
      `top needs a metric: ${Object.values(METRICS).join(', ')}`
    );
  const value = (id: string) => metricOf(I, id, key) ?? 0;
  const ranked = I.ids.filter(id => typeof metricOf(I, id, key) === 'number');
  if (!ranked.length)
    throw new QueryError(
      `no file has ${key}: metrics were not computed for this result`
    );
  const all = (args[1] ?? '').toLowerCase() === 'all';
  const n = all
    ? ranked.length
    : Math.max(1, Number.parseInt(args[1] ?? '20', 10) || 20);
  const order = ranked
    .toSorted((a, b) => value(b) - value(a) || (a < b ? -1 : 1))
    .slice(0, n);
  const rows = order.map((id, i) =>
    fileRow(I, id, { rank: i + 1, value: metricOf(I, id, key) })
  );
  const more =
    ranked.length > rows.length
      ? `; \`top ${key} all\` ranks all ${ranked.length}`
      : '';
  return {
    metric: key,
    rows,
    edges: [],
    nodes: order,
    stats: { metric: key, shown: rows.length, ranked: ranked.length },
    summary: `top ${rows.length} of ${ranked.length} files by ${key}${more}`,
  };
}

// Component numbers are local to this answer, increasing from prerequisites to dependents.
export function dependencyOrder(I: QueryIndex, { args }: QueryArgs): OpResult {
  if (args.length !== 1) throw new QueryError('order needs one file');
  const file = resolveFile(I, args[0]);
  const plan = prerequisiteOrder(I.G, I.components(), nodeOf(I.G, file));
  const rows = plan.groups.flatMap((group, component) =>
    group.map(v => fileRow(I, I.ids[v], { component }))
  );
  const edges = plan.crossings.map(([u, v]): Pair => [I.ids[u], I.ids[v]]);
  const crossings: {
    source: string;
    target: string;
    evidence: PairEvidence;
  }[] = edges.map(([source, target]) => ({
    source,
    target,
    evidence: evidenceOf(I, source, target),
  }));
  return {
    nodes: rows.map(r => r.id),
    rows,
    edges,
    crossings,
    stats: { groups: plan.groups.length },
    summary: `${file}: ${plural(rows.length, 'file')} in ${plural(plan.groups.length, 'dependency group')}, prerequisites first. Files in the same group move together.`,
  };
}

export function trace(I: QueryIndex, { args, depth }: QueryArgs): OpResult {
  const [target, ...expression] = args;
  if (!target || !expression.length)
    throw new QueryError(
      'trace needs a file or file#symbol and a relation pattern'
    );
  if (I.imports)
    throw new QueryError(
      'Choose IMPORTS in the trace pattern to restrict relation types'
    );
  const index = I.relations();
  const seed = index.has(target)
    ? target
    : target.includes('#')
      ? resolveSymbol(I, target)
      : resolveFile(I, target);
  let found: ReturnType<typeof traceRelations>;
  try {
    found = traceRelations(
      index,
      seed,
      expression.join(' '),
      depth ?? Infinity
    );
  } catch (error) {
    if (error instanceof PatternError) throw new QueryError(error.message);
    throw error;
  }
  const sources = new Map<string, Relation>();
  for (const edge of found.steps)
    if (!sources.has(edge.target) || edge.relation === 'DEFINES')
      sources.set(edge.target, edge);
  const rows: Row[] = found.endpoints.map(endpoint => {
    const symbol = I.symbols().byId.get(endpoint);
    const origin = sources.get(endpoint);
    const citedFile = I.file.has(endpoint)
      ? endpoint
      : (symbol?.file ?? origin?.path);
    const line =
      symbol?.line ?? (origin?.path === citedFile ? origin?.lines[0] : null);
    return {
      id: citedFile ?? endpoint,
      endpoint,
      label: endpoint,
      loc: citedFile ? { path: citedFile, line: line ?? null } : null,
    };
  });
  const files = new Set([fileOf(seed)]);
  const pairs = new Map<string, Pair>();
  for (const step of found.steps) {
    const a = fileOf(step.source),
      b = fileOf(step.target);
    files.add(a);
    files.add(b);
    if (a !== b) pairs.set(JSON.stringify([a, b]), [a, b]);
  }
  return {
    rows,
    nodes: [...new Set([...files, ...rows.map(r => r.id)])].filter(id =>
      I.file.has(id)
    ),
    edges: [...pairs.values()].filter(
      ([a, b]) => I.file.has(a) && I.file.has(b)
    ),
    steps: found.steps,
    ...(I.D.enrichment && {
      revision: I.D.enrichment.revision,
      providers: I.D.enrichment.providers,
    }),
    stats: {
      visitedStates: found.visited,
      maxHops: Number.isFinite(depth) ? depth : null,
    },
    summary: `${plural(rows.length, 'endpoint')} reachable from ${seed} by ${expression.join(' ')} with ${found.steps.length === 1 ? '1 witness' : `${found.steps.length} witnesses`}`,
  };
}

export type { CallEvidence };
