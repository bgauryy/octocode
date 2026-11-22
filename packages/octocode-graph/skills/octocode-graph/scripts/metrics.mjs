// Graph metrics for the representation: every number comes from a standard algorithm over the weighted file
// graph (graph-model.mjs), never from prose rules. Per-file values land on D.files[i].metrics; sets (cycles,
// communities, packages) and timings land on D.metrics; the weighted edges on D.fileEdges.
import { csr, fileEdges, undirected } from './graph-model.mjs';
import { betweenness, depths, kcore, louvain, pagerank, scc } from './algorithms.mjs';

const round = (x, digits = 6) => Number(x.toFixed(digits));
const byText = (a, b) => a.localeCompare(b);

function strengths(G) {
  const inW = new Float64Array(G.n), outW = new Float64Array(G.n);
  for (let u = 0; u < G.n; u++) for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
    outW[u] += G.out.weights[k];
    inW[G.out.targets[k]] += G.out.weights[k];
  }
  return { inW, outW };
}

// Strongly connected components with more than one file: dependency cycles across the whole stitched graph.
function cycleSets(G, S, { rank, D }) {
  return S.comps.filter((m) => m.length > 1).map((members) => {
    const set = new Set(members);
    let weight = 0;
    for (const u of members) for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) if (set.has(G.out.targets[k])) weight += G.out.weights[k];
    // Languages and chunks say what kind of cycle it is: Rust modules of one crate may legally reference each other.
    const languages = [...new Set(members.map((i) => D.files[i].language ?? 'unknown'))].toSorted(byText);
    const chunks = [...new Set(members.map((i) => D.files[i].chunk ?? '-'))].toSorted(byText);
    return { size: members.length, weight: round(weight, 3), languages, chunks, members: members.toSorted((a, b) => rank[b] - rank[a]).map((i) => G.ids[i]) };
  }).toSorted((a, b) => b.size - a.size || b.weight - a.weight).map((c, id) => ({ id, ...c }));
}

// A community's name: the deepest folder all its members share, else the folder of its most important member.
function labelOf(ids) {
  const dirs = ids.map((id) => id.split('/').slice(0, -1));
  const shared = dirs[0].filter((seg, i) => dirs.every((d) => d[i] === seg));
  return shared.length ? shared.join('/') : dirs[0].join('/') || '(root)';
}

// Louvain communities with their weight inside and across, and how much they coincide with declared packages.
function communitySets(G, D, { membership, rank, adj }) {
  const groups = Map.groupBy(membership.map((c, i) => [c, i]), ([c]) => c);
  return [...groups.values()].map((rows) => {
    const members = rows.map(([, i]) => i).toSorted((a, b) => rank[b] - rank[a]);
    const inside = new Set(members);
    let internal = 0, external = 0;
    for (const u of members) for (const [v, w] of adj[u]) if (inside.has(v)) internal += w / 2; else external += w;
    const byChunk = Map.groupBy(members, (i) => D.files[i].chunk ?? '-');
    const [chunk, list] = [...byChunk].toSorted((a, b) => b[1].length - a[1].length)[0];
    return { label: labelOf(members.map((i) => G.ids[i])), size: members.length, internal: round(internal, 3), external: round(external, 3), chunk, purity: round(list.length / members.length, 3), members: members.map((i) => G.ids[i]) };
  }).toSorted((a, b) => b.size - a.size).map((c, id) => ({ id, ...c }));
}

// Martin package metrics per chunk: afferent coupling Ca (files outside depending on it), efferent Ce (files outside
// it depends on), instability I = Ce / (Ca + Ce).
function packageSets(G, D) {
  const chunkOf = (i) => D.files[i].chunk ?? '-';
  const pk = new Map();
  const at = (c) => pk.get(c) ?? pk.set(c, { chunk: c, files: 0, ca: new Set(), ce: new Set(), inWeight: 0, outWeight: 0 }).get(c);
  for (let u = 0; u < G.n; u++) {
    at(chunkOf(u)).files++;
    for (let k = G.out.offsets[u]; k < G.out.offsets[u + 1]; k++) {
      const v = G.out.targets[k];
      if (chunkOf(u) === chunkOf(v)) continue;
      at(chunkOf(u)).ce.add(v); at(chunkOf(u)).outWeight += G.out.weights[k];
      at(chunkOf(v)).ca.add(u); at(chunkOf(v)).inWeight += G.out.weights[k];
    }
  }
  return [...pk.values()].map((p) => ({ chunk: p.chunk, files: p.files, ca: p.ca.size, ce: p.ce.size, instability: p.ca.size + p.ce.size ? round(p.ce.size / (p.ca.size + p.ce.size), 3) : null, inWeight: round(p.inWeight, 3), outWeight: round(p.outWeight, 3) })).toSorted((a, b) => b.ca + b.ce - (a.ca + a.ce));
}

// Communities sharing a folder label are told apart by their most important file (members are PageRank-ordered).
function distinctLabels(communities) {
  const count = new Map();
  for (const c of communities) count.set(c.label, (count.get(c.label) ?? 0) + 1);
  for (const c of communities) if (count.get(c.label) > 1 && c.size > 1) c.label = `${c.label} · ${c.members[0].split('/').pop()}`;
  return communities;
}

function timed(timings, name, fn) {
  const t0 = performance.now();
  const out = fn();
  timings[name] = Math.round(performance.now() - t0);
  return out;
}

export function computeMetrics(D) {
  const timings = {};
  const edges = timed(timings, 'edges', () => fileEdges(D));
  const G = timed(timings, 'csr', () => csr(D.files.map((f) => f.id), edges));
  const S = timed(timings, 'scc', () => scc(G));
  const depth = timed(timings, 'depth', () => depths(G, S));
  const { rank, iterations } = timed(timings, 'pagerank', () => pagerank(G));
  const between = timed(timings, 'betweenness', () => betweenness(G));
  const adj = undirected(G);
  const { membership, modularity } = timed(timings, 'louvain', () => louvain(adj));
  const core = timed(timings, 'kcore', () => kcore(adj));
  const { inW, outW } = strengths(G);
  const cycles = cycleSets(G, S, { rank, D });
  const cycleOf = new Map(cycles.flatMap((c) => c.members.map((id) => [id, c.id])));
  D.files.forEach((f, i) => {
    f.metrics = { pagerank: round(rank[i], 8), betweenness: round(between[i]), inWeight: round(inW[i], 3), outWeight: round(outW[i], 3), depth: depth[i], core: core[i], community: membership[i], cycle: cycleOf.get(f.id) ?? null };
  });
  D.fileEdges = edges.map((e) => ({ ...e, weight: round(e.weight, 3) }));
  const communities = distinctLabels(communitySets(G, D, { membership, rank, adj }));
  // Community ids in data are the ranked list positions.
  const renumber = new Map(communities.map((c) => [membership[G.index.get(c.members[0])], c.id]));
  for (const f of D.files) f.metrics.community = renumber.get(f.metrics.community);
  D.metrics = {
    weightRule: 'w(u→v) = [u imports v] + Σ call sites × evidence (LSP-resolved 1; else confidence high 1, medium 0.5, low 0.25); test-only edges excluded',
    stats: { nodes: G.n, edges: G.m, weight: round(edges.filter((e) => !e.test).reduce((a, e) => a + e.weight, 0), 3), modularity: round(modularity, 4), cycles: cycles.length, largestCycle: cycles[0]?.size ?? 0, maxDepth: Math.max(0, ...depth), maxCore: Math.max(0, ...core), communities: communities.filter((c) => c.size > 1).length, isolated: communities.filter((c) => c.size === 1).length, pagerankIterations: iterations },
    cycles, communities, packages: packageSets(G, D), timings,
  };
  return D.metrics;
}
