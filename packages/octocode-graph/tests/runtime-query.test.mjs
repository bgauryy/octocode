// Walks that ask what runs: import kinds decide whether an edge loads (loads.ts), `--runtime` and `--eager` filter on
// them, `reach` answers what an entry loads with bytes, packages, lazy boundaries and forbid rules, `hubs` ranks files
// by what they cost importers, `path --all` lists every first hop, and `--count`/`--group` fold rows.
import assert from 'node:assert/strict';
import test from 'node:test';
import { computeMetrics } from '../src/runtime/metrics.ts';
import { formatResult, runQuery } from '../src/runtime/query.ts';
import { loadOf } from '../src/loads.ts';

const BYTES = {
  'src/entry.ts': 100,
  'src/a.ts': 200,
  'src/big.ts': 5000,
  'src/types.ts': 50,
  'src/model.ts': 7000,
  'src/lazy.ts': 300,
  'src/heavy.ts': 9000,
  'src/util.ts': 400,
  'src/leaf.ts': 10,
  'src/fence.ts': 20,
  'src/b.ts': 30,
  'src/x.ts': 1,
  'src/y.ts': 1,
};
const file = (id) => ({ id, label: id.split('/').pop(), chunk: 'app', language: 'typescript', check: { status: 'ok', bytes: BYTES[id] } });
const imp = (source, target, line, kinds) => ({ source, target, line, lines: [line], confidence: 'high', resolution: kinds, test: false });

// entry → a → big (static); entry ⇢ types (type-only) → model; entry ⇢ lazy (dynamic) → heavy → pkg:ai;
// entry → fence → util (re-exports leaf's clipText); entry → b → util (own name); x ⇄ y only through a type import.
function fixture() {
  const D = {
    section: '/repo',
    chunks: [{ id: 'app', path: '.', files: 13, package: '@x/app', built: true }],
    files: Object.keys(BYTES).map(file),
    imports: [
      imp('src/entry.ts', 'src/a.ts', 1, ['static-import']),
      imp('src/entry.ts', 'src/types.ts', 2, ['type-import']),
      imp('src/entry.ts', 'src/lazy.ts', 3, ['dynamic-import']),
      imp('src/entry.ts', 'src/fence.ts', 4, ['static-import']),
      imp('src/entry.ts', 'src/b.ts', 5, ['static-import']),
      imp('src/a.ts', 'src/big.ts', 1, ['static-import']),
      imp('src/types.ts', 'src/model.ts', 1, ['static-import']),
      imp('src/lazy.ts', 'src/heavy.ts', 1, ['static-import']),
      imp('src/fence.ts', 'src/util.ts', 1, ['static-import']),
      imp('src/b.ts', 'src/util.ts', 1, ['static-import']),
      imp('src/util.ts', 'src/leaf.ts', 1, ['named-reexport']),
      imp('src/x.ts', 'src/y.ts', 1, ['static-import']),
      imp('src/y.ts', 'src/x.ts', 1, ['type-import']),
    ],
    symbols: [],
    calls: [],
    uses: [
      { source: 'src/fence.ts', target: 'src/leaf.ts#clipText', line: 1, detail: 'reexport' },
      { source: 'src/b.ts', target: 'src/util.ts#own', line: 1, detail: 'import' },
      { source: 'src/util.ts', target: 'src/leaf.ts#clipText', line: 1, detail: 'reexported-name' },
    ],
    externals: [
      { id: 'pkg:ai', count: 1, files: ['src/heavy.ts'], kinds: { 'src/heavy.ts': ['external', 'static-import'] } },
      { id: 'pkg:zod', count: 1, files: ['src/a.ts'], kinds: { 'src/a.ts': ['external', 'type-import'] } },
      { id: 'pkg:react', count: 1, files: ['src/a.ts'], kinds: { 'src/a.ts': ['external'] } },
    ],
  };
  computeMetrics(D);
  return D;
}

const D = fixture();
const ids = (r) => r.rows.map((x) => x.id).toSorted();

test('an edge loads as its strongest import kind; how it was stitched or declared settles nothing', () => {
  assert.equal(loadOf(['static-import', 'type-import']), 'eager');
  assert.equal(loadOf(['external', 'type-import']), 'type');
  assert.equal(loadOf(['type-named-reexport']), 'type');
  assert.equal(loadOf(['dynamic-import', 'type-import']), 'deferred');
  assert.equal(loadOf(['crate-path']), 'compile');
  for (const none of [[], ['relative'], ['package'], ['external-unknown']]) assert.equal(loadOf(none), 'unknown', none.join());
});

test('path shows each hop kind; --runtime refuses a route that crosses a type-only import', () => {
  const r = runQuery(D, 'path src/entry.ts src/model.ts');
  assert.equal(r.found, true);
  assert.equal(r.rows[0].load, 'type');
  assert.deepEqual(r.rows[0].evidence.kinds, ['type-import']);
  assert.match(formatResult(r), /type/);
  const runtime = runQuery(D, 'path src/entry.ts src/model.ts --runtime');
  assert.equal(runtime.found, false);
  assert.equal(runtime.load, 'runtime');
});

test('--runtime drops type-only imports from walks; --eager also drops dynamic imports', () => {
  assert.ok(ids(runQuery(D, 'deps src/entry.ts')).includes('src/types.ts'));
  assert.deepEqual(ids(runQuery(D, 'deps src/entry.ts --runtime')), ['src/a.ts', 'src/b.ts', 'src/fence.ts', 'src/lazy.ts']);
  assert.deepEqual(ids(runQuery(D, 'deps src/entry.ts --eager')), ['src/a.ts', 'src/b.ts', 'src/fence.ts']);
  assert.ok(!ids(runQuery(D, 'impact src/model.ts --runtime')).includes('src/entry.ts'));
});

test('reach: files and bytes an entry loads, eager vs lazy, lazy boundaries, packages and their chains', () => {
  const r = runQuery(D, 'reach src/entry.ts');
  assert.equal(r.ok, true);
  const runtime = ['src/a.ts', 'src/b.ts', 'src/big.ts', 'src/fence.ts', 'src/heavy.ts', 'src/lazy.ts', 'src/leaf.ts', 'src/util.ts'];
  assert.deepEqual(ids(r), runtime);
  const lazy = r.rows.filter((x) => x.load === 'lazy').map((x) => x.id).toSorted();
  assert.deepEqual(lazy, ['src/heavy.ts', 'src/lazy.ts']);
  const sum = (list) => list.reduce((a, id) => a + BYTES[id], 0);
  assert.equal(r.stats.bytes, sum(['src/entry.ts', ...runtime]));
  assert.equal(r.stats.lazyBytes, sum(lazy));
  assert.deepEqual(r.boundaries, [{ source: 'src/entry.ts', target: 'src/lazy.ts', line: 3 }]);
  const pkg = Object.fromEntries(r.packages.map((p) => [p.id, p]));
  assert.equal(pkg['pkg:zod'], undefined, 'a type-only package import loads nothing');
  assert.equal(pkg['pkg:ai'].load, 'lazy');
  assert.deepEqual(pkg['pkg:ai'].chain, ['src/entry.ts', 'src/lazy.ts', 'src/heavy.ts', 'pkg:ai']);
  assert.equal(pkg['pkg:react'].load, 'eager', 'an import of unknown kind counts as loading');
  assert.equal(pkg['pkg:@x/app'].bytes, sum(['src/entry.ts', ...runtime]));
  assert.match(r.summary, /lazy 2 files/);
});

test('reach --eager answers startup alone; --forbid fails with the chain, holds when unreached, refuses unknown paths', () => {
  const eager = runQuery(D, 'reach src/entry.ts --eager');
  assert.ok(!ids(eager).includes('src/lazy.ts'));
  assert.ok(!eager.packages.some((p) => p.id === 'pkg:ai'));
  const broken = runQuery(D, 'reach src/entry.ts --forbid pkg:ai src/model.ts');
  assert.equal(broken.failed, true);
  assert.deepEqual(broken.violations.map((v) => v.target), ['pkg:ai']);
  assert.deepEqual(broken.violations[0].chain.map((h) => h.id), ['src/entry.ts', 'src/lazy.ts', 'src/heavy.ts', 'pkg:ai']);
  assert.match(formatResult(broken), /VIOLATION pkg:ai/);
  const holds = runQuery(D, 'reach src/entry.ts --eager --forbid pkg:ai');
  assert.equal(holds.failed, undefined);
  assert.match(holds.summary, /rule holds/);
  assert.equal(runQuery(D, 'reach src/entry.ts --forbid src/nope.ts').ok, false);
});

test('hubs: ranked by importers × bytes one import loads; per importer, names taken and re-export-only importers', () => {
  const ranked = runQuery(D, 'hubs');
  const util = ranked.rows.find((x) => x.id === 'src/util.ts');
  assert.equal(util.importers, 2);
  assert.equal(util.closureBytes, BYTES['src/util.ts'] + BYTES['src/leaf.ts']);
  assert.equal(util.value, 2 * util.closureBytes);
  const r = runQuery(D, 'hubs src/util.ts');
  const by = Object.fromEntries(r.rows.map((x) => [x.id, x]));
  assert.equal(by['src/fence.ts'].verdict, 'reexports-only');
  assert.deepEqual(by['src/fence.ts'].direct, ['src/leaf.ts']);
  assert.equal(by['src/b.ts'].verdict, 'own-names');
  assert.equal(r.rows[0].id, 'src/fence.ts', 'importers that can skip the hub come first');
});

test('path --all: one route per distinct first hop', () => {
  const r = runQuery(D, 'path src/entry.ts src/leaf.ts --all');
  assert.deepEqual(r.rows.map((x) => x.path), [
    ['src/entry.ts', 'src/b.ts', 'src/util.ts', 'src/leaf.ts'],
    ['src/entry.ts', 'src/fence.ts', 'src/util.ts', 'src/leaf.ts'],
  ]);
  assert.equal(runQuery(D, 'path src/entry.ts src/leaf.ts --all 1').rows.length, 1);
});

test('runtime metrics and cycles recount over the imports that run', () => {
  const all = runQuery(D, 'top inweight all');
  const runtime = runQuery(D, 'top inweight all --runtime');
  const value = (r, id) => r.rows.find((x) => x.id === id)?.value;
  assert.equal(value(all, 'src/types.ts'), 1);
  assert.equal(value(runtime, 'src/types.ts'), 0);
  assert.ok(runQuery(D, 'cycle').rows.some((x) => x.members.includes('src/x.ts')));
  assert.equal(runQuery(D, 'cycle --runtime').rows.length, 0);
  assert.equal(runQuery(D, 'cycle src/x.ts --runtime').rows.length, 0);
});

test('--count answers the count; --group folds rows by folder, chunk, package or depth with bytes', () => {
  const counted = runQuery(D, 'dependents src/util.ts depth all --count');
  assert.deepEqual(counted.rows, []);
  assert.equal(counted.count, 3);
  const g = runQuery(D, 'reach src/entry.ts --group dir');
  assert.deepEqual(g.rows.map((x) => [x.id, x.count]), [['src', 8]]);
  assert.equal(g.rows[0].bytes, Object.entries(BYTES).filter(([id]) => ids(runQuery(D, 'reach src/entry.ts')).includes(id)).reduce((a, [, b]) => a + b, 0));
  assert.deepEqual(runQuery(D, 'reach src/entry.ts --group depth').rows.map((x) => x.id).toSorted(), ['1', '2', '3']);
  assert.equal(runQuery(D, 'deps src/entry.ts --group nonsense').ok, false);
});

test('a walk over every edge marks files reached only through import type or import(), and names --runtime', () => {
  const r = runQuery(D, 'deps src/entry.ts');
  const load = Object.fromEntries(r.rows.map((x) => [x.id, x.load]));
  assert.equal(load['src/types.ts'], 'type');
  assert.equal(load['src/lazy.ts'], 'deferred');
  assert.equal(load['src/a.ts'], undefined);
  assert.match(r.summary, /1 file reached through a type-only import .*--runtime/);
  assert.match(runQuery(D, 'path src/entry.ts src/model.ts').summary, /type-only import.*--runtime/);
  assert.match(formatResult(runQuery(D, 'path src/entry.ts src/model.ts --all')), /→\[type\] src\/types\.ts/);
});

test('hubs ranks under a directory when one is named; reach counts runtime builtins apart from packages', () => {
  const scoped = runQuery(D, 'hubs all src');
  assert.ok(scoped.rows.length > 0 && scoped.rows.every((x) => x.id.startsWith('src/')));
  assert.match(scoped.summary, /under src\//);
  const data = structuredClone(D);
  data.externals.push({ id: 'pkg:node:fs', count: 1, files: ['src/a.ts'], kinds: { 'src/a.ts': ['external-builtin', 'static-import'] } });
  const r = runQuery(data, 'reach src/entry.ts');
  assert.equal(r.packages.find((p) => p.id === 'pkg:node:fs').kind, 'builtin');
  assert.equal(r.stats.builtin, 1);
  assert.match(r.summary, /plus 1 builtin module/);
});

test('a pair joined by import type and import() cites the import() line, and hubs takes names only from imports that run', () => {
  const data = structuredClone(D);
  // src/cmd.ts: line 3 `import type { Saved } from './util'`, line 40 `await import('./util')`.
  data.files.push(file('src/cmd.ts'));
  data.imports.push({ ...imp('src/cmd.ts', 'src/util.ts', 3, ['type-import', 'dynamic-import']), lines: [3, 40], kindLines: { 'type-import': [3], 'dynamic-import': [40] } });
  data.uses.push({ source: 'src/cmd.ts', target: 'src/util.ts#Saved', line: 3, detail: 'import' });
  computeMetrics(data);
  const r = runQuery(data, 'reach src/cmd.ts');
  assert.deepEqual(r.boundaries, [{ source: 'src/cmd.ts', target: 'src/util.ts', line: 40 }]);
  assert.deepEqual(r.rows.find((x) => x.id === 'src/util.ts').importedAt, { path: 'src/cmd.ts', line: 40 });
  const hub = runQuery(data, 'hubs src/util.ts').rows.find((x) => x.id === 'src/cmd.ts');
  assert.equal(hub.verdict, 'names-unknown', 'a type-only name is not what a lazy import takes');
  assert.equal(hub.loc.line, 40);
  assert.equal(runQuery(data, 'path src/cmd.ts src/util.ts --runtime').rows[0].loc.line, 40);
});

test('folding witnesses keeps each import kind with its lines', async () => {
  const { foldEdges } = await import('../src/runtime/section.ts');
  const [edge] = foldEdges([
    { edge: 'imports', from: 'a.ts', to: 'b.ts', line: 3, confidence: 'high', detail: 'type-import' },
    { edge: 'imports', from: 'a.ts', to: 'b.ts', line: 40, confidence: 'high', detail: 'dynamic-import' },
  ], 'imports');
  assert.deepEqual(edge.kindLines, { 'type-import': [3], 'dynamic-import': [40] });
});

test('reach takes a directory as entries; a forbidden package nothing imports is an error with near names', () => {
  const dir = runQuery(D, 'reach src --eager');
  assert.equal(dir.ok, true);
  assert.ok(dir.nodes.includes('src/entry.ts') && dir.nodes.includes('src/x.ts'));
  const typo = runQuery(D, 'reach src/entry.ts --forbid pkg:a1');
  assert.equal(typo.ok, false);
  assert.match(typo.error, /pkg:a1 names no package this graph imports/);
  assert.ok(typo.candidates.some((c) => c.id === 'pkg:ai'));
  assert.equal(runQuery(D, 'reach src/entry.ts --eager --forbid pkg:zod').failed, undefined, 'a package imported only as types is known and not loaded');
});

test('the reach summary counts what it names: its own code by package, then packages it imports', () => {
  const r = runQuery(D, 'reach src/entry.ts');
  assert.match(r.summary, /own code: pkg:@x\/app /);
  assert.match(r.summary, /2 imported packages \(2 external, 0 workspace\)/);
  assert.match(formatResult(runQuery(D, 'hubs')), /= \d+(\.\d+)? (B|KiB|MiB) loaded across importers/);
});

test('every hop cites the line of the import that decides its load, in any walk', () => {
  const data = structuredClone(D);
  data.files.push(file('src/cmd.ts'));
  data.imports.push({ ...imp('src/cmd.ts', 'src/util.ts', 3, ['type-import', 'dynamic-import']), lines: [3, 40], kindLines: { 'type-import': [3], 'dynamic-import': [40] } });
  computeMetrics(data);
  const hop = runQuery(data, 'path src/cmd.ts src/util.ts').rows[0];
  assert.equal(hop.load, 'deferred');
  assert.equal(hop.loc.line, 40);
  assert.equal(runQuery(data, 'deps src/cmd.ts').rows[0].loc.line, 1, 'a dependency row opens its own file');
  assert.equal(runQuery(data, 'dependents src/util.ts').rows.find((x) => x.id === 'src/cmd.ts').loc.line, 40);
});

test('a query that starts with a file names the queries to ask about it, not the whole grammar', () => {
  const one = runQuery(D, 'src/entry.ts');
  assert.equal(one.ok, false);
  assert.match(one.error, /starts with a file, not a query/);
  assert.ok(one.error.length < 300, one.error);
  assert.ok(one.candidates.some((c) => c.query === 'about src/entry.ts'));
  const two = runQuery(D, 'src/entry.ts src/big.ts');
  assert.ok(two.candidates.some((c) => c.query === 'path src/entry.ts src/big.ts'));
});

test('sizes are binary units; --count prints the count; a barrel that re-exports a hub takes those names', () => {
  assert.match(runQuery(D, 'reach src/entry.ts').summary, /KiB/);
  assert.match(formatResult(runQuery(D, 'dependents src/util.ts depth all --count')), /^count: 3$/m);
  const data = structuredClone(D);
  data.files.push(file('src/index.ts'));
  data.imports.push(imp('src/index.ts', 'src/util.ts', 1, ['named-reexport']));
  data.uses.push({ source: 'src/index.ts', target: 'src/util.ts#own', line: 1, detail: 'reexported-name' });
  computeMetrics(data);
  const barrel = runQuery(data, 'hubs src/util.ts').rows.find((x) => x.id === 'src/index.ts');
  assert.equal(barrel.verdict, 'own-names');
  assert.deepEqual(barrel.names, ['own (reexported-name)']);
});

test('load walks keep resolved calls of languages that bind at compile time, never of module-loading languages', () => {
  // C# links through namespaces (no file imports): its calls are its only dependency evidence. A JS call with no
  // import loads nothing.
  const cs = (id) => ({ ...file(id), language: 'csharp' });
  const data = {
    section: '/repo',
    files: [cs('cs/Main.cs'), cs('cs/Json.cs'), cs('cs/Util.cs'), file('src/a.ts'), file('src/b.ts'), file('src/c.ts')],
    // TypeScript loads modules through imports: one import anywhere says so.
    imports: [imp('src/c.ts', 'src/b.ts', 1, ['static-import'])],
    symbols: [],
    calls: [
      { source: 'cs/Main.cs#Main', target: 'cs/Json.cs#Parse', line: 3, lines: [3], count: 1, confidence: 'high', via: 'graph', test: false },
      { source: 'cs/Json.cs#Parse', target: 'cs/Util.cs#Trim', line: 9, lines: [9], count: 2, confidence: 'high', via: 'graph', test: false },
      { source: 'cs/Main.cs#Main', target: 'cs/Util.cs#Weak', line: 4, lines: [4], count: 1, confidence: 'low', via: 'graph', test: false },
      { source: 'src/a.ts#f', target: 'src/b.ts#g', line: 2, lines: [2], count: 1, confidence: 'high', via: 'graph', test: false },
    ],
    externals: [],
  };
  computeMetrics(data);
  const r = runQuery(data, 'reach cs/Main.cs');
  assert.deepEqual(ids(r), ['cs/Json.cs', 'cs/Util.cs']);
  assert.equal(r.rows.find((x) => x.id === 'cs/Util.cs').depth, 2, 'a low-confidence call is not followed without --weak');
  assert.deepEqual(ids(runQuery(data, 'reach src/a.ts')), [], 'a TS call with no import loads nothing');
  assert.equal(runQuery(data, 'deps cs/Main.cs --runtime').rows.length, 1);
});

// The 6 MB-worker shape: worker → a | b → shared/util → pkg:@x/ai; worker ⇢ lazy (import()) → pkg:@x/ai; worker ⇢
// types (import type) → pkg:@x/ai, which never loads it.
function workerFixture() {
  const names = ['src/worker.ts', 'src/a.ts', 'src/b.ts', 'src/shared/util.ts', 'src/types.ts', 'src/lazy.ts'];
  const data = {
    section: '/repo',
    files: names.map(file),
    imports: [
      imp('src/worker.ts', 'src/a.ts', 1, ['static-import']),
      imp('src/worker.ts', 'src/b.ts', 2, ['static-import']),
      imp('src/worker.ts', 'src/types.ts', 3, ['type-import']),
      imp('src/worker.ts', 'src/lazy.ts', 4, ['dynamic-import']),
      imp('src/a.ts', 'src/shared/util.ts', 2, ['static-import']),
      imp('src/b.ts', 'src/shared/util.ts', 3, ['static-import']),
    ],
    symbols: [],
    calls: [],
    externals: [
      {
        id: 'pkg:@x/ai',
        count: 3,
        files: ['src/shared/util.ts', 'src/types.ts', 'src/lazy.ts'],
        kinds: { 'src/shared/util.ts': ['external', 'static-import'], 'src/types.ts': ['external', 'type-import'], 'src/lazy.ts': ['external', 'static-import'] },
        lines: { 'src/shared/util.ts': [7], 'src/types.ts': [1], 'src/lazy.ts': [5] },
      },
    ],
  };
  computeMetrics(data);
  return data;
}

test('V5: why <entry> <pkg> lists the route through every first hop, each hop with its line and kinds', () => {
  const data = workerFixture();
  const r = runQuery(data, 'why src/worker.ts pkg:@x/ai');
  assert.equal(r.ok, true, r.error);
  assert.equal(r.found, true);
  assert.deepEqual(r.rows.map((x) => x.chain.map((h) => h.id)), [
    ['src/worker.ts', 'src/lazy.ts', 'pkg:@x/ai'],
    ['src/worker.ts', 'src/a.ts', 'src/shared/util.ts', 'pkg:@x/ai'],
    ['src/worker.ts', 'src/b.ts', 'src/shared/util.ts', 'pkg:@x/ai'],
  ]);
  assert.deepEqual(r.rows[1].chain.map((h) => [h.line ?? null, h.kinds ?? null]), [
    [1, ['static-import']],
    [2, ['static-import']],
    [7, ['external', 'static-import']],
    [null, null],
  ]);
  assert.deepEqual(r.rows[0].chain[0].kinds, ['dynamic-import']);
  assert.match(formatResult(r), /route 2: src\/worker\.ts:1 \[static-import → eager\] → src\/a\.ts:2 \[static-import → eager\] → src\/shared\/util\.ts:7 \[external,static-import → eager\] → pkg:@x\/ai/);
  assert.equal(runQuery(data, 'why src/worker.ts pkg:@x/ai --eager').rows.length, 2, 'import() is not startup');

  const toFile = runQuery(data, 'why src/worker.ts src/shared/util.ts');
  assert.deepEqual(toFile.rows.map((x) => x.chain.at(-2).id), ['src/a.ts', 'src/b.ts']);
  const typeOnly = runQuery(data, 'why src/worker.ts src/types.ts');
  assert.equal(typeOnly.ok, true);
  assert.equal(typeOnly.found, false);
  assert.match(typeOnly.summary, /does not load src\/types\.ts at run time/);
  const typo = runQuery(data, 'why src/worker.ts pkg:@x/a1');
  assert.equal(typo.ok, false);
  assert.ok(typo.candidates.some((c) => c.id === 'pkg:@x/ai'));
});

test('reach rows carry their chain from the entry, each hop with its import line and kinds', () => {
  const r = runQuery(workerFixture(), 'reach src/worker.ts');
  const util = r.rows.find((x) => x.id === 'src/shared/util.ts');
  assert.equal(util.chain.length, 3);
  assert.equal(util.chain[0].id, 'src/worker.ts');
  assert.equal(util.chain.at(-1).id, 'src/shared/util.ts');
  assert.ok(util.chain.slice(0, -1).every((h) => Number.isInteger(h.line) && h.kinds.includes('static-import')));
  const ai = r.packages.find((p) => p.id === 'pkg:@x/ai');
  assert.deepEqual(ai.chain, ['src/worker.ts', 'src/lazy.ts', 'pkg:@x/ai']);
});

test('reach shows an external package its build measured, with how, and totals what it loads', () => {
  const data = structuredClone(D);
  const ai = data.externals.find((x) => x.id === 'pkg:ai');
  Object.assign(ai, { bytes: 6_000_000, bytesMethod: 'installed-closure', bytesEvidence: { source: 'node_modules/ai/index.js', files: 120, version: '4.1.0' } });
  const r = runQuery(data, 'reach src/entry.ts');
  const pkg = r.packages.find((p) => p.id === 'pkg:ai');
  assert.equal(pkg.bytes, 6_000_000);
  assert.equal(pkg.bytesMethod, 'installed-closure');
  assert.equal(r.packages[0].id, 'pkg:ai', 'the heaviest package comes first');
  assert.equal(r.stats.packageBytes, 6_000_000);
  assert.match(r.summary, /pkg:ai 5\.72 MiB \(installed-closure\)/);
  assert.equal(r.packages.find((p) => p.id === 'pkg:react').bytes, null, 'an unmeasured package stays unknown');
});

test("the load the engine recorded decides every walk, over the package's own kind table", () => {
  // `future-import` is a kind the package's table has never heard of; the engine says it is type-only.
  const data = structuredClone(D);
  data.files.push(file('src/next.ts'), file('src/lib.ts'));
  data.imports.push({ ...imp('src/next.ts', 'src/lib.ts', 1, ['future-import']), loads: { 'future-import': 'type' } });
  data.externals.push({ id: 'pkg:zz', count: 1, files: ['src/next.ts'], kinds: { 'src/next.ts': ['external', 'future-import'] }, loads: { 'future-import': 'type' } });
  computeMetrics(data);
  assert.equal(runQuery(data, 'deps src/next.ts').rows[0].load, 'type');
  assert.deepEqual(ids(runQuery(data, 'deps src/next.ts --runtime')), []);
  const r = runQuery(data, 'reach src/next.ts');
  assert.deepEqual(ids(r), []);
  assert.ok(!r.packages.some((p) => p.id === 'pkg:zz'), 'a type-only package import loads nothing');
  assert.equal(r.stats.unsettled, 0);
});

test('each chain hop names the load its import kinds amount to, so a mixed hop says why it counts', () => {
  const data = structuredClone(D);
  data.imports.find((e) => e.source === 'src/entry.ts' && e.target === 'src/a.ts').resolution = ['type-import', 'static-import'];
  computeMetrics(data);
  const r = runQuery(data, 'why src/entry.ts src/big.ts');
  const text = formatResult(r);
  assert.match(text, /src\/entry\.ts:1 \[type-import,static-import → eager\]/);
  const lazy = runQuery(data, 'reach src/entry.ts --forbid pkg:ai');
  assert.equal(lazy.violations[0].chain[0].load, 'deferred');
  assert.match(formatResult(lazy), /\[dynamic-import → deferred\]/);
});

test('a file path written from a directory above the build root names the same graph file; a literal id still wins', () => {
  const data = { section: '/home/me/pkg/src', files: ['cli.ts', 'lib/a.ts', 'src/x.ts'].map(file), imports: [], symbols: [], calls: [] };
  computeMetrics(data);
  const named = (q) => runQuery(data, q).nodes?.[0];
  assert.equal(named('files src/cli.ts'), 'cli.ts');
  assert.equal(named('files pkg/src/lib/a.ts'), 'lib/a.ts');
  assert.equal(named('files src/cli.ts:3'), 'cli.ts');
  assert.equal(named('files src/x.ts'), 'src/x.ts', 'a graph file spelled that way is taken first');
  // A path whose leading directories are not the root's own is never trimmed to a file of the same name.
  assert.equal(runQuery(data, 'files other/cli.ts').ok, false);
  assert.equal(runQuery(data, 'about other/src/cli.ts').ok, false);
});
