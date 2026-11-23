// How an entry reaches an outcome (query-index.ts graphs):
//   region <entry> <outcome>   everything that can take part in a route from entry to outcome: reachable forward
//                              from the entry and backward from the outcome. Files by default; two symbols
//                              (`file#name`) ask the call graph. `mustPass` marks the steps on every modeled route
//                              (the outcome's dominators from the entry), the checkpoints a change cannot route
//                              around. Unresolved dispatch and unbound calls can leave a real route out.
import type { Graph } from '../graph-model.ts';
import { fileOf } from '../ids.ts';
import { callGraph } from './query-critical.ts';
import {
  QueryError,
  resolveFile,
  resolveSymbol,
  type QueryIndex,
} from './query-index.ts';
import {
  fileRow,
  plural,
  type OpResult,
  type Pair,
  type QueryArgs,
  type Row,
} from './query-ops.ts';

// Breadth-first distances from `start` along `adj` (out or in); -1 where it does not reach.
function distances(G: Graph, start: number, adj: Graph['out']) {
  const d = new Int32Array(G.n).fill(-1);
  d[start] = 0;
  const queue = [start];
  for (let head = 0; head < queue.length; head++) {
    const u = queue[head];
    for (let k = adj.offsets[u]; k < adj.offsets[u + 1]; k++) {
      const v = adj.targets[k];
      if (d[v] < 0) {
        d[v] = d[u] + 1;
        queue.push(v);
      }
    }
  }
  return d;
}

// Immediate dominators from `s` over the nodes in `inside` (Cooper, Harvey and Kennedy, "A Simple, Fast Dominance
// Algorithm"): a node dominates `t` when every route from `s` to `t` passes through it.
function dominators(G: Graph, s: number, inside: Uint8Array) {
  const order: number[] = [];
  const seen = new Uint8Array(G.n);
  // Iterative depth-first postorder from s within the region.
  const stack: [number, number][] = [[s, G.out.offsets[s]]];
  seen[s] = 1;
  while (stack.length) {
    const top = stack[stack.length - 1];
    const [u, k] = top;
    if (k < G.out.offsets[u + 1]) {
      top[1]++;
      const v = G.out.targets[k];
      if (inside[v] && !seen[v]) {
        seen[v] = 1;
        stack.push([v, G.out.offsets[v]]);
      }
    } else {
      order.push(u);
      stack.pop();
    }
  }
  const rpo = order.toReversed();
  const index = new Int32Array(G.n).fill(-1);
  rpo.forEach((v, i) => (index[v] = i));
  const idom = new Int32Array(G.n).fill(-1);
  idom[s] = s;
  const meet = (a: number, b: number) => {
    let [x, y] = [a, b];
    while (x !== y) {
      while (index[x] > index[y]) x = idom[x];
      while (index[y] > index[x]) y = idom[y];
    }
    return x;
  };
  for (let changed = true; changed;) {
    changed = false;
    for (const v of rpo) {
      if (v === s) continue;
      let next = -1;
      for (let k = G.in.offsets[v]; k < G.in.offsets[v + 1]; k++) {
        const p = G.in.targets[k];
        if (!inside[p] || idom[p] < 0) continue;
        next = next < 0 ? p : meet(p, next);
      }
      if (next >= 0 && idom[v] !== next) {
        idom[v] = next;
        changed = true;
      }
    }
  }
  return idom;
}

interface Endpoints {
  G: Graph;
  s: number;
  t: number;
  /** A row for node v: a file row, or the symbol's row at its file. */
  row: (v: number, extra: Record<string, unknown>) => Row;
  file: (v: number) => string;
  level: 'file' | 'function';
}

function endpoints(I: QueryIndex, a: string, b: string): Endpoints {
  if (a.includes('#') || b.includes('#')) {
    const [from, to] = [resolveSymbol(I, a), resolveSymbol(I, b)];
    const { G } = callGraph(I);
    const [s, t] = [G.index.get(from), G.index.get(to)];
    if (s === undefined || t === undefined)
      throw new QueryError(
        `${s === undefined ? from : to} takes part in no call the graph bound`
      );
    const symbols = I.symbols().byId;
    return {
      G,
      s,
      t,
      level: 'function',
      file: v => fileOf(G.ids[v]),
      row: (v, extra) => {
        const id = G.ids[v];
        const info = symbols.get(id);
        return fileRow(I, fileOf(id), {
          line: info?.line ?? undefined,
          symbol: id,
          name: info?.name ?? id.split('#').pop(),
          kind: info?.kind ?? 'symbol',
          ...extra,
        });
      },
    };
  }
  const [from, to] = [resolveFile(I, a), resolveFile(I, b)];
  const [s, t] = [I.G.index.get(from), I.G.index.get(to)];
  if (s === undefined || t === undefined)
    throw new QueryError(
      `${s === undefined ? from : to} is not in the file graph`
    );
  return {
    G: I.G,
    s,
    t,
    level: 'file',
    file: v => I.ids[v],
    row: (v, extra) => fileRow(I, I.ids[v], extra),
  };
}

export function region(I: QueryIndex, { args }: QueryArgs): OpResult {
  if (args.length !== 2)
    throw new QueryError(
      'region needs an entry and an outcome: region <file|file#symbol> <file|file#symbol>'
    );
  const E = endpoints(I, args[0], args[1]);
  const { G, s, t } = E;
  const forward = distances(G, s, G.out);
  const backward = distances(G, t, G.in);
  const [entry, outcome] = [G.ids[s], G.ids[t]];
  if (forward[t] < 0)
    return {
      rows: [],
      edges: [],
      nodes: [E.file(s), E.file(t)],
      stats: { level: E.level, size: 0, mustPass: 0 },
      summary: `no modeled route ${entry} → ${outcome}${distances(G, t, G.out)[s] >= 0 ? `; ${outcome} reaches ${entry} instead` : ''}`,
    };
  const inside = new Uint8Array(G.n);
  const members: number[] = [];
  for (let v = 0; v < G.n; v++)
    if (forward[v] >= 0 && backward[v] >= 0) {
      inside[v] = 1;
      members.push(v);
    }
  const idom = dominators(G, s, inside);
  const must = new Set<number>();
  for (let v = idom[t]; v !== s && v >= 0; v = idom[v]) must.add(v);
  const rows = members
    .sort(
      (a, b) =>
        forward[a] - forward[b] ||
        backward[b] - backward[a] ||
        (G.ids[a] < G.ids[b] ? -1 : 1)
    )
    .map(v =>
      E.row(v, {
        fromEntry: forward[v],
        toOutcome: backward[v],
        mustPass: v === s || v === t || must.has(v),
      })
    );
  const edges = new Map<string, Pair>();
  for (const u of members)
    for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
      const v = G.out.targets[k];
      const [a, b] = [E.file(u), E.file(v)];
      if (inside[v] && a !== b) edges.set(`${a}\0${b}`, [a, b]);
    }
  const checkpoints = [...must].map(v => G.ids[v]);
  return {
    rows,
    edges: [...edges.values()],
    nodes: [...new Set(members.map(E.file))],
    stats: {
      level: E.level,
      size: members.length,
      shortest: forward[t],
      mustPass: checkpoints,
    },
    summary: `${plural(members.length, E.level === 'file' ? 'file' : 'function')} can take part in ${entry} → ${outcome} (shortest ${plural(forward[t], 'hop')}); ${checkpoints.length ? `every modeled route passes ${checkpoints.join(', ')}` : 'no single step lies on every modeled route'}${E.level === 'function' ? '; unresolved dispatch can leave a real route out' : ''}`,
  };
}
