// The shared graph-algorithm conformance suite: the same cases and expected answers the Rust engine reads
// (test-utils/graph-algorithms/SPEC.md). Every case also runs with its nodes and edges reversed and rotated: the answer
// never depends on input order.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { cycleGroups, fewestHopPath, pagerank, scc, shortestRing } from '../src/algorithms.ts';
import { csr } from '../src/graph-model.ts';
import { walk } from '../src/runtime/query-ops.ts';

const cases = JSON.parse(
  readFileSync(new URL('../../../test-utils/graph-algorithms/cases.json', import.meta.url), 'utf8')
);

const rotate = (list, by) => (list.length ? [...list.slice(by % list.length), ...list.slice(0, by % list.length)] : list);
const orders = [
  { label: 'as written', arrange: (c) => c },
  { label: 'reversed', arrange: (c) => ({ ...c, nodes: c.nodes.toReversed(), edges: c.edges.toReversed() }) },
  { label: 'rotated', arrange: (c) => ({ ...c, nodes: rotate(c.nodes, 2), edges: rotate(c.edges, 3) }) },
];

for (const base of cases)
  for (const { label, arrange } of orders)
    test(`conformance: ${String(base.name)} (${label})`, () => {
      const c = arrange(base);
      const G = csr(c.nodes, c.edges.map(([source, target]) => ({ source, target, weight: 1, test: false })));
      const S = scc(G);
      const names = (members) => members.map((v) => G.ids[v]);
      const { expected } = base;

      assert.deepEqual(cycleGroups(G, S).map(names), expected.cycles, 'cycle groups');

      const groups = cycleGroups(G, S);
      for (const id of c.nodes) {
        const s = G.index.get(id);
        const members = new Set(S.comps[S.comp[s]]);
        const ring = shortestRing(G, s, members);
        if (id in expected.ring) assert.deepEqual(ring && names(ring), expected.ring[id], `ring through ${id}`);
      }
      for (const group of groups) {
        const smallest = names(group)[0];
        const ring = shortestRing(G, G.index.get(smallest), new Set(group));
        assert.equal(names(ring)[0], smallest, 'a ring with no queried node starts at the smallest member');
      }

      for (const { from, to, path } of expected.path) {
        const route = fewestHopPath(G, G.index.get(from), G.index.get(to));
        assert.deepEqual(route && names(route), path, `path ${from} -> ${to}`);
      }

      for (const { seeds, depths } of expected.reach) {
        const { depth } = walk({ G }, seeds, { dir: 'in', maxDepth: Infinity });
        const reached = Object.fromEntries(
          [...depth].filter(([, d]) => d > 0).map(([v, d]) => [G.ids[v], d])
        );
        assert.deepEqual(reached, depths, `reverse reach of ${String(seeds)}`);
      }

      const { rank } = pagerank(G);
      for (const [id, value] of Object.entries(expected.pagerank))
        assert.ok(Math.abs(rank[G.index.get(id)] - value) < 1e-6, `pagerank ${id}: ${rank[G.index.get(id)]} vs ${value}`);
      assert.equal(rank.length, c.nodes.length);
    });
