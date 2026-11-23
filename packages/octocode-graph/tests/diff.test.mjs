// Structural diff of two saved graphs: renames, dependencies, cycles, package links, reach and the review order.
import assert from 'node:assert/strict';
import test from 'node:test';
import { computeMetrics } from '../src/runtime/metrics.ts';
import { diffGraphs, formatDiff, renames } from '../src/runtime/diff.ts';

const file = (id, chunk) => ({ id, label: id.split('/').pop(), chunk, language: 'javascript' });
const imp = (source, target) => ({ source, target, line: 1, lines: [1], confidence: 'high', test: false, count: 1, resolution: [] });
function graph(files, imports) {
  const D = { section: '/repo', files: files.map(([id, chunk]) => file(id, chunk)), imports: imports.map(([s, t]) => imp(s, t)), symbols: [], calls: [] };
  computeMetrics(D);
  return D;
}

const before = graph(
  [['a/x.mjs', 'a'], ['a/y.mjs', 'a'], ['b/p.mjs', 'b'], ['b/q.mjs', 'b'], ['c/z.mjs', 'c']],
  [['a/x.mjs', 'a/y.mjs'], ['a/y.mjs', 'b/p.mjs'], ['b/p.mjs', 'b/q.mjs']]
);
// x.mjs became x.ts, q now imports p back (a cycle), z newly depends on y (package c → a), and w is new.
const after = graph(
  [['a/x.ts', 'a'], ['a/y.mjs', 'a'], ['b/p.mjs', 'b'], ['b/q.mjs', 'b'], ['c/z.mjs', 'c'], ['c/w.mjs', 'c']],
  [['a/x.ts', 'a/y.mjs'], ['a/y.mjs', 'b/p.mjs'], ['b/p.mjs', 'b/q.mjs'], ['b/q.mjs', 'b/p.mjs'], ['c/z.mjs', 'a/y.mjs']]
);

test('a renamed file keeps its edges: only real dependency changes are reported', () => {
  const d = diffGraphs(before, after);
  assert.deepEqual(d.files, { added: ['c/w.mjs'], removed: [], renamed: [{ from: 'a/x.mjs', to: 'a/x.ts', rule: 'extension' }] });
  assert.deepEqual(d.edges.added.map((e) => [e.source, e.target, e.crossChunk]), [['b/q.mjs', 'b/p.mjs', false], ['c/z.mjs', 'a/y.mjs', true]].sort((a, b) => a[0].localeCompare(b[0])));
  assert.deepEqual(d.edges.removed, []);
});

test('a new cycle and a new package link lead the review; reach and coupling follow', () => {
  const d = diffGraphs(before, after);
  assert.deepEqual(d.cycles.map((c) => [c.change, c.after]), [['new', ['b/p.mjs', 'b/q.mjs']]]);
  assert.equal(d.review[0].kind, 'cycle');
  assert.equal(d.review[0].severity, 'high');
  const link = d.packages.links.find((l) => l.change === 'added');
  assert.deepEqual([link.from, link.to, link.edges], ['c', 'a', 1]);
  assert.equal(link.againstStability, link.instability.from < link.instability.to);
  assert.ok(d.review.some((f) => f.kind === 'package-link'));
  assert.deepEqual(d.reach.find((r) => r.file === 'a/y.mjs'), { file: 'a/y.mjs', before: 1, after: 2 });
  assert.equal(d.summary.cyclesNew, 1);
  assert.match(formatDiff(d), /\[high\] new dependency cycle of 2 files: b\/p\.mjs, b\/q\.mjs/);
});

test('a cycle that loses every member is resolved; identical graphs have nothing to review', () => {
  const d = diffGraphs(after, before);
  assert.deepEqual(d.cycles.map((c) => c.change), ['resolved']);
  assert.equal(diffGraphs(after, after).review.length, 0);
  assert.match(formatDiff(diffGraphs(after, after)), /Nothing to review/);
});

test('renames pair only unambiguous files: same path without extension, or the only file with its name', () => {
  assert.deepEqual(renames(['a/x.mjs', 'old/util.ts', 'p/i.ts', 'q/i.ts'], ['a/x.ts', 'new/util.ts', 'r/i.ts']), [
    { from: 'a/x.mjs', to: 'a/x.ts', rule: 'extension' },
    { from: 'old/util.ts', to: 'new/util.ts', rule: 'move' },
  ]);
});
