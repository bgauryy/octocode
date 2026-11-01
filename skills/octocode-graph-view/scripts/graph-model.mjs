// The weighted file graph every metric reads: integer node ids and CSR adjacency (offsets + targets + weights).
// One weight rule, documented here only:
//   w(u→v) = [u imports v] + Σ call sites from u's symbols into v's symbols × evidence
//   evidence = 1 when the language server resolved the call (`both`, `lsp`), else the graph confidence
//              (high 1, medium 0.5, low 0.25; calls stitched between chunks are medium).
// An import is a structural dependency (1); calls measure how much of it is used. An edge is `strong` when an import
// or a call with full evidence (LSP-resolved or high confidence) backs it: reachability questions walk strong edges.
// Test-only edges (every witness is test code) are kept for display and left out of the production graph.
const CONFIDENCE = { high: 1, medium: 0.5, low: 0.25 };
const RESOLVED = new Set(['both', 'lsp']);
const fileOf = (id) => String(id).split('#')[0];

export const callEvidence = (c) => (RESOLVED.has(c.via) ? 1 : CONFIDENCE[c.confidence] ?? CONFIDENCE.medium);

// Weighted file edges: { source, target, weight, imports, calls, test, strong } per file pair.
export function fileEdges(D) {
  const by = new Map();
  const at = (u, v) => {
    const k = `${u}\0${v}`;
    if (!by.has(k)) by.set(k, { source: u, target: v, weight: 0, imports: 0, calls: 0, test: true, strong: false });
    return by.get(k);
  };
  for (const e of D.imports) {
    const x = at(e.source, e.target);
    x.imports = 1;
    x.strong = true;
    x.weight += 1;
    x.test &&= Boolean(e.test);
  }
  for (const c of D.calls) {
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

// CSR over the production edges of the nodes in `ids`: out and in adjacency with weights.
export function csr(ids, edges) {
  const index = new Map(ids.map((id, i) => [id, i]));
  const n = ids.length;
  const list = edges.filter((e) => !e.test && index.has(e.source) && index.has(e.target)).map((e) => [index.get(e.source), index.get(e.target), e.weight]);
  const build = (from, to) => {
    const offsets = new Int32Array(n + 1);
    for (const e of list) offsets[e[from] + 1]++;
    for (let i = 0; i < n; i++) offsets[i + 1] += offsets[i];
    const fill = offsets.slice(0, n);
    const targets = new Int32Array(list.length), weights = new Float64Array(list.length);
    for (const e of list) {
      const at = fill[e[from]]++;
      targets[at] = e[to];
      weights[at] = e[2];
    }
    return { offsets, targets, weights };
  };
  return { n, ids, index, m: list.length, out: build(0, 1), in: build(1, 0) };
}

// Undirected weighted view (w(u,v) + w(v,u)) as an adjacency map per node, for Louvain and k-core.
export function undirected(G) {
  const adj = Array.from({ length: G.n }, () => new Map());
  for (let u = 0; u < G.n; u++) for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
    const v = G.out.targets[k], w = G.out.weights[k];
    if (u === v) continue;
    adj[u].set(v, (adj[u].get(v) ?? 0) + w);
    adj[v].set(u, (adj[v].get(u) ?? 0) + w);
  }
  return adj;
}
