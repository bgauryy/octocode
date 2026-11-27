// The engine states each import's load (graphQuery `load` on import witnesses and import diagnostics, from its one
// kind table); the package folds it into the graph and reads it first, keeping its kind mirror only for graphs built
// before the engine stated it.
import assert from 'node:assert/strict';
import test from 'node:test';
import { loadOf, loadOfKind } from '../src/loads.ts';
import { externalsOf, foldEdges, symbolRows } from '../src/runtime/section.ts';
import { stitch } from '../src/runtime/stitch.ts';

const witness = (to, line, detail, load, importKind) => ({
  edge: 'imports', from: 'a.ts', to, line, confidence: 'high', detail, ...(load && { load }), ...(importKind && { importKind }),
});

test('a recorded load wins over the kind mirror; a kind with none recorded falls back to it', () => {
  assert.equal(loadOfKind('static-import', { 'static-import': 'deferred' }), 'deferred');
  assert.equal(loadOfKind('namespace-use', {}), 'compile');
  assert.equal(loadOf(['some-new-kind'], { 'some-new-kind': 'type' }), 'type');
  assert.equal(loadOf(['some-new-kind']), 'unknown');
  // The strongest recorded load of the edge's kinds.
  assert.equal(loadOf(['type-import', 'dynamic-import'], { 'type-import': 'type', 'dynamic-import': 'deferred' }), 'deferred');
});

test('folding import witnesses keeps the load the engine states for each kind', () => {
  const [file] = foldEdges([witness('b.ts', 1, 'type-import', 'type'), witness('b.ts', 2, 'static-import', 'eager')], 'imports');
  assert.deepEqual(file.loads, { 'type-import': 'type', 'static-import': 'eager' });
  assert.equal(loadOf(file.resolution, file.loads), 'eager');
  // A package import's load belongs to its kind, not to `detail` (how the package is declared).
  const [pkg] = foldEdges([witness('pkg:big', 5, 'external', 'deferred', 'dynamic-import')], 'imports');
  assert.deepEqual(pkg.loads, { 'dynamic-import': 'deferred' });
  // An engine before `load` records none.
  const [old] = foldEdges([witness('b.ts', 1, 'type-import')], 'imports');
  assert.equal(old.loads, undefined);
});

test('a package names each importing file its import lines and every recorded load', () => {
  const edges = foldEdges([
    witness('pkg:big', 5, 'external', 'deferred', 'dynamic-import'),
    witness('pkg:big', 1, 'external', 'type', 'type-import'),
    { ...witness('pkg:big', 9, 'external', 'eager', 'static-import'), from: 'c.ts' },
  ], 'imports');
  const [big] = externalsOf(edges);
  assert.deepEqual(big.lines, { 'a.ts': [1, 5], 'c.ts': [9] });
  assert.deepEqual(big.loads, { 'dynamic-import': 'deferred', 'type-import': 'type', 'static-import': 'eager' });
});

test('a declaration keeps the parameter list and return type its engine row states', () => {
  const { declarations } = symbolRows(
    [{ id: 'a.ts#run', path: 'a.ts', symbolName: 'run', kind: 'function', line: 1, params: '(g: Graph)', returns: 'Row[]' },
     { id: 'a.ts#n', path: 'a.ts', symbolName: 'n', kind: 'constant', line: 2 }],
    new Set(['a.ts']),
    [],
  );
  assert.deepEqual(declarations.map((d) => [d.name, d.params, d.returns]), [['run', '(g: Graph)', 'Row[]'], ['n', undefined, undefined]]);
  assert.ok(!('params' in declarations[1]), 'no empty field for a value');
});

test('stitching keeps each chunk file’s package import lines and loads under its chunk path', () => {
  const node = (id) => ({ id, label: id, group: '.', fanIn: 0, fanOut: 0 });
  const empty = { imports: [], symbols: [], calls: [], insights: { findings: {}, issues: [], cycles: { all: [] } } };
  const datas = new Map([
    ['app', { ...empty, files: [node('m.ts')], externals: [{ id: 'pkg:big', count: 1, files: ['m.ts'], kinds: { 'm.ts': ['external', 'dynamic-import'] }, lines: { 'm.ts': [5] }, loads: { 'dynamic-import': 'deferred' } }] }],
    ['web', { ...empty, files: [node('w.ts')], externals: [{ id: 'pkg:big', count: 1, files: ['w.ts'], kinds: { 'w.ts': ['external', 'static-import'] }, lines: { 'w.ts': [2] }, loads: { 'static-import': 'eager' } }] }],
  ]);
  const { whole } = stitch({ root: '/r', chunks: [{ id: 'app', path: 'app' }, { id: 'web', path: 'web' }], datas, files: ['app/m.ts', 'web/w.ts'] });
  const [big] = whole.externals;
  assert.deepEqual(big.lines, { 'app/m.ts': [5], 'web/w.ts': [2] });
  assert.deepEqual(big.loads, { 'dynamic-import': 'deferred', 'static-import': 'eager' });
});
