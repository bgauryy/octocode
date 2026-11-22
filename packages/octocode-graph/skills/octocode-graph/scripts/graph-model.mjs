// Generated from packages/octocode-graph/src by scripts/build.mjs.
const CONFIDENCE = { high: 1, medium: 0.5, low: 0.25 };
const RESOLVED = /* @__PURE__ */ new Set(["both", "lsp"]);
const fileOf = (id) => id.split("#")[0];
const callEvidence = (c) => RESOLVED.has(c.via ?? "") ? 1 : CONFIDENCE[c.confidence ?? "medium"] ?? CONFIDENCE.medium;
function fileEdges(D, includeTests = true) {
  const by = /* @__PURE__ */ new Map();
  const at = (u, v) => {
    const k = `${u}\0${v}`;
    if (!by.has(k))
      by.set(k, {
        source: u,
        target: v,
        weight: 0,
        imports: 0,
        calls: 0,
        test: true,
        strong: false
      });
    return by.get(k);
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
function csr(ids, edges) {
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
function undirected(G) {
  const adj = Array.from({ length: G.n }, () => /* @__PURE__ */ new Map());
  for (let u = 0; u < G.n; u++)
    for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
      const v = G.out.targets[k], w = G.out.weights[k];
      if (u === v) continue;
      adj[u].set(v, (adj[u].get(v) ?? 0) + w);
      adj[v].set(u, (adj[v].get(u) ?? 0) + w);
    }
  return adj;
}
export {
  callEvidence,
  csr,
  fileEdges,
  undirected
};
