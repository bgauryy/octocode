import assert from 'node:assert/strict';
import test from 'node:test';
import { betweenness, depths, kcore, louvain, pagerank, scc } from './algorithms.mjs';
import { csr, fileEdges, undirected } from './graph-model.mjs';
import { computeMetrics } from './metrics.mjs';

// A graph from [source, target, weight] triples over node names.
function graph(names, triples) {
  return csr(names, triples.map(([source, target, weight = 1]) => ({ source, target, weight, test: false })));
}

test('Tarjan finds every cycle and orders components so successors come first', () => {
  const G = graph(['a', 'b', 'c', 'd', 'e'], [['a', 'b'], ['b', 'c'], ['c', 'a'], ['c', 'd'], ['d', 'e'], ['e', 'd']]);
  const S = scc(G);
  const sets = S.comps.map((m) => m.map((i) => G.ids[i]).toSorted((a, b) => a.localeCompare(b)).join(''));
  assert.deepEqual(sets, ['de', 'abc']);
  assert.deepEqual([...depths(G, S)], [1, 1, 1, 0, 0]);
});

test('dependency depth is the longest path to a node that depends on nothing', () => {
  const G = graph(['app', 'svc', 'util', 'log'], [['app', 'svc'], ['app', 'util'], ['svc', 'util'], ['util', 'log']]);
  assert.deepEqual([...depths(G, scc(G))], [3, 2, 1, 0]);
});

test('weighted PageRank sums to 1, ranks the shared dependency first and follows weights', () => {
  const G = graph(['a', 'b', 'c', 'x', 'y'], [['a', 'x', 1], ['b', 'x', 1], ['c', 'x', 1], ['c', 'y', 9]]);
  const { rank } = pagerank(G);
  assert.ok(Math.abs(rank.reduce((s, r) => s + r, 0) - 1) < 1e-9);
  assert.ok(rank[3] > rank[0] && rank[4] > rank[0]);
  const light = pagerank(graph(['a', 'b', 'c', 'x', 'y'], [['a', 'x', 1], ['b', 'x', 1], ['c', 'x', 1], ['c', 'y', 1]])).rank;
  assert.ok(rank[4] > light[4], 'a heavier edge carries more importance');
});

test('betweenness is highest on the bridge of a chain and zero at its ends', () => {
  const G = graph(['a', 'b', 'c', 'd'], [['a', 'b'], ['b', 'c'], ['c', 'd']]);
  const b = betweenness(G);
  assert.equal(b[0], 0);
  assert.equal(b[3], 0);
  assert.ok(b[1] > 0 && b[2] > 0);
  assert.ok(Math.abs(b[1] - b[2]) < 1e-12);
});

const clique = (xs) => xs.flatMap((u) => xs.filter((v) => v !== u).map((v) => [u, v, 3]));
test('Louvain separates two dense groups joined by one weak edge, with positive modularity', () => {
  const G = graph(['a', 'b', 'c', 'd', 'e', 'f'], [...clique(['a', 'b', 'c']), ...clique(['d', 'e', 'f']), ['c', 'd', 1]]);
  const { membership, modularity } = louvain(undirected(G));
  assert.equal(new Set(membership).size, 2);
  assert.equal(membership[0], membership[2]);
  assert.notEqual(membership[0], membership[3]);
  assert.ok(modularity > 0.3);
});

test('k-core gives the clique its core number and a tail core 1', () => {
  const pairs = [['a', 'b'], ['a', 'c'], ['a', 'd'], ['b', 'c'], ['b', 'd'], ['c', 'd'], ['d', 'e']];
  assert.deepEqual([...kcore(undirected(graph(['a', 'b', 'c', 'd', 'e'], pairs)))], [3, 3, 3, 3, 1]);
});

test('the weight rule: an import counts 1, each call adds its evidence, LSP-resolved calls count fully, tests stay out', () => {
  const D = {
    imports: [{ source: 'a', target: 'b' }, { source: 't', target: 'b', test: true }],
    calls: [
      { source: 'a#f', target: 'b#g', count: 2, confidence: 'high', via: 'graph' },
      { source: 'a#f', target: 'b#h', count: 1, confidence: 'low', via: 'graph' },
      { source: 'a#f', target: 'b#k', count: 1, confidence: 'medium', via: 'both' },
      { source: 'a#f', target: 'a#x', count: 5, confidence: 'high', via: 'graph' },
    ],
  };
  const edges = fileEdges(D);
  assert.deepEqual(edges.map((e) => [e.source, e.target, e.weight, e.imports, e.calls, e.test]), [['a', 'b', 1 + 2 + 0.25 + 1, 1, 4, false], ['t', 'b', 1, 1, 0, true]]);
});

test('metrics land on every file, cross-package cycles are found, and Martin instability is per chunk', () => {
  const files = ['p/a.ts', 'p/b.ts', 'q/c.ts'].map((id) => ({ id, chunk: id.split('/')[0] }));
  const D = { files, imports: [{ source: 'p/a.ts', target: 'q/c.ts' }, { source: 'q/c.ts', target: 'p/b.ts' }, { source: 'p/b.ts', target: 'p/a.ts' }], calls: [] };
  const M = computeMetrics(D);
  assert.equal(M.cycles.length, 1);
  assert.equal(M.cycles[0].size, 3);
  assert.ok(D.files.every((f) => f.metrics && f.metrics.cycle === 0));
  const q = M.packages.find((p) => p.chunk === 'q');
  assert.deepEqual([q.ca, q.ce, q.instability], [1, 1, 0.5]);
  assert.equal(D.fileEdges.length, 3);
});

test('communities that share a folder are named by their most important file', () => {
  const files = ['s/a.ts', 's/b.ts', 's/c.ts', 's/d.ts'].map((id) => ({ id, chunk: 's' }));
  const D = { files, imports: [{ source: 's/a.ts', target: 's/b.ts' }, { source: 's/b.ts', target: 's/a.ts' }, { source: 's/c.ts', target: 's/d.ts' }, { source: 's/d.ts', target: 's/c.ts' }], calls: [] };
  const labels = computeMetrics(D).communities.map((c) => c.label);
  assert.equal(new Set(labels).size, labels.length);
  assert.ok(labels.every((l) => l.startsWith('s · ')));
});
