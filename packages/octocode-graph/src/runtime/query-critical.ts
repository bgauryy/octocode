// Critical code over one data.json (query-index.ts):
//   critical functions [n|all]   functions ranked by PageRank over the weighted call graph (call sites × evidence),
//                                with the call sites, callers and transitive callers each one has
//   critical points [n|all]      files whose removal disconnects the production file graph (articulation points),
//                                ranked by how many files they alone keep connected
//   critical path [<file>] [--up]   the longest dependency chain: from <file> down to what it finally rests on
//                                (--up: from the furthest dependent down to <file>); without a file, the deepest
//                                chain in the graph. Each hop carries its weight and evidence like `path`.
// Edge filters (--tests, --imports, --weak) select the graph as for every walk.
import { callEvidence, csr, type Graph } from '../graph-model.ts';
import { pagerank, type Components } from '../algorithms.ts';
import { QueryError, resolveFile, type QueryIndex } from './query-index.ts';
import {
  fileRow,
  metricOf,
  pathRows,
  plural,
  type OpResult,
  type Pair,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import { fileOf } from '../ids.ts';

/** Rows `critical functions` and `critical points` show unless asked for `all` or a count. */
export const CRITICAL_SHOWN = 20;

function shownCount(args: string[], ranked: number) {
  const all = (args[0] ?? '').toLowerCase() === 'all';
  if (all) return ranked;
  const n = Number.parseInt(args[0] ?? '', 10);
  return Number.isSafeInteger(n) && n > 0 ? n : CRITICAL_SHOWN;
}

const more = (form: string, shown: number, ranked: number) =>
  ranked > shown ? `; \`critical ${form} all\` ranks all ${ranked}` : '';

// The production call graph between functions, weighted like file edges: call sites × binding evidence.
export function callGraph(I: QueryIndex) {
  const calls = (I.D.calls ?? []).filter(
    c =>
      (I.tests || !c.test) &&
      c.source !== c.target &&
      c.source.includes('#') &&
      c.target.includes('#')
  );
  const ids = [...new Set(calls.flatMap(c => [c.source, c.target]))];
  const edges = calls.map(c => ({
    source: c.source,
    target: c.target,
    weight: (c.count ?? 1) * callEvidence(c),
    test: false,
  }));
  return { calls, G: csr(ids, edges) };
}

// How many nodes reach `start` along incoming edges (its transitive callers or dependents).
function reachCount(G: Graph, start: number) {
  const seen = new Uint8Array(G.n);
  seen[start] = 1;
  const queue = [start];
  for (let head = 0; head < queue.length; head++) {
    const u = queue[head];
    for (let k = G.in.offsets[u]; k < G.in.offsets[u + 1]; k++) {
      const v = G.in.targets[k];
      if (!seen[v]) {
        seen[v] = 1;
        queue.push(v);
      }
    }
  }
  return queue.length - 1;
}

function criticalFunctions(I: QueryIndex, args: string[]): OpResult {
  const { calls, G } = callGraph(I);
  if (!G.n)
    throw new QueryError('no function calls in this graph: nothing to rank');
  const { rank } = pagerank(G);
  const sites = new Float64Array(G.n);
  const callers = new Map<number, Set<string>>();
  for (const c of calls) {
    const t = G.index.get(c.target);
    if (t === undefined) continue;
    sites[t] += c.count ?? 1;
    let set = callers.get(t);
    if (!set) callers.set(t, (set = new Set()));
    set.add(c.source);
  }
  const symbols = I.symbols().byId;
  const order = Array.from(G.ids.keys()).sort(
    (a, b) => rank[b] - rank[a] || (G.ids[a] < G.ids[b] ? -1 : 1)
  );
  const shown = order.slice(0, shownCount(args.slice(1), order.length));
  const rows: Row[] = shown.map((v, i) => {
    const id = G.ids[v];
    const s = symbols.get(id);
    const from = callers.get(v) ?? new Set<string>();
    return fileRow(I, fileOf(id), {
      line: s?.line ?? undefined,
      rank: i + 1,
      symbol: id,
      name: s?.name ?? id.split('#').pop(),
      kind: s?.kind ?? 'symbol',
      pagerank: Number(rank[v].toFixed(8)),
      callSites: sites[v],
      callers: from.size,
      callerFiles: new Set([...from].map(fileOf)).size,
      transitiveCallers: reachCount(G, v),
    });
  });
  return {
    rows,
    edges: [],
    nodes: [...new Set(rows.map(r => r.id))],
    stats: { functions: G.n, calls: calls.length, shown: rows.length },
    summary: `top ${rows.length} of ${G.n} functions by PageRank over the call graph (weight: call sites × evidence)${more('functions', rows.length, G.n)}`,
  };
}

interface Cut {
  node: number;
  parts: number[];
  component: number;
}

// Articulation points of the undirected file graph (Hopcroft–Tarjan, iterative): for each, the sizes of the parts its
// removal leaves of its connected component.
function articulation(G: Graph): Cut[] {
  const n = G.n;
  const disc = new Int32Array(n).fill(-1),
    low = new Int32Array(n),
    size = new Int32Array(n),
    parent = new Int32Array(n).fill(-1);
  const separated = new Map<number, number[]>();
  const neighbors = (u: number) => {
    const out: number[] = [];
    for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++)
      out.push(G.out.targets[k]);
    for (let k = G.in.offsets[u]; k < G.in.offsets[u + 1]; k++)
      out.push(G.in.targets[k]);
    return out.filter(v => v !== u);
  };
  const cuts: Cut[] = [];
  let time = 0;
  for (let root = 0; root < n; root++) {
    if (disc[root] >= 0) continue;
    const members: number[] = [];
    const stack: [number, number[], number][] = [[root, neighbors(root), 0]];
    disc[root] = low[root] = time++;
    size[root] = 1;
    members.push(root);
    let rootChildren = 0;
    while (stack.length) {
      const frame = stack[stack.length - 1];
      const [u, adj] = frame;
      if (frame[2] < adj.length) {
        const v = adj[frame[2]++];
        if (disc[v] < 0) {
          parent[v] = u;
          disc[v] = low[v] = time++;
          size[v] = 1;
          members.push(v);
          if (u === root) rootChildren++;
          stack.push([v, neighbors(v), 0]);
        } else if (v !== parent[u]) low[u] = Math.min(low[u], disc[v]);
        continue;
      }
      stack.pop();
      const p = parent[u];
      if (p < 0) continue;
      low[p] = Math.min(low[p], low[u]);
      size[p] += size[u];
      if (low[u] >= disc[p]) {
        const list = separated.get(p) ?? [];
        list.push(size[u]);
        separated.set(p, list);
      }
    }
    const total = members.length;
    for (const u of members) {
      const parts = separated.get(u);
      if (!parts) continue;
      if (u === root && rootChildren < 2) continue;
      const rest = total - 1 - parts.reduce((a, b) => a + b, 0);
      cuts.push({
        node: u,
        parts: [...parts, ...(rest > 0 ? [rest] : [])].sort((a, b) => b - a),
        component: total,
      });
    }
  }
  return cuts;
}

function criticalPoints(I: QueryIndex, args: string[]): OpResult {
  const cuts = articulation(I.G)
    .map(c => ({ ...c, cutOff: c.component - 1 - c.parts[0] }))
    .sort(
      (a, b) =>
        b.cutOff - a.cutOff ||
        (metricOf(I, I.ids[b.node], 'pagerank') ?? 0) -
          (metricOf(I, I.ids[a.node], 'pagerank') ?? 0) ||
        (I.ids[a.node] < I.ids[b.node] ? -1 : 1)
    );
  const shown = cuts.slice(0, shownCount(args.slice(1), cuts.length));
  const rows = shown.map((c, i) =>
    fileRow(I, I.ids[c.node], {
      rank: i + 1,
      value: c.cutOff,
      parts: c.parts,
      componentSize: c.component,
    })
  );
  return {
    rows,
    edges: [],
    nodes: rows.map(r => r.id),
    stats: { points: cuts.length, shown: rows.length },
    summary: cuts.length
      ? `${plural(cuts.length, 'file')} alone connect parts of the file graph; the top one cuts off ${shown[0].cutOff} files from the rest of its component (value: files cut off; parts: sizes left after removing it)${more('points', rows.length, cuts.length)}`
      : 'no single file disconnects the file graph',
  };
}

// Longest chain of components below (down) or above (up) each node, over the condensation DAG.
function chainDepths(G: Graph, { comp, comps }: Components, up: boolean) {
  const d = new Int32Array(comps.length);
  const adj = up ? G.in : G.out;
  // Tarjan lists a component after every component it reaches: down walks that order, up walks it reversed.
  const order = comps.map((_, i) => i);
  if (up) order.reverse();
  for (const c of order) {
    let best = -1;
    for (const u of comps[c])
      for (let k = adj.offsets[u]; k < adj.offsets[u + 1]; k++) {
        const t = comp[adj.targets[k]];
        if (t !== c && d[t] > best) best = d[t];
      }
    d[c] = best + 1;
  }
  return d;
}

// The chain from `start`: inside a cycle, the fewest hops to a member that leaves for the next-deepest component,
// then that heaviest edge out.
function chain(I: QueryIndex, start: number, up: boolean) {
  const G = I.G;
  const S = I.components();
  const d = chainDepths(G, S, up);
  const adj = up ? G.in : G.out;
  const route = [start];
  let cur = start;
  for (;;) {
    const c = S.comp[cur];
    if (d[c] === 0) break;
    const prev = new Map<number, number>([[cur, -1]]);
    const queue = [cur];
    let exit: [number, number] | null = null;
    for (let h = 0; h < queue.length && !exit; h++) {
      const u = queue[h];
      let best = -1,
        weight = -Infinity;
      for (let k = adj.offsets[u]; k < adj.offsets[u + 1]; k++) {
        const v = adj.targets[k];
        if (S.comp[v] === c) {
          if (!prev.has(v)) {
            prev.set(v, u);
            queue.push(v);
          }
        } else if (d[S.comp[v]] === d[c] - 1 && adj.weights[k] > weight) {
          [best, weight] = [v, adj.weights[k]];
        }
      }
      if (best >= 0) exit = [u, best];
    }
    if (!exit) break;
    const inner: number[] = [];
    for (let u = exit[0]; u !== cur; u = prev.get(u) ?? cur) inner.push(u);
    route.push(...inner.reverse(), exit[1]);
    cur = exit[1];
  }
  return { route: route.map(v => I.ids[v]), depth: d[S.comp[start]] };
}

function criticalPath(I: QueryIndex, args: string[]): OpResult {
  const up = args.includes('--up');
  const rest = args.slice(1).filter(a => a !== '--up');
  if (rest.length > 1)
    throw new QueryError('critical path takes at most one file');
  if (!I.G.n) throw new QueryError('the file graph is empty');
  let start: number;
  if (rest.length) {
    const v = I.G.index.get(resolveFile(I, rest[0]));
    if (v === undefined)
      throw new QueryError(`${rest[0]} is not in the file graph`);
    start = v;
  } else {
    // The deepest file; ties go to the more important one.
    const d = chainDepths(I.G, I.components(), up);
    const S = I.components();
    start = 0;
    for (let v = 1; v < I.G.n; v++) {
      const [a, b] = [d[S.comp[v]], d[S.comp[start]]];
      if (
        a > b ||
        (a === b &&
          (metricOf(I, I.ids[v], 'pagerank') ?? 0) >
            (metricOf(I, I.ids[start], 'pagerank') ?? 0))
      )
        start = v;
    }
  }
  const { route, depth } = chain(I, start, up);
  // Rows read as dependencies: each file depends on the next (an upward chain is listed from its top).
  const ordered = up ? route.toReversed() : route;
  const rows = pathRows(I, ordered, 'forward');
  const edges = ordered.slice(1).map((id, i): Pair => [ordered[i], id]);
  const subject = I.ids[start];
  return {
    rows,
    edges,
    nodes: ordered,
    stats: {
      depth,
      files: ordered.length,
      hops: edges.length,
      direction: up ? 'up' : 'down',
    },
    summary: up
      ? `${subject}: the longest chain of dependents above it has ${plural(depth, 'level')} (${plural(edges.length, 'hop')} through ${plural(ordered.length, 'file')}); a change to it can travel up this chain`
      : `${subject}: the longest dependency chain below it has ${plural(depth, 'level')} (${plural(edges.length, 'hop')} through ${plural(ordered.length, 'file')}); it rests on every file of this chain`,
  };
}

const FORMS: Record<string, (I: QueryIndex, args: string[]) => OpResult> = {
  functions: criticalFunctions,
  points: criticalPoints,
  path: criticalPath,
};

export function critical(I: QueryIndex, { args }: QueryArgs): OpResult {
  const form = (args[0] ?? '').toLowerCase();
  if (!Object.hasOwn(FORMS, form))
    throw new QueryError(
      'critical needs functions [n|all], points [n|all] or path [<file>] [--up]'
    );
  return FORMS[form](I, args);
}
