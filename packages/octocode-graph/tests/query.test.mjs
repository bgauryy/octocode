// The graph query language on a hand-built result: two chunks joined by imports and calls, a cycle, weights that
// make one route stronger, a test-only edge, and names that resolve ambiguously.
import assert from 'node:assert/strict';
import test from 'node:test';
import { computeMetrics } from '../src/runtime/metrics.ts';
import { formatResult, parseArgs, resolveNodes, runQuery, tokenize } from '../src/runtime/query.ts';

const file = (id, chunk) => ({ id, label: id.split('/').pop(), chunk, language: 'javascript' });
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
  assert.deepEqual(parseArgs(['a', 'depth', '2', '--tests']), { args: ['a'], tests: true, imports: false, weak: false, load: null, depth: 2 });
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
  assert.match(back.summary, /the rows show that reverse route .*not a route from b\/util\.mjs/);
  assert.equal(ids(back)[0], 'a/x.mjs');
  const none = runQuery(D, 'path c/util.mjs a/x.mjs');
  assert.deepEqual([none.ok, none.found, none.rows.length], [true, false, 0]);
  assert.equal(runQuery(D, 'path a/x.mjs').ok, false);
});

test('cycle: the strongly connected set, ordered along one ring through the file', () => {
  const r = runQuery(D, 'cycle q.mjs');
  assert.deepEqual(r.rows.map((x) => [x.id, x.order, x.nextFile]), [['b/q.mjs', 0, 'b/p.mjs'], ['b/p.mjs', 1, 'b/q.mjs']]);
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

test('top ranks completely; find reads as search', () => {
  const r = runQuery(D, 'top pagerank 3');
  assert.equal(r.rows.length, 3);
  const values = r.rows.map((x) => x.value);
  assert.deepEqual(values, values.toSorted((a, b) => b - a));
  assert.equal(runQuery(D, 'top inweight all').rows.length, D.files.length);
  assert.equal(runQuery(D, 'top depth').rows[0].id, 'a/x.mjs');
  assert.equal(runQuery(D, 'top nothing').ok, false);
  // `find` is read as `search`, the one text query: every name match leads its ranked rows, then what holds the word.
  const f = runQuery(D, 'find util');
  assert.equal(f.readAs.query, 'search util');
  assert.deepEqual(f.rows.filter((x) => x.match !== 'text').map((x) => x.id).toSorted(), ['b/util.mjs', 'c/util.mjs']);
  assert.deepEqual(f.rows.slice(0, 2).map((x) => x.match), ['exact', 'exact']);
  const s = runQuery(D, 'search start --symbols');
  assert.deepEqual(s.rows.slice(0, 2).map((x) => [x.id, x.symbol, x.loc.line]), [['b/p.mjs', 'b/p.mjs#start', 40], ['c/util.mjs', 'c/util.mjs#start', 50]]);
  assert.equal(s.rows.every((x) => x.symbol), true);
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
  const data = { section: '.', files: ['a.ts', 'b.ts', 'c.ts'].map((id) => ({ id, label: id, chunk: 'x', metrics: {} })), imports: [], symbols: [], calls: [
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

test('minimum hops and structural strength select different witnessed routes on the same projection', () => {
  const data = fixture();
  data.imports.push(imp('a/x.mjs', 'b/p.mjs', 47));
  computeMetrics(data);
  const query = 'path x.mjs p.mjs';
  assert.deepEqual(runQuery(data, query).nodes, ['a/x.mjs', 'a/y.mjs', 'b/p.mjs']);
  const hops = runQuery(data, `${query} --hops`);
  assert.deepEqual(hops.nodes, ['a/x.mjs', 'b/p.mjs']);
  assert.deepEqual(hops.rows[0].evidence.imports, [47]);
  assert.equal(hops.stats.cost, 1);
  assert.equal(hops.stats.objective, 'hops');
  assert.equal(runQuery(data, 'path p.mjs x.mjs --hops').stats.direction, 'reverse');
  assert.equal(runQuery(data, 'path p.mjs p.mjs --hops').stats.hops, 0);
  assert.equal(runQuery(data, 'path x.test.mjs p.mjs --hops').found, false);
  assert.equal(runQuery(data, 'path x.test.mjs p.mjs --tests --hops').stats.hops, 2);
});

test('order groups a cycle and lists its prerequisites first with the crossing witnesses', () => {
  const r = runQuery(D, 'order a/x.mjs --imports');
  assert.equal(r.ok, true);
  assert.deepEqual(r.nodes.toSorted(), ['a/x.mjs', 'a/y.mjs', 'a/z.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  const component = new Map(r.rows.map(row => [row.id, row.component]));
  assert.equal(component.get('b/p.mjs'), component.get('b/q.mjs'));
  for (const crossing of r.crossings) {
    assert.ok(component.get(crossing.source) > component.get(crossing.target));
    assert.ok(crossing.evidence.imports.length);
  }
  assert.equal(r.crossings.length, 5);
});

test('a test-only import cannot turn a production weak call into a strong production import', () => {
  const data = { files: D.files, symbols: [], imports: [imp('a/x.mjs', 'b/p.mjs', 2, { test: true })],
    calls: [call('a/x.mjs#main', 'b/p.mjs#start', { confidence: 'medium' })] };
  computeMetrics(data);
  assert.equal(runQuery(data, 'path x.mjs p.mjs --imports').found, false);
  assert.equal(runQuery(data, 'path x.mjs p.mjs').found, false);
  assert.equal(runQuery(data, 'path x.mjs p.mjs --weak').stats.weight, 0.5);
  assert.equal(runQuery(data, 'path x.mjs p.mjs --imports --tests').stats.weight, 1.5);
});

test('trace follows typed transitions with zero-length matches and keeps alternate witnesses', () => {
  const q = runQuery(D, 'trace a/x.mjs "DEFINES CALLS+"');
  assert.equal(q.ok, true);
  assert.deepEqual(q.rows.map(r => r.endpoint).sort(), ['a/y.mjs#run', 'b/p.mjs#start']);
  assert.deepEqual(q.steps.filter(s => s.relation === 'CALLS').map(s => s.lines), [[11, 12], [21, 22]]);
  const cycle = runQuery(D, 'trace b/p.mjs "(IMPORTS|DEFINES)*"');
  assert.deepEqual(cycle.rows.map(r => r.endpoint).sort(), ['b/p.mjs', 'b/p.mjs#start', 'b/q.mjs', 'b/util.mjs']);
  assert.equal(runQuery(D, 'trace a/x.mjs "IMPORTS!CALLS"').ok, false);
  assert.equal(runQuery(D, 'trace a/x.mjs READS').ok, false);
});

test('bounded typed walks retain only witnesses on an accepting path within the hop budget', () => {
  const files = ['a','b','c','d'].map(id=>({id}));
  const imports = [['a','b'],['a','c'],['c','b'],['b','d'],['b','a']].map(([source,target])=>({source,target}));
  const data = { files, imports, symbols:[{id:'d#f',file:'d',line:1}] };
  const result = runQuery(data,'trace a IMPORTS+ DEFINES --depth 3');
  assert.equal(result.ok,true,result.error);
  assert.deepEqual(result.rows.map(r=>r.endpoint),['d#f']);
  assert.deepEqual(result.steps.map(e=>`${e.source}>${e.target}`).sort(),['a>b','b>d','d>d#f']);
  const zero = runQuery(data,'trace a IMPORTS* --depth 0');
  assert.deepEqual(zero.rows.map(r=>r.endpoint),['a']);
  assert.deepEqual(zero.steps,[]);
  assert.equal(runQuery(data,'trace a IMPORTS --depth 1.5').ok,false);
});

test('symbol lookups and typed walks do not materialize a weighted file projection', () => {
  const d = { files: [{id:'a.ts'},{id:'b.ts'}], calls: [{source:'a.ts#a',target:'b.ts#b',confidence:'high'}],
    get fileEdges() { throw new Error('unrelated weighted projection'); } };
  assert.equal(runQuery(d,'callers b.ts#b').rows.length,1);
  assert.equal(runQuery(d,'trace a.ts#a CALLS').rows.length,1);
});

test('callers --tests counts the callers in test files: the comparison with production callers is a field', () => {
  const d = { files: [{id:'a.ts'},{id:'b.ts'},{id:'t.test.ts',roles:['test']}],
    calls: [{source:'a.ts#a',target:'b.ts#b',confidence:'high'},{source:'t.test.ts#check',target:'b.ts#b',confidence:'high',test:true},{source:'t.test.ts#check2',target:'b.ts#b',confidence:'high',test:true}] };
  const production = runQuery(d,'callers b.ts#b');
  assert.equal(production.stats.testCalls, undefined, 'without --tests test callers are not read, so not counted');
  const all = runQuery(d,'callers b.ts#b --tests');
  assert.deepEqual([all.stats.calls, all.stats.testCalls, all.stats.testFiles], [3, 2, 1]);
  assert.equal(all.stats.calls - all.stats.testCalls, production.stats.calls);
  assert.match(all.summary, /2 callers in test files/);
  assert.equal(runQuery(d,'callers a.ts#a --tests').stats?.testCalls, 0);
  assert.equal(runQuery(d,'callees t.test.ts#check --tests').stats.testCalls, undefined, 'callees state no test count');
});

test('cycle weight and identity describe the selected component', () => {
  const data = { files: ['a','b','c'].map(id => file(id,'root')), symbols: [],
    imports: [imp('a','b',1), imp('b','a',2)],
    calls: [call('b#f','c#g'), call('c#g','b#f')] };
  computeMetrics(data);
  const result = runQuery(data, 'cycle a --imports');
  assert.equal(result.stats.size, 2);
  assert.equal(result.stats.weight, 2);
  assert.equal(result.stats.cycle, null);
});

test('file-set traversal gives the shortest depth from any seed and validates every seed', () => {
  for (const op of ['impact', 'deps', 'dependents']) {
    const seeds = ['b/util.mjs', 'a/y.mjs'];
    const parts = seeds.map(id => runQuery(D, `${op} ${id} depth 2`));
    const expected = new Map();
    for (const row of parts.flatMap(r => r.rows)) {
      if (!seeds.includes(row.id)) expected.set(row.id, Math.min(expected.get(row.id) ?? Infinity, row.depth));
    }
    const grouped = runQuery(D, `${op} ${seeds.join(' ')} y.mjs depth 2`);
    assert.equal(grouped.ok, true);
    assert.deepEqual(new Map(grouped.rows.map(r => [r.id,r.depth])), expected);
    assert.equal(new Set(grouped.nodes).size, grouped.nodes.length);
    assert.equal(runQuery(D, `${op} y.mjs unknown.mjs`).ok, false);
  }
});

test('files returns saved metadata and requested source lines without traversing edges', () => {
  const data = { section:'/repo', files:fixture().files, get calls() { throw Error('unneeded calls'); } };
  const r = runQuery(data, 'files /repo/a/x.mjs:11 b/p.mjs x.mjs:19');
  assert.equal(r.ok,true);
  assert.deepEqual(r.rows.map(f=>[f.id,f.loc.line,f.language]), [['a/x.mjs',11,'javascript'],['b/p.mjs',1,'javascript'],['a/x.mjs',19,'javascript']]);
  assert.deepEqual(r.nodes,['a/x.mjs','b/p.mjs']);
  // Source reads are absolute (the graph's root + path), so localFetch resolves them from any working directory.
  assert.deepEqual(r.sourceQueries,[{path:'/repo/a/x.mjs',ranges:[{startLine:11,endLine:11},{startLine:19,endLine:19}]},{path:'/repo/b/p.mjs'}]);
  assert.deepEqual(runQuery(data,'files x.mjs:19 x.mjs x.mjs:11').sourceQueries,[{path:'/repo/a/x.mjs'}]);
  assert.equal(runQuery(data,'files util.mjs').candidates.length,2);
});

test('candidate retries keep every quoted path and the selected source location', () => {
  const data = {files:['a/weird "dir"/unit.js','b/unit.js','src/my file.js','a/literal:42','a/#file.js'].map(id=>file(id,'a'))};
  const r = runQuery(data, 'files "my file.js" unit.js:12:4');
  assert.equal(r.candidates.length,2);
  for (const choice of r.candidates) {
    const selected = runQuery(data,choice.query);
    assert.deepEqual(selected.rows.map(f=>f.id), ['src/my file.js',choice.id]);
    assert.equal(selected.rows[1].loc.line,12);
    assert.equal(selected.rows[1].loc.column,4);
  }
  assert.deepEqual(runQuery(data,'files literal:42').sourceQueries,[{path:'a/literal:42'}]);
  assert.equal(runQuery(data,'files #file.js:42').rows[0].loc.line,42);
  assert.equal(runQuery(data,'files b/unit.js:0').ok,false);
});

test('offline stdin returns one result per question and continues after a query error', async () => {
  const {spawnSync} = await import('node:child_process');
  const {writeFileSync} = await import('node:fs');
  const {join} = await import('node:path');
  const {fileURLToPath} = await import('node:url');
  const {tempDir} = await import('./test-tmp.mjs');
  const out = tempDir('graph-query-stdin-');
  writeFileSync(join(out,'data.json'), JSON.stringify(D));
  const cli = fileURLToPath(new URL('../src/runtime/commands/query.ts',import.meta.url));
  const q = ['files x.mjs','unknown','impact x.mjs y.mjs'];
  const proc = spawnSync(process.execPath,[cli,out,'--stdin','--json'],{input:q.join('\r\n\r\n'),encoding:'utf8'});
  assert.equal(proc.status,1,proc.stderr);
  // The CLI answers exactly as runQuery, with the stale-file warning every surface adds (withStaleness).
  const { editedSince, withStaleness } = await import('../src/runtime/query.ts');
  const edited = editedSince(D, join(out, 'data.json'));
  assert.deepEqual(proc.stdout.trim().split('\n').map(s=>JSON.parse(s)),q.map(s=>withStaleness(runQuery(D,s),edited)));
});

test('search ranks names by how they match, then by call sites received; every match is reachable', () => {
  const r = runQuery(D, 'search start');
  assert.equal(r.ok, true);
  // Both `start` declarations match exactly; the one with production calls ranks first.
  assert.deepEqual(r.rows.slice(0, 2).map((x) => [x.symbol, x.match, x.rank]), [['b/p.mjs#start', 'exact', 1], ['c/util.mjs#start', 'exact', 2]]);
  assert.equal(r.rows[0].callSites, 3);
  assert.equal(r.stats.matches, r.rows.length);
  // Separators and case do not matter; initials and word prefixes match a camelCase name.
  const named = { ...D, symbols: [...D.symbols, sym('a/y.mjs#parseArgs', 25), sym('a/z.mjs#getUserName', 35)] };
  for (const text of ['parse_args', 'PARSEARGS', 'parse-args']) assert.equal(runQuery(named, `search ${text}`).rows[0].match, 'exact', text);
  // A name match ranks before every row that only holds a query word (in its path, or a name it calls).
  const words = runQuery(named, 'search p args');
  assert.deepEqual([words.rows[0].name, words.rows[0].match], ['parseArgs', 'words']);
  assert.equal(words.rows.slice(1).every((x) => x.match === 'text'), true);
  assert.deepEqual(runQuery(named, 'search gun').rows.map((x) => [x.name, x.match]), [['getUserName', 'words']]);
  assert.deepEqual(runQuery(named, 'search args parse').rows.map((x) => x.match), ['terms']);
  // Letters in order only when nothing holds the text or any of its words.
  const fuzzy = runQuery(named, 'search prsrgs');
  assert.deepEqual([fuzzy.rows[0].name, fuzzy.rows[0].match, fuzzy.stats.fuzzy], ['parseArgs', 'fuzzy', true]);
  assert.equal(runQuery(named, 'search prsArgs').stats.fuzzy, false, 'a query word a name holds is a text match');
  assert.equal(runQuery(D, 'search util --files').rows.every((x) => x.kind === 'file'), true);
  // No symbol is named util; one is found by its file's path, as a text match.
  assert.deepEqual(runQuery(D, 'search util --symbols').rows.map((x) => [x.symbol, x.match]), [['c/util.mjs#start', 'text']]);
  assert.equal(runQuery(D, 'search start --bogus').ok, false);
  assert.match(runQuery(D, 'search zzzz').summary, /nothing matches/);
});

test('search shows the best rows and names the command that lists every match', () => {
  const many = { ...D, symbols: Array.from({ length: 25 }, (_, i) => sym(`a/x.mjs#handler${i}`, i + 1)) };
  const r = runQuery(many, 'search handler');
  assert.equal(r.rows.length, 20);
  assert.equal(r.stats.matches, 25);
  assert.match(r.summary, /`search handler --all` lists all 25/);
  assert.equal(runQuery(many, 'search handler --all').rows.length, 25);
});

test('about a file: its metrics and community, direct dependencies and dependents, and its declarations', () => {
  const r = runQuery(D, 'about a/y.mjs');
  assert.equal(r.ok, true);
  assert.deepEqual(r.rows.map((x) => [x.role, x.id]), [['subject', 'a/y.mjs'], ['dependency', 'b/p.mjs'], ['dependent', 'a/x.mjs'], ['declares', 'a/y.mjs']]);
  assert.equal(r.rows[0].metrics.community, D.files.find((f) => f.id === 'a/y.mjs').metrics.community);
  assert.deepEqual(r.stats, { dependencies: 1, dependents: 1, declarations: 1, weakDependencies: 0, weakDependents: 0 });
  assert.equal(runQuery(D, 'about y.mjs:20').rows[0].loc.line, 20);
});

test('about a symbol: callers and callees; an ambiguous name lists re-runnable candidates', () => {
  const r = runQuery(D, 'about a/y.mjs#run');
  assert.deepEqual(r.rows.map((x) => [x.role, x.symbol]), [['subject', 'a/y.mjs#run'], ['caller', 'a/x.mjs#main'], ['callee', 'b/p.mjs#start']]);
  const bare = runQuery(D, 'about run');
  assert.equal(bare.rows[0].symbol, 'a/y.mjs#run');
  const ambiguous = runQuery(D, 'about start');
  assert.equal(ambiguous.ok, false);
  assert.deepEqual(ambiguous.candidates.map((c) => c.query), ['about b/p.mjs#start', 'about c/util.mjs#start']);
  assert.match(runQuery(D, 'about nothing-here').error, /try search nothing-here/);
});

test('critical functions: PageRank over the call graph weighted by call sites and evidence, with callers and reach', () => {
  const r = runQuery(D, 'critical functions');
  assert.equal(r.ok, true, r.error);
  const top = r.rows[0];
  assert.deepEqual([top.symbol, top.callSites, top.callers, top.callerFiles, top.transitiveCallers], ['b/p.mjs#start', 3, 2, 2, 3]);
  assert.deepEqual(r.rows.map((x) => x.rank), r.rows.map((_, i) => i + 1));
  assert.equal(runQuery(D, 'critical functions 1').rows.length, 1);
  assert.match(runQuery(D, 'critical functions 1').summary, /`critical functions all` ranks all 4/);
});

test('critical points: files whose removal disconnects the file graph, with the parts left', () => {
  const r = runQuery(D, 'critical points');
  assert.deepEqual(r.rows.map((x) => [x.id, x.value, x.parts]), [['b/p.mjs', 2, [3, 2]], ['b/q.mjs', 1, [4, 1]]]);
});

test('critical path: the longest dependency chain, through a cycle, and the chain of dependents above a file', () => {
  const down = runQuery(D, 'critical path');
  assert.deepEqual(down.nodes, ['a/x.mjs', 'a/y.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  assert.deepEqual([down.stats.depth, down.stats.hops], [3, 4]);
  // Each row carries the hop's weight and evidence like `path`: x → y is the heavier of x's two edges.
  assert.equal(down.rows[0].nextFile, 'a/y.mjs');
  assert.ok(down.rows[0].weight > 1);
  const up = runQuery(D, 'critical path b/util.mjs --up');
  assert.deepEqual(up.nodes, ['a/x.mjs', 'a/y.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  assert.equal(runQuery(D, 'critical path b/util.mjs').stats.hops, 0);
  assert.equal(runQuery(D, 'critical nonsense').ok, false);
});

test('callers state what they cannot see: other chunks need LSP, Rust macros need the language server', () => {
  // One chunk, no Rust: the answer is the graph's own, with nothing to disclose.
  assert.equal(runQuery(D, 'callers b/p.mjs#start').warnings, undefined);
  const multi = { ...D, chunks: [{ id: 'a', path: 'a', built: true }, { id: 'b', path: 'b', built: true }], callStats: { mode: 'both', lsp: 'ok', scope: 'functions in hub files' } };
  const r = runQuery(multi, 'callers b/p.mjs#start');
  assert.equal(r.ok, true);
  assert.match(r.warnings[0], /other chunks .*\(functions in hub files\)/);
  assert.deepEqual(r.next.confirm, { tool: 'lspSearch', input: { queries: [{ operation: 'callers', path: '/repo/b/p.mjs', symbolName: 'start', line: 40 }] } });
  assert.match(r.summary, /may be incomplete/);
  assert.match(formatResult(r), /^confirm: octocode lspSearch/m);
  const rust = { ...D, files: [...D.files, file('r/lib.rs', 'r')], symbols: [...D.symbols, sym('r/lib.rs#go', 3)], calls: [...D.calls, call('a/x.mjs#main', 'r/lib.rs#go')] };
  assert.match(runQuery(rust, 'callers r/lib.rs#go').warnings.join(' '), /Rust macro/);
  assert.equal(runQuery(multi, 'about b/p.mjs#start').next.confirm.tool, 'lspSearch');
});

test('region: what can take part in entry → outcome, and the steps every modeled route must pass', () => {
  const r = runQuery(D, 'region a/x.mjs b/util.mjs');
  assert.equal(r.ok, true, r.error);
  assert.deepEqual(r.rows.map((x) => x.id).toSorted(), ['a/x.mjs', 'a/y.mjs', 'a/z.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  assert.deepEqual(r.stats.mustPass, ['b/q.mjs', 'b/p.mjs']);
  assert.deepEqual(r.rows.filter((x) => x.mustPass).map((x) => x.id).toSorted(), ['a/x.mjs', 'b/p.mjs', 'b/q.mjs', 'b/util.mjs']);
  // Two symbols ask the call graph.
  const fn = runQuery(D, 'region a/x.mjs#main b/p.mjs#start');
  assert.deepEqual([fn.stats.level, fn.stats.mustPass], ['function', ['a/y.mjs#run']]);
  assert.match(runQuery(D, 'region b/util.mjs a/x.mjs').summary, /no modeled route b\/util\.mjs → a\/x\.mjs; a\/x\.mjs reaches b\/util\.mjs instead/);
});

test('path --avoid: a route that bypasses the avoided files breaks a rule that requires them', () => {
  const bypass = runQuery(D, 'path a/x.mjs b/p.mjs --avoid a/y.mjs');
  assert.deepEqual([bypass.found, bypass.nodes], [true, ['a/x.mjs', 'a/z.mjs', 'b/p.mjs']]);
  const none = runQuery(D, 'path a/x.mjs b/util.mjs --avoid b/q.mjs');
  assert.equal(none.found, false);
  assert.match(none.summary, /every route .* passes through the avoided 1 file/);
  // A directory avoids every file under it; avoiding an endpoint is refused.
  assert.equal(runQuery(D, 'path a/x.mjs b/p.mjs --avoid a/').ok, false);
  assert.equal(runQuery(D, 'path a/x.mjs b/p.mjs --avoid a/y.mjs a/z.mjs').found, false);
});

test('answers hand over the next Octocode call: read the best hit or the declaration, confirm callers', () => {
  const about = runQuery(D, 'about a/y.mjs#run');
  assert.deepEqual(about.next.read, { tool: 'localFetch', input: { queries: [{ path: '/repo/a/y.mjs', ranges: [{ startLine: 20, endLine: 20 }] }] } });
  assert.deepEqual(runQuery(D, 'about a/y.mjs').next.read.input.queries, [{ path: '/repo/a/y.mjs' }]);
  assert.equal(runQuery(D, 'search start').next.read.input.queries[0].path, '/repo/b/p.mjs');
  assert.match(formatResult(about), /^read: octocode localFetch/m);
});

test('cycle with no file lists every measured cycle, largest first, with all its members', () => {
  const r = runQuery(D, 'cycle');
  assert.equal(r.ok, true, r.error);
  assert.equal(r.rows.length, D.metrics.cycles.length);
  assert.deepEqual(r.rows[0].members.toSorted(), ['b/p.mjs', 'b/q.mjs']);
  assert.match(formatResult(r), /b\/p\.mjs.*b\/q\.mjs|b\/q\.mjs.*b\/p\.mjs/);
  assert.match(r.summary, /`cycle <file>`/);
});

test('a callers answer is partial unless every callable was probed and every probe answered', () => {
  const open = runQuery(fixture(), 'callers b/p.mjs#start');
  assert.equal(open.isPartial, true);
  assert.deepEqual(open.partialReasons, ['callersNotProbed']);
  assert.ok(open.next?.confirm, 'a partial answer hands over the confirming lspSearch call');
  assert.match(open.summary, /may be incomplete \(callersNotProbed\)/);
  const all = fixture();
  all.callStats = { scope: 'all callable declarations', lsp: 'ok' };
  const complete = runQuery(all, 'callers b/p.mjs#start');
  assert.equal(complete.isPartial, undefined);
  assert.equal(complete.warnings, undefined);
  all.callStats.lsp = 'partial';
  assert.equal(runQuery(all, 'callers b/p.mjs#start').isPartial, true, 'an unanswered probe leaves callers unproven');
});
