import type { Graph, Neighbors } from './graph-model.ts';

export interface Components {
  comp: Int32Array;
  comps: number[][];
}
// Standard graph algorithms over the CSR graph of graph-model.mjs. Each is the textbook form, iterative (no
// recursion depth limits), deterministic (fixed node order) and runs to its own completion criterion.

// Tarjan's strongly connected components. Components come out in reverse topological order: each one after every
// component it reaches. comp[v] is v's component index.
export function scc(G: Graph): Components {
  const { n, out } = G;
  const index = new Int32Array(n).fill(-1),
    low = new Int32Array(n),
    comp = new Int32Array(n).fill(-1);
  const onStack = new Uint8Array(n),
    stack: number[] = [],
    comps: number[][] = [];
  let next = 0;
  const open = (v: number, work: [number, number][]) => {
    index[v] = low[v] = next++;
    stack.push(v);
    onStack[v] = 1;
    work.push([v, out.offsets[v]]);
  };
  const close = (v: number) => {
    const members = [];
    for (let w = -1; w !== v;) {
      w = stack.pop()!;
      onStack[w] = 0;
      comp[w] = comps.length;
      members.push(w);
    }
    comps.push(members);
  };
  for (let s = 0; s < n; s++) {
    if (index[s] >= 0) continue;
    const work: [number, number][] = [];
    open(s, work);
    while (work.length) {
      const top = work.at(-1)!,
        v = top[0];
      if (top[1] < out.offsets[v + 1]) {
        const w = out.targets[top[1]++];
        if (index[w] < 0) open(w, work);
        else if (onStack[w]) low[v] = Math.min(low[v], index[w]);
        continue;
      }
      work.pop();
      if (work.length)
        low[work.at(-1)![0]] = Math.min(low[work.at(-1)![0]], low[v]);
      if (low[v] === index[v]) close(v);
    }
  }
  return { comp, comps };
}

// The cycle groups of a graph, as test-utils/graph-algorithms/SPEC.md defines them: components of several nodes, or of
// one node with an edge to itself; each as its members' indices in ascending id order; largest group first, then by
// smallest member id.
export function cycleGroups(G: Graph, { comps }: Components) {
  const looped = (v: number) => {
    for (let k = G.out.offsets[v]; k < G.out.offsets[v + 1]; k++)
      if (G.out.targets[k] === v) return true;
    return false;
  };
  const byId = (a: number, b: number) =>
    G.ids[a] < G.ids[b] ? -1 : G.ids[a] > G.ids[b] ? 1 : 0;
  return comps
    .filter(m => m.length > 1 || looped(m[0]))
    .map(m => m.toSorted(byId))
    .toSorted((a, b) => b.length - a.length || byId(a[0], b[0]));
}

// The shortest cycle through `s` inside its component `members` (breadth-first from s until an edge returns to s). The
// ring starts at s and lists each node once; ties resolve by neighbours in ascending id order, which the CSR holds.
// null when s is on no cycle.
export function shortestRing(G: Graph, s: number, members: Set<number>) {
  const { out } = G;
  const prev = new Map([[s, -1]]);
  for (let frontier = [s]; frontier.length;) {
    const next: number[] = [];
    for (const u of frontier)
      for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
        const v = out.targets[k];
        if (v === s) {
          const ring = [u];
          while (ring[ring.length - 1] !== s)
            ring.push(prev.get(ring.at(-1)!)!);
          return ring.reverse();
        }
        if (members.has(v) && !prev.has(v)) {
          prev.set(v, u);
          next.push(v);
        }
      }
    frontier = next;
  }
  return null;
}

// Fewest-hop path s → t, as node indices (null when t is unreachable); `blocked` nodes are never entered. Ties resolve
// by neighbours in ascending id order, which the CSR holds.
export function fewestHopPath(
  G: Graph,
  s: number,
  t: number,
  blocked?: Uint8Array
) {
  const { out } = G;
  const prev = new Int32Array(G.n).fill(-1);
  const queue = [s];
  prev[s] = s;
  for (let head = 0; head < queue.length && prev[t] < 0; head++) {
    const u = queue[head];
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++) {
      const v = out.targets[k];
      if (prev[v] >= 0 || blocked?.[v]) continue;
      prev[v] = u;
      queue.push(v);
    }
  }
  if (prev[t] < 0) return null;
  const route = [t];
  while (route[route.length - 1] !== s) route.push(prev[route.at(-1)!]);
  return route.reverse();
}

// Dependency depth: the longest path from a node's component to a sink of the condensation DAG (0 = depends on
// nothing in the graph). Reverse topological order means every successor is already final.
export function depths(G: Graph, { comp, comps }: Components) {
  const d = new Int32Array(comps.length);
  comps.forEach((members, c) => {
    let best = -1;
    for (const u of members)
      for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
        const t = comp[G.out.targets[k]];
        if (t !== c && d[t] > best) best = d[t];
      }
    d[c] = best + 1;
  });
  return Int32Array.from(comp, c => d[c]);
}

// Weighted PageRank: importance flows from a file to what it depends on, split by edge weight; files with no
// outgoing edges spread theirs evenly. Power iteration until the L1 change is below `tolerance`.
export function pagerank(G: Graph, { damping = 0.85, tolerance = 1e-10 } = {}) {
  const { n, out } = G;
  const strength = new Float64Array(n);
  for (let u = 0; u < n; u++)
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++)
      strength[u] += out.weights[k];
  let r = new Float64Array(n).fill(1 / n),
    next = new Float64Array(n),
    iterations = 0;
  for (let diff = Infinity; diff >= tolerance; iterations++) {
    let dangling = 0;
    for (let u = 0; u < n; u++) if (!strength[u]) dangling += r[u];
    next.fill((1 - damping) / n + (damping * dangling) / n);
    for (let u = 0; u < n; u++)
      for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++)
        next[out.targets[k]] += (damping * r[u] * out.weights[k]) / strength[u];
    diff = 0;
    for (let u = 0; u < n; u++) diff += Math.abs(next[u] - r[u]);
    [r, next] = [next, r];
  }
  return { rank: r, iterations };
}

// Brandes betweenness on directed shortest paths (hop count), normalized by (n-1)(n-2). Predecessors are read back
// from the in-adjacency (dist[v] = dist[w] - 1), so no per-source lists are allocated.
export function betweenness(G: Graph) {
  const { n, out } = G;
  const cb = new Float64Array(n),
    sigma = new Float64Array(n),
    delta = new Float64Array(n);
  const dist = new Int32Array(n),
    order = new Int32Array(n);
  for (let s = 0; s < n; s++) {
    sigma.fill(0);
    dist.fill(-1);
    delta.fill(0);
    sigma[s] = 1;
    dist[s] = 0;
    let head = 0,
      tail = 0;
    order[tail++] = s;
    while (head < tail) {
      const v = order[head++];
      for (let k = out.offsets[v]; k < out.offsets[v + 1]; k++) {
        const w = out.targets[k];
        if (dist[w] < 0) {
          dist[w] = dist[v] + 1;
          order[tail++] = w;
        }
        if (dist[w] === dist[v] + 1) sigma[w] += sigma[v];
      }
    }
    accumulate(G, {
      s,
      order: order.subarray(0, tail),
      dist,
      sigma,
      delta,
      cb,
    });
  }
  const scale = n > 2 ? 1 / ((n - 1) * (n - 2)) : 1;
  return cb.map(x => x * scale);
}
function accumulate(
  G: Graph,
  {
    s,
    order,
    dist,
    sigma,
    delta,
    cb,
  }: {
    s: number;
    order: Int32Array;
    dist: Int32Array;
    sigma: Float64Array;
    delta: Float64Array;
    cb: Float64Array;
  }
) {
  for (let i = order.length - 1; i > 0; i--) {
    const w = order[i];
    for (let k = G.in.offsets[w]; k < G.in.offsets[w + 1]; k++) {
      const v = G.in.targets[k];
      if (dist[v] === dist[w] - 1)
        delta[v] += (sigma[v] / sigma[w]) * (1 + delta[w]);
    }
    if (w !== s) cb[w] += delta[w];
  }
}

// Louvain modularity communities on the undirected weighted graph (adj[i]: Map j -> weight, symmetric). Local moving
// until no node improves, then aggregate communities into nodes; repeat until a level moves nothing.
export function louvain(adj: Neighbors) {
  let graph = adj.map((m, i) => new Map([...m].filter(([j]) => j !== i)));
  let membership = adj.map((_, i) => i);
  for (;;) {
    const { community, moved } = localMoving(graph);
    if (!moved) break;
    const renumber = new Map<number, number>();
    for (const c of community)
      if (!renumber.has(c)) renumber.set(c, renumber.size);
    const remap = community.map(c => renumber.get(c)!);
    membership = membership.map(c => remap[c]);
    graph = aggregate(graph, remap, renumber.size);
  }
  return { membership, modularity: modularity(adj, membership) };
}
function localMoving(g: Neighbors) {
  const n = g.length;
  const k = g.map(m => [...m.values()].reduce((a, w) => a + w, 0));
  const m2 = k.reduce((a, x) => a + x, 0);
  const community = Array.from({ length: n }, (_, i) => i),
    tot = [...k];
  let moved = false;
  for (let improved = m2 > 0; improved;) {
    improved = false;
    for (let i = 0; i < n; i++) {
      const best = bestCommunity(g, i, { community, tot, k, m2 });
      if (best !== community[i]) {
        tot[community[i]] -= k[i];
        tot[best] += k[i];
        community[i] = best;
        improved = moved = true;
      }
    }
  }
  return { community, moved };
}
function bestCommunity(
  g: Neighbors,
  i: number,
  {
    community,
    tot,
    k,
    m2,
  }: { community: number[]; tot: number[]; k: number[]; m2: number }
) {
  const links = new Map<number, number>();
  for (const [j, w] of g[i])
    if (j !== i) links.set(community[j], (links.get(community[j]) ?? 0) + w);
  const own = community[i];
  const gain = (c: number) =>
    (links.get(c) ?? 0) - ((tot[c] - (c === own ? k[i] : 0)) * k[i]) / m2;
  let best = own,
    bestGain = gain(own);
  for (const c of links.keys())
    if (gain(c) > bestGain + 1e-12) {
      best = c;
      bestGain = gain(c);
    }
  return best;
}
function aggregate(g: Neighbors, remap: number[], size: number): Neighbors {
  const next = Array.from({ length: size }, () => new Map<number, number>());
  g.forEach((m, i) => {
    for (const [j, w] of m)
      next[remap[i]].set(remap[j], (next[remap[i]].get(remap[j]) ?? 0) + w);
  });
  return next;
}
export function modularity(adj: Neighbors, membership: number[]) {
  let m2 = 0;
  const tot = new Map<number, number>(),
    inner = new Map<number, number>();
  adj.forEach((m, i) => {
    for (const [j, w] of m) {
      if (j === i) continue;
      m2 += w;
      tot.set(membership[i], (tot.get(membership[i]) ?? 0) + w);
      if (membership[i] === membership[j])
        inner.set(membership[i], (inner.get(membership[i]) ?? 0) + w);
    }
  });
  if (!m2) return 0;
  return [...tot.keys()].reduce(
    (q, c) => q + (inner.get(c) ?? 0) / m2 - (tot.get(c)! / m2) ** 2,
    0
  );
}

// k-core numbers (Batagelj–Zaversnik bucket algorithm) on the undirected simple graph.
export function kcore(adj: Neighbors) {
  const n = adj.length;
  const deg = adj.map((m, i) => m.size - (m.has(i) ? 1 : 0));
  const maxd = deg.reduce((max, degree) => Math.max(max, degree), 0);
  const bin = new Int32Array(maxd + 1);
  for (const d of deg) bin[d]++;
  for (let d = 0, start = 0; d <= maxd; d++) {
    const c = bin[d];
    bin[d] = start;
    start += c;
  }
  const vert = new Int32Array(n),
    pos = new Int32Array(n);
  for (let v = 0; v < n; v++) {
    pos[v] = bin[deg[v]]++;
    vert[pos[v]] = v;
  }
  for (let d = maxd; d > 0; d--) bin[d] = bin[d - 1];
  bin[0] = 0;
  for (let i = 0; i < n; i++) {
    const v = vert[i];
    for (const u of adj[v].keys()) {
      if (u === v || deg[u] <= deg[v]) continue;
      const du = deg[u],
        pu = pos[u],
        pw = bin[du],
        w = vert[pw];
      if (u !== w) {
        pos[u] = pw;
        vert[pu] = w;
        pos[w] = pu;
        vert[pw] = u;
      }
      bin[du]++;
      deg[u]--;
    }
  }
  return Int32Array.from(deg);
}

// Tarjan emits a component after all of its prerequisites. Restrict the returned
// groups to the dependency closure of the seed while preserving that ordering.
export function prerequisiteOrder(
  graph: Graph,
  components: Components,
  start: number
) {
  const seen = new Set<number>([start]);
  const queue = [start];
  const crossings: [number, number][] = [];
  for (let head = 0; head < queue.length; head++) {
    const u = queue[head];
    for (let k = graph.out.offsets[u]; k < graph.out.offsets[u + 1]; k++) {
      const v = graph.out.targets[k];
      if (components.comp[u] !== components.comp[v]) crossings.push([u, v]);
      if (!seen.has(v)) {
        seen.add(v);
        queue.push(v);
      }
    }
  }
  const ids = [...new Set(queue.map(v => components.comp[v]))].sort(
    (a, b) => a - b
  );
  return { groups: ids.map(c => components.comps[c]), crossings };
}
