// Generated from packages/octocode-graph/src by scripts/build.mjs.
function scc(G) {
  const { n, out } = G;
  const index = new Int32Array(n).fill(-1), low = new Int32Array(n), comp = new Int32Array(n).fill(-1);
  const onStack = new Uint8Array(n), stack = [], comps = [];
  let next = 0;
  const open = (v, work) => {
    index[v] = low[v] = next++;
    stack.push(v);
    onStack[v] = 1;
    work.push([v, out.offsets[v]]);
  };
  const close = (v) => {
    const members = [];
    for (let w = -1; w !== v; ) {
      w = stack.pop();
      onStack[w] = 0;
      comp[w] = comps.length;
      members.push(w);
    }
    comps.push(members);
  };
  for (let s = 0; s < n; s++) {
    if (index[s] >= 0) continue;
    const work = [];
    open(s, work);
    while (work.length) {
      const top = work.at(-1), v = top[0];
      if (top[1] < out.offsets[v + 1]) {
        const w = out.targets[top[1]++];
        if (index[w] < 0) open(w, work);
        else if (onStack[w]) low[v] = Math.min(low[v], index[w]);
        continue;
      }
      work.pop();
      if (work.length)
        low[work.at(-1)[0]] = Math.min(low[work.at(-1)[0]], low[v]);
      if (low[v] === index[v]) close(v);
    }
  }
  return { comp, comps };
}
function depths(G, { comp, comps }) {
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
  return Int32Array.from(comp, (c) => d[c]);
}
function pagerank(G, { damping = 0.85, tolerance = 1e-10 } = {}) {
  const { n, out } = G;
  const strength = new Float64Array(n);
  for (let u = 0; u < n; u++)
    for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++)
      strength[u] += out.weights[k];
  let r = new Float64Array(n).fill(1 / n), next = new Float64Array(n), iterations = 0;
  for (let diff = Infinity; diff >= tolerance; iterations++) {
    let dangling = 0;
    for (let u = 0; u < n; u++) if (!strength[u]) dangling += r[u];
    next.fill((1 - damping) / n + damping * dangling / n);
    for (let u = 0; u < n; u++)
      for (let k = out.offsets[u]; k < out.offsets[u + 1]; k++)
        next[out.targets[k]] += damping * r[u] * out.weights[k] / strength[u];
    diff = 0;
    for (let u = 0; u < n; u++) diff += Math.abs(next[u] - r[u]);
    [r, next] = [next, r];
  }
  return { rank: r, iterations };
}
function betweenness(G) {
  const { n, out } = G;
  const cb = new Float64Array(n), sigma = new Float64Array(n), delta = new Float64Array(n);
  const dist = new Int32Array(n), order = new Int32Array(n);
  for (let s = 0; s < n; s++) {
    sigma.fill(0);
    dist.fill(-1);
    delta.fill(0);
    sigma[s] = 1;
    dist[s] = 0;
    let head = 0, tail = 0;
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
      cb
    });
  }
  const scale = n > 2 ? 1 / ((n - 1) * (n - 2)) : 1;
  return cb.map((x) => x * scale);
}
function accumulate(G, {
  s,
  order,
  dist,
  sigma,
  delta,
  cb
}) {
  for (let i = order.length - 1; i > 0; i--) {
    const w = order[i];
    for (let k = G.in.offsets[w]; k < G.in.offsets[w + 1]; k++) {
      const v = G.in.targets[k];
      if (dist[v] === dist[w] - 1)
        delta[v] += sigma[v] / sigma[w] * (1 + delta[w]);
    }
    if (w !== s) cb[w] += delta[w];
  }
}
function louvain(adj) {
  let graph = adj.map((m, i) => new Map([...m].filter(([j]) => j !== i)));
  let membership = adj.map((_, i) => i);
  for (; ; ) {
    const { community, moved } = localMoving(graph);
    if (!moved) break;
    const renumber = /* @__PURE__ */ new Map();
    for (const c of community)
      if (!renumber.has(c)) renumber.set(c, renumber.size);
    const remap = community.map((c) => renumber.get(c));
    membership = membership.map((c) => remap[c]);
    graph = aggregate(graph, remap, renumber.size);
  }
  return { membership, modularity: modularity(adj, membership) };
}
function localMoving(g) {
  const n = g.length;
  const k = g.map((m) => [...m.values()].reduce((a, w) => a + w, 0));
  const m2 = k.reduce((a, x) => a + x, 0);
  const community = Array.from({ length: n }, (_, i) => i), tot = [...k];
  let moved = false;
  for (let improved = m2 > 0; improved; ) {
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
function bestCommunity(g, i, {
  community,
  tot,
  k,
  m2
}) {
  const links = /* @__PURE__ */ new Map();
  for (const [j, w] of g[i])
    if (j !== i) links.set(community[j], (links.get(community[j]) ?? 0) + w);
  const own = community[i];
  const gain = (c) => (links.get(c) ?? 0) - (tot[c] - (c === own ? k[i] : 0)) * k[i] / m2;
  let best = own, bestGain = gain(own);
  for (const c of links.keys())
    if (gain(c) > bestGain + 1e-12) {
      best = c;
      bestGain = gain(c);
    }
  return best;
}
function aggregate(g, remap, size) {
  const next = Array.from({ length: size }, () => /* @__PURE__ */ new Map());
  g.forEach((m, i) => {
    for (const [j, w] of m)
      next[remap[i]].set(remap[j], (next[remap[i]].get(remap[j]) ?? 0) + w);
  });
  return next;
}
function modularity(adj, membership) {
  let m2 = 0;
  const tot = /* @__PURE__ */ new Map(), inner = /* @__PURE__ */ new Map();
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
    (q, c) => q + (inner.get(c) ?? 0) / m2 - (tot.get(c) / m2) ** 2,
    0
  );
}
function kcore(adj) {
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
  const vert = new Int32Array(n), pos = new Int32Array(n);
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
      const du = deg[u], pu = pos[u], pw = bin[du], w = vert[pw];
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
function prerequisiteOrder(graph, components, start) {
  const seen = /* @__PURE__ */ new Set([start]);
  const queue = [start];
  const crossings = [];
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
  const ids = [...new Set(queue.map((v) => components.comp[v]))].sort(
    (a, b) => a - b
  );
  return { groups: ids.map((c) => components.comps[c]), crossings };
}
export {
  betweenness,
  depths,
  kcore,
  louvain,
  modularity,
  pagerank,
  prerequisiteOrder,
  scc
};
