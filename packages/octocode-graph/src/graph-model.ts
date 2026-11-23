import { fileOf } from './ids.ts';
// The weighted file graph every metric reads: integer node ids and CSR adjacency (offsets + targets + weights).
// One weight rule, documented here only:
//   w(u→v) = [u imports v] + Σ call sites from u's symbols into v's symbols × evidence
//   evidence = 1 when the language server resolved the call (`both`, `lsp`), else the graph confidence
//              (high 1, medium 0.5, low 0.25; calls stitched between chunks are medium).
// An import is a structural dependency (1); calls measure how much of it is used. An edge is `strong` when an import
// or a call with full evidence (LSP-resolved or high confidence) backs it: reachability questions walk strong edges.
// Test-only edges (every witness is test code) are kept for display and left out of the production graph.
export interface Witness {
  source: string;
  target: string;
  test?: boolean;
  count?: number;
  via?: string;
  confidence?: string;
}
export interface FileEdge {
  source: string;
  target: string;
  weight: number;
  imports: number;
  calls: number;
  test: boolean;
  strong: boolean;
}
export interface Witnesses {
  imports: readonly Witness[];
  calls: readonly Witness[];
}
export type Graph = ReturnType<typeof csr>;
export type Neighbors = Map<number, number>[];

const CONFIDENCE: Record<string, number> = { high: 1, medium: 0.5, low: 0.25 };
const RESOLVED = new Set(['both', 'lsp']);

export const callEvidence = (c: Witness) =>
  RESOLVED.has(c.via ?? '')
    ? 1
    : (CONFIDENCE[c.confidence ?? 'medium'] ?? CONFIDENCE.medium);

// Weighted file edges: { source, target, weight, imports, calls, test, strong } per file pair.
export function fileEdges(D: Witnesses, includeTests = true) {
  const by = new Map<string, FileEdge>();
  const at = (u: string, v: string) => {
    const k = `${u}\0${v}`;
    if (!by.has(k))
      by.set(k, {
        source: u,
        target: v,
        weight: 0,
        imports: 0,
        calls: 0,
        test: true,
        strong: false,
      });
    return by.get(k)!;
  };
  for (const e of D.imports) {
    if (!includeTests && e.test) continue;
    const x = at(e.source, e.target);
    x.imports = 1;
    x.strong = true;
    x.weight += 1;
    x.test &&= Boolean(e.test);
  }
  for (const c of D.calls) {
    if (!includeTests && c.test) continue;
    const [u, v] = [fileOf(c.source), fileOf(c.target)];
    if (u === v) continue;
    const x = at(u, v);
    const n = c.count ?? 1;
    x.calls += n;
    x.weight += n * callEvidence(c);
    x.strong ||= callEvidence(c) === 1;
    x.test &&= Boolean(c.test);
  }
  return [...by.values()];
}

// CSR over the production edges of the nodes in `ids`: out and in adjacency with weights. A node's neighbours are in
// ascending id order (test-utils/graph-algorithms/SPEC.md: the tie rule), so no answer depends on the order the nodes
// or edges came in.
export function csr(
  ids: string[],
  edges: readonly Pick<FileEdge, 'source' | 'target' | 'weight' | 'test'>[]
) {
  const index = new Map(ids.map((id, i) => [id, i]));
  const n = ids.length;
  const list = edges
    .filter(e => !e.test && index.has(e.source) && index.has(e.target))
    .map(e => [index.get(e.source)!, index.get(e.target)!, e.weight]);
  const build = (from: number, to: number) => {
    const sorted = list.toSorted((a, b) => {
      const [x, y] = [ids[a[to]], ids[b[to]]];
      return x < y ? -1 : x > y ? 1 : 0;
    });
    const offsets = new Int32Array(n + 1);
    for (const e of sorted) offsets[e[from] + 1]++;
    for (let i = 0; i < n; i++) offsets[i + 1] += offsets[i];
    const fill = offsets.slice(0, n);
    const targets = new Int32Array(sorted.length),
      weights = new Float64Array(sorted.length);
    for (const e of sorted) {
      const at = fill[e[from]]++;
      targets[at] = e[to];
      weights[at] = e[2];
    }
    return { offsets, targets, weights };
  };
  return { n, ids, index, m: list.length, out: build(0, 1), in: build(1, 0) };
}

// Undirected weighted view (w(u,v) + w(v,u)) as an adjacency map per node, for Louvain and k-core.
export function undirected(G: Graph): Neighbors {
  const adj = Array.from({ length: G.n }, () => new Map<number, number>());
  for (let u = 0; u < G.n; u++)
    for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
      const v = G.out.targets[k],
        w = G.out.weights[k];
      if (u === v) continue;
      adj[u].set(v, (adj[u].get(v) ?? 0) + w);
      adj[v].set(u, (adj[v].get(u) ?? 0) + w);
    }
  return adj;
}
