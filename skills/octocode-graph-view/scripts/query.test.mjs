// The graph query language on a hand-built result: two chunks joined by imports and calls, a cycle, weights that
// make one route stronger, a test-only edge, and names that resolve ambiguously.
import assert from 'node:assert/strict';
import test from 'node:test';
import { computeMetrics } from './metrics.mjs';
import { formatResult, parseArgs, resolveNodes, runQuery, tokenize } from './query.mjs';

const file = (id, chunk) => ({ id, label: id.split('/').pop(), chunk, language: 'javascript', loc: { path: id, line: 1 } });
const imp = (source, target, line, extra = {}) => ({ source, target, line, lines: [line], confidence: 'high', test: false, ...extra });
const call = (source, target, extra = {}) => ({ source, target, line: 5, lines: [5], count: 1, confidence: 'high', via: 'graph', test: false, ...extra });
const sym = (id, line) => ({ id, file: id.split('#')[0], name: id.split('#')[1], kind: 'function', line });

// a/x → a/y → b/p is the strong route (weights 3, 3); a/x → a/z → b/p the weak one (1, 1.5).
// b/p ⇄ b/q is a cycle; b/q → b/util; c/util.mjs is a second util.mjs; a/x.test.mjs → a/x is test-only.
function fixture() {
  const D = {
    section: '/repo',
    files: [file('a/x.mjs', 'a'), file('a/y.mjs', 'a'), file('a/z.mjs', 'a'), file('a/x.test.mjs', 'a'), file('b/p.mjs', 'b'), file('b/q.mjs', 'b'), file('b/util.mjs', 'b'), file('c/util.mjs', 'c')],
    imports: [
      imp('a/x.mjs', 'a/y.mjs', 2), imp('a/x.mjs', 'a/z.mjs', 3), imp('a/y.mjs', 'b/p.mjs', 4), imp('a/z.mjs', 'b/p.mjs', 6),
      imp('b/p.mjs', 'b/q.mjs', 1), imp('b/q.mjs', 'b/p.mjs', 2), imp('b/q.mjs', 'b/util.mjs', 3), imp('a/x.test.mjs', 'a/x.mjs', 1, { test: true }),
    ],
    symbols: [sym('a/x.mjs#main', 10), sym('a/y.mjs#run', 20), sym('a/z.mjs#go', 30), sym('b/p.mjs#start', 40), sym('c/util.mjs#start', 50)],
    calls: [
      call('a/x.mjs#main', 'a/y.mjs#run', { count: 2, lines: [11, 12] }),
      call('a/y.mjs#run', 'b/p.mjs#start', { count: 2, lines: [21, 22], via: 'both' }),
      call('a/z.mjs#go', 'b/p.mjs#start', { via: 'stitched', confidence: 'medium', lines: [31] }),
    ],
  };
  computeMetrics(D);
  return D;
}

const D = fixture();
const ids = (r) => r.rows.map((x) => x.id);

test('tokens, depth and --tests parse anywhere after the op', () => {
  assert.deepEqual(tokenize('find "two words" x'), ['find', 'two words', 'x']);
  assert.deepEqual(parseArgs(['a', 'depth', '2', '--tests']), { args: ['a'], tests: true, imports: false, weak: false, depth: 2 });
  assert.equal(parseArgs(['a', '--weak']).weak, true);
  assert.equal(parseArgs(['--depth=all']).depth, Infinity);
  assert.equal(runQuery(D, 'impact b/util.mjs depth 0').ok, false);
});

test('file arguments: path, root-prefixed, ./, path:line, unique suffix; ambiguity lists candidates as re-runnable queries', () => {
  for (const arg of ['a/x.mjs', '/repo/a/x.mjs', './a/x.mjs', 'a/x.mjs:12', 'x.mjs', 'a/x.mjs#main']) assert.equal(runQuery(D, `deps ${arg}`).nodes[0], 'a/x.mjs', arg);
  const r = runQuery(D, 'impact util.mjs');
  assert.equal(r.ok, false);
  assert.match(r.error, /ambiguous/);
  assert.deepEqual(r.candidates, [{ id: 'b/util.mjs', query: 'impact b/util.mjs' }, { id: 'c/util.mjs', query: 'impact c/util.mjs' }]);
  const none = runQuery(D, 'deps nope.zz');
  assert.equal(none.ok, false);
  assert.equal(none.candidates, undefined);
  assert.equal(runQuery(D, 'bogus x').ok, false);
  assert.deepEqual(resolveNodes(D, ['p.mjs', 'b/p.mjs', 'util.mjs', 'nope']).ids, ['b/p.mjs']);
  assert.deepEqual(resolveNodes(D, ['util.mjs', 'nope']).problems.map((p) => p.arg), ['util.mjs', 'nope']);
});

test('impact: every transitive dependent with its depth, the weight that reached it, counts per depth and chunk', () => {
  const r = runQuery(D, 'impact b/util.mjs');
  assert.equal(r.ok, true);
  assert.deepEqual(r.rows.map((x) => [x.id, x.depth]), [['b/q.mjs', 1], ['b/p.mjs', 2], ['a/y.mjs', 3], ['a/z.mjs', 3], ['a/x.mjs', 4]]);
  assert.deepEqual(r.stats.byDepth, { 1: 1, 2: 1, 3: 2, 4: 1 });
  assert.deepEqual(r.stats.byChunk, { b: 2, a: 3 });
  const y = r.rows.find((x) => x.id === 'a/y.mjs');
  assert.deepEqual([y.weight, y.through, y.loc], [3, 'b/p.mjs', { path: 'a/y.mjs', line: 4 }]);
  // a/x reaches depth 3 through y (weight 3), not z (1).
  assert.deepEqual([r.rows.at(-1).through, r.rows.at(-1).weight], ['a/y.mjs', 3]);
  assert.deepEqual(r.nodes, ['b/util.mjs', ...ids(r)]);
  assert.ok(r.edges.some(([u, v]) => u === 'a/y.mjs' && v === 'b/p.mjs'));
  assert.deepEqual(ids(runQuery(D, 'impact b/util.mjs depth 2')), ['b/q.mjs', 'b/p.mjs']);
  assert.equal(runQuery(D, 'impact b/util.mjs --tests').rows.at(-1).id, 'a/x.test.mjs', 'test-only edges only with --tests');
  assert.match(r.summary, /5 files depend on b\/util\.mjs transitively/);

});

test('--imports walks import edges only: a call bound by name alone drops out', () => {
  const E = fixture();
  E.calls.push(call('c/util.mjs#start', 'b/util.mjs#helper'));
  delete E.fileEdges;
  computeMetrics(E);
  assert.deepEqual(ids(runQuery(E, 'dependents b/util.mjs')).toSorted(), ['b/q.mjs', 'c/util.mjs']);
  const r = runQuery(E, 'dependents b/util.mjs --imports');
  assert.deepEqual([ids(r), r.imports], [['b/q.mjs'], true]);
});

test('deps and dependents: depth 1 by default, deeper on request', () => {
  assert.deepEqual(ids(runQuery(D, 'deps a/x.mjs')), ['a/y.mjs', 'a/z.mjs']);
  assert.deepEqual(ids(runQuery(D, 'deps a/x.mjs depth all')).toSorted(), ['a/y.mjs', 'a/z.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  assert.deepEqual(ids(runQuery(D, 'dependents p.mjs')).toSorted(), ['a/y.mjs', 'a/z.mjs', 'b/q.mjs']);
  assert.deepEqual(ids(runQuery(D, 'dependents a/x.mjs')), []);
  assert.deepEqual(ids(runQuery(D, 'dependents a/x.mjs --tests')), ['a/x.test.mjs']);
});

test('path: the heavier route wins, with hop weights and evidence; no path reports the reverse direction', () => {
  const r = runQuery(D, 'path x.mjs b/util.mjs');
  assert.equal(r.found, true);
  assert.deepEqual(ids(r), ['a/x.mjs', 'a/y.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  assert.deepEqual(r.rows.map((x) => x.weight), [3, 3, 1, 1, undefined]);
  const hop = r.rows[1];
  assert.deepEqual(hop.evidence.imports, [4]);
  assert.deepEqual(hop.evidence.calls.map((c) => [c.source, c.target, c.via]), [['a/y.mjs#run', 'b/p.mjs#start', 'both']]);
  assert.deepEqual(r.edges, [['a/x.mjs', 'a/y.mjs'], ['a/y.mjs', 'b/p.mjs'], ['b/p.mjs', 'b/q.mjs'], ['b/q.mjs', 'b/util.mjs']]);
  assert.equal(r.stats.hops, 4);
  const back = runQuery(D, 'path b/util.mjs a/x.mjs');
  assert.equal(back.found, false);
  assert.equal(back.stats.direction, 'reverse');
  assert.match(back.summary, /no path b\/util\.mjs → a\/x\.mjs; the reverse/);
  assert.equal(ids(back)[0], 'a/x.mjs');
  const none = runQuery(D, 'path c/util.mjs a/x.mjs');
  assert.deepEqual([none.ok, none.found, none.rows.length], [true, false, 0]);
  assert.equal(runQuery(D, 'path a/x.mjs').ok, false);
});

test('cycle: the strongly connected set, ordered along one ring through the file', () => {
  const r = runQuery(D, 'cycle q.mjs');
  assert.deepEqual(r.rows.map((x) => [x.id, x.order, x.next]), [['b/q.mjs', 0, 'b/p.mjs'], ['b/p.mjs', 1, 'b/q.mjs']]);
  assert.deepEqual(r.edges, [['b/q.mjs', 'b/p.mjs'], ['b/p.mjs', 'b/q.mjs']]);
  assert.equal(r.stats.size, 2);
  assert.equal(r.stats.cycle, D.metrics.cycles[0].id);
  assert.equal(r.rows[0].loc.line, 2, 'the ring row points at the import line of its hop');
  const none = runQuery(D, 'cycle a/x.mjs');
  assert.deepEqual([none.ok, none.rows.length], [true, 0]);
  assert.match(none.summary, /no dependency cycle/);
});

test('community: members by PageRank and links to other communities, by file or id', () => {
  const id = D.files.find((f) => f.id === 'b/p.mjs').metrics.community;
  const r = runQuery(D, 'community b/p.mjs');
  assert.equal(r.ok, true);
  assert.equal(r.stats.id, id);
  assert.ok(r.rows.some((x) => x.id === 'b/p.mjs'));
  const ranks = r.rows.map((x) => x.pagerank);
  assert.deepEqual(ranks, ranks.toSorted((a, b) => b - a));
  assert.deepEqual(runQuery(D, `community ${id}`).rows, r.rows);
  for (const l of r.links) assert.notEqual(l.community, id);
  assert.equal(runQuery(D, 'community 99').ok, false);
});

test('callers and callees: symbol-level with provenance; a bare name shared by two files is ambiguous', () => {
  const amb = runQuery(D, 'callers start');
  assert.equal(amb.ok, false);
  assert.deepEqual(amb.candidates.map((c) => c.query), ['callers b/p.mjs#start', 'callers c/util.mjs#start']);
  const r = runQuery(D, 'callers p.mjs#start');
  assert.deepEqual(r.rows.map((x) => [x.symbol, x.via, x.confidence, x.loc.line]), [['a/y.mjs#run', 'both', 'high', 21], ['a/z.mjs#go', 'stitched', 'medium', 31]]);
  assert.deepEqual(r.nodes, ['b/p.mjs', 'a/y.mjs', 'a/z.mjs']);
  assert.deepEqual(r.stats.via, { both: 1, stitched: 1 });
  const out = runQuery(D, 'callees main');
  assert.deepEqual(out.rows.map((x) => [x.id, x.name, x.count, x.loc.line]), [['a/y.mjs', 'run', 2, 20]]);
  assert.deepEqual(out.edges, [['a/x.mjs', 'a/y.mjs']]);
});

test('top and find: complete rankings and matches', () => {
  const r = runQuery(D, 'top pagerank 3');
  assert.equal(r.rows.length, 3);
  const values = r.rows.map((x) => x.value);
  assert.deepEqual(values, values.toSorted((a, b) => b - a));
  assert.equal(runQuery(D, 'top inweight all').rows.length, D.files.length);
  assert.equal(runQuery(D, 'top depth').rows[0].id, 'a/x.mjs');
  assert.equal(runQuery(D, 'top nothing').ok, false);
  const f = runQuery(D, 'find util');
  assert.deepEqual(ids(f), ['b/util.mjs', 'c/util.mjs']);
  const s = runQuery(D, 'find start');
  assert.deepEqual(s.rows.map((x) => [x.id, x.symbol, x.loc.line]), [['b/p.mjs', 'b/p.mjs#start', 40], ['c/util.mjs', 'c/util.mjs#start', 50]]);
});

test('formatResult lists every row with its location, and candidates to re-run', () => {
  const text = formatResult(runQuery(D, 'impact b/util.mjs'));
  assert.equal(text.split('\n').length, 6);
  assert.match(text, /a\/y\.mjs:4 {2}d3 {2}w 3 {2}via b\/p\.mjs/);
  assert.match(formatResult(runQuery(D, 'impact util.mjs')), /candidates \(re-run one\):\n {2}impact b\/util\.mjs/);
});

test('walks follow strong edges by default; --weak adds edges backed only by name-bound medium/low calls', () => {
  const files = ['a.ts', 'b.ts', 'c.ts'].map((id) => ({ id, chunk: 'x' }));
  const data = { files, imports: [{ source: 'b.ts', target: 'a.ts' }], calls: [{ source: 'c.ts#f', target: 'a.ts#g', count: 3, confidence: 'medium', via: 'graph' }], symbols: [], declarations: [] };
  const rowIds = (q) => runQuery(data, q).rows.map((r) => r.id).toSorted((x, y) => x.localeCompare(y));
  assert.deepEqual(rowIds('impact a.ts'), ['b.ts']);
  assert.deepEqual(rowIds('impact a.ts --weak'), ['b.ts', 'c.ts']);
});

test('a strong-edge answer states what --weak would add, so it never contradicts the metrics', () => {
  const data = { section: '.', files: ['a.ts', 'b.ts', 'c.ts'].map((id) => ({ id, label: id, chunk: 'x', loc: { path: id, line: 1 }, metrics: {} })), imports: [], symbols: [], calls: [
    { source: 'a.ts#f', target: 'b.ts#g', via: 'graph', confidence: 'medium', line: 2 },
    { source: 'b.ts#g', target: 'a.ts#f', via: 'graph', confidence: 'medium', line: 3 },
  ] };
  const impactNone = runQuery(data, 'impact b.ts');
  assert.deepEqual(impactNone.rows, []);
  assert.equal(impactNone.stats.weakAdds, 1);
  assert.match(impactNone.summary, /nothing depends on b\.ts; --weak adds 1 file/);
  assert.match(runQuery(data, 'dependents b.ts').summary, /0 files depend on b\.ts within depth 1; --weak adds 1 file/);
  assert.match(runQuery(data, 'cycle b.ts').summary, /no dependency cycle; --weak finds it in a cycle of 2 files/);
  assert.match(runQuery(data, 'path a.ts b.ts').summary, /--weak finds one of 1 hop/);
  assert.doesNotMatch(runQuery(data, 'impact b.ts --weak').summary, /--weak adds/);
  assert.doesNotMatch(runQuery(data, 'impact b.ts --imports').summary, /--weak adds/);
  assert.doesNotMatch(runQuery(data, 'impact c.ts').summary, /--weak adds/);
});
