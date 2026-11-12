import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { bindCalls, chunkOf, packageKey, resolveImport, resolveRustPath, stitch } from './stitch.mjs';
import { tempDir } from './test-tmp.mjs';

const has = (p) => ['lib/a.ts', 'lib/b/index.js', 'c.mjs'].includes(p);
const node = (id) => ({ id, label: id, group: '.', fanIn: 0, fanOut: 0 });
const decl = (file, name) => ({ id: `${file}#${name}`, file, name: name.split('.').pop() });
const byText = (a, b) => a.localeCompare(b);

test('chunk membership is the deepest chunk unless an excluded name prunes the file', () => {
  const chunks = [{ id: 'root', path: '.', exclude: ['a', 'src'] }, { id: 'a', path: 'a' }];
  assert.equal(chunkOf('a/x.ts', chunks).id, 'a');
  assert.equal(chunkOf('top.ts', chunks).id, 'root');
  assert.equal(chunkOf('tiny/src/x.ts', chunks), null);
});

test('relative specifiers resolve by module suffixes, and package ids normalize', () => {
  assert.equal(resolveImport('tests/t.ts', '../lib/a.js', has), 'lib/a.ts');
  assert.equal(resolveImport('tests/t.ts', '../lib/b', has), 'lib/b/index.js');
  assert.equal(resolveImport('x/y.mjs', '../c.mjs', has), 'c.mjs');
  assert.equal(resolveImport('t.ts', '../../outside.ts', has), null);
  assert.equal(resolveImport('t.ts', 'react', has), null);
  assert.equal(packageKey('pkg:@x/core/sub'), '@x/core');
  assert.equal(packageKey('pkg:octocode_contract_types'), packageKey('octocode-contract-types'));
});

test('stitching joins chunks file to file and keeps unmatched imports as gaps', () => {
  const chunks = [{ id: 'lib', path: 'lib', package: '@x/lib' }, { id: 'tests', path: 'tests' }];
  const datas = new Map([
    ['lib', { files: [node('a.ts')], imports: [], symbols: [{ id: 'a.ts#f', file: 'a.ts', name: 'f' }], calls: [], fnFiles: ['a.ts'], externals: [], insights: { findings: { cycle: 1 }, issues: [{ detector: 'cycle', subject: 'a.ts' }], cycles: { all: [] } } }],
    ['tests', { files: [node('t.ts'), node('u.ts')], imports: [], symbols: [], calls: [], externals: [{ id: 'pkg:@x/lib/sub', count: 1, files: ['u.ts'] }], outsideRoot: [{ file: 't.ts', line: 2, specifier: '../lib/a.js' }, { file: 't.ts', line: 3, specifier: '../lib/gone.js' }], insights: { findings: {}, issues: [], cycles: { all: [] } } }],
  ]);
  const { cross, unresolved, whole } = stitch({ root: '/r', chunks, datas, files: ['lib/a.ts', 'tests/t.ts', 'tests/u.ts'] });
  assert.deepEqual(cross.map((e) => `${e.source}>${e.target}:${e.via}`), ['tests/t.ts>lib/a.ts:relative', 'tests/u.ts>pkg:@x/lib:package']);
  assert.deepEqual(unresolved.map((u) => u.specifier), ['../lib/gone.js']);
  assert.deepEqual(whole.files.map((f) => f.id).toSorted(byText), ['lib/a.ts', 'pkg:@x/lib', 'tests/t.ts', 'tests/u.ts']);
  assert.equal(whole.imports.filter((e) => e.crossChunk).length, 2);
  assert.equal(whole.symbols[0].id, 'lib/a.ts#f');
  assert.equal(whole.insights.issues[0].subject, 'lib/a.ts');
  assert.equal(whole.files.find((f) => f.id === 'lib/a.ts').fanIn, 1);
});

test('root-level files group under the section root', () => {
  const datas = new Map([['root', { files: [node('cache.rs'), node('cache/disk.rs'), node('lone.rs')], imports: [{ source: 'cache.rs', target: 'cache/disk.rs', line: 1 }], symbols: [], calls: [], externals: [], insights: { findings: {}, issues: [], cycles: { all: [] } } }]]);
  const { whole } = stitch({ root: '/r', chunks: [{ id: 'root', path: '.' }], datas, files: ['cache.rs', 'cache/disk.rs', 'lone.rs'] });
  assert.deepEqual(Object.fromEntries(whole.files.map((f) => [f.id, f.group])), { 'cache.rs': '.', 'cache/disk.rs': 'cache', 'lone.rs': '.' });
});

test('calls between chunks bind exact imported names only; members, unimported and ambiguous names stay unbound', () => {
  const chunks = [{ id: 'lib', path: 'lib' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['lib', { declarations: [decl('a.ts', 'run'), decl('a.ts', 'Box.read'), decl('b.ts', 'only'), decl('b.ts', 'dup'), decl('c.ts', 'dup')] }],
    ['app', { declarations: [decl('m.ts', 'main')], unresolvedCalls: [
      { from: 'm.ts#main', file: 'm.ts', targetName: 'run', line: 3 },
      { from: 'm.ts#main', file: 'm.ts', targetName: 'run', line: 4 },
      { from: 'm.ts#main', file: 'm.ts', targetName: 'read', line: 5 },
      { from: 'm.ts#main', file: 'm.ts', targetName: 'x.run', line: 6 },
      { from: 'm.ts#main', file: 'm.ts', targetName: 'only', line: 7 },
      { from: 'm.ts#main', file: 'm.ts', targetName: 'dup', line: 8 },
    ].map((u) => ({ ...u, reason: 'not-declared-in-repo' })) }],
  ]);
  const cross = [{ source: 'app/m.ts', target: 'lib/a.ts', via: 'relative', sourceChunk: 'app', targetChunk: 'lib' }];
  const rows = bindCalls({ chunks, datas, cross });
  assert.deepEqual(rows.map((e) => [e.target, e.confidence, e.resolution[0], e.count]), [
    ['lib/a.ts#run', 'medium', 'stitched-import', 2],
  ]);
});

test('rust crate, super and self paths resolve to the deepest module file', () => {
  const files = new Set(['c/Cargo.toml', 'c/src/lib.rs', 'c/src/tools.rs', 'c/src/tools/graph.rs', 'c/src/tools/graph/walk.rs', 'c/src/a/mod.rs', 'c/src/a/b.rs']);
  const inCrate = (p) => files.has(p);
  assert.equal(resolveImport('c/src/tools/graph/walk.rs', 'crate::tools::graph::climb', inCrate), 'c/src/tools/graph.rs');
  assert.equal(resolveImport('c/src/tools/graph/walk.rs', 'super::climb', inCrate), 'c/src/tools/graph.rs');
  assert.equal(resolveImport('c/src/tools/graph/walk.rs', 'super::super::Item', inCrate), 'c/src/tools.rs');
  assert.equal(resolveImport('c/src/a/mod.rs', 'self::b::f', inCrate), 'c/src/a/b.rs');
  assert.equal(resolveImport('c/src/a/b.rs', 'super::f', inCrate), 'c/src/a/mod.rs');
  assert.equal(resolveImport('c/src/a/b.rs', 'crate::a::b::g', inCrate), null); // its own file: no edge
  assert.equal(resolveImport('x/y.rs', 'crate::z', inCrate), null); // no Cargo.toml above
});

test('rust paths from a root inside a crate resolve from the root module, and climbing out stays unresolved', () => {
  const files = new Set(['limits.rs', 'store/ingest.rs', 'store/tests/limits.rs']);
  const inRoot = (p) => files.has(p);
  // The root is the crate's `tools/ast_graph` module.
  assert.equal(resolveRustPath('store/tests/limits.rs', 'crate::tools::ast_graph::limits::LIMITS', inRoot, { cratePrefix: ['tools', 'ast_graph'] }), 'limits.rs');
  assert.equal(resolveRustPath('store/tests/limits.rs', 'super::super::ingest::build', inRoot), 'store/ingest.rs');
  assert.equal(resolveRustPath('store/tests/limits.rs', 'crate::other::x', inRoot, { cratePrefix: ['tools', 'ast_graph'] }), null);
  assert.equal(resolveRustPath('store/tests/limits.rs', 'crate::tools::ast_graph::limits::LIMITS', inRoot), null);
});

test('package calls bind only names the engine saw imported, never macros or modules, and qualified names only into their package', () => {
  const chunks = [{ id: 'lib', path: 'lib', package: 'lib' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['lib', { files: [node('m.rs')], imports: [], symbols: [], calls: [], externals: [], declarations: [{ id: 'm.rs#json', file: 'm.rs', name: 'json', kind: 'module' }, { id: 'm.rs#run', file: 'm.rs', name: 'run', kind: 'function' }, { id: 'm.rs#format', file: 'm.rs', name: 'format', kind: 'function' }], insights: { findings: {}, issues: [], cycles: { all: [] } } }],
    ['app', { files: [node('main.rs')], imports: [], symbols: [], calls: [], externals: [{ id: 'pkg:lib', count: 1, files: ['main.rs'] }], unresolvedCalls: [{ from: 'main.rs#main', file: 'main.rs', targetName: 'json', line: 3, reason: 'external-package' }, { from: 'main.rs#main', file: 'main.rs', targetName: 'format', line: 4, reason: 'not-declared-in-repo' }, { from: 'main.rs#main', file: 'main.rs', targetName: 'run', line: 5, reason: 'external-package' }, { from: 'main.rs#go', file: 'main.rs', targetName: 'lib::run', line: 6, reason: 'external-package' }, { from: 'main.rs#go', file: 'main.rs', targetName: 'other::run', line: 7, reason: 'external-package' }], insights: { findings: {}, issues: [], cycles: { all: [] } } }],
  ]);
  const { crossCalls } = stitch({ root: '/r', chunks, datas, files: ['lib/m.rs', 'app/main.rs'] });
  assert.deepEqual(crossCalls.map((c) => [c.source, c.target]), [['app/main.rs#main', 'lib/m.rs#run'], ['app/main.rs#go', 'lib/m.rs#run']]);
});

test('rust paths into another workspace crate resolve to its deepest module file by the crate name `use` spells', () => {
  const root = tempDir('stitch-crates-');
  const manifests = {
    'crates/io/Cargo.toml': '[package]\nname = "octocode-io"\n\n[lib]\ndoctest = false\n',
    'crates/rt/Cargo.toml': '[package]\nname = "rt-pkg"\n\n[lib]\nname = "rt_core"\npath = "lib/root.rs"\n\n[dependencies]\nname = "not-the-lib"\n',
    'crates/cli/Cargo.toml': '[package]\nname = "octocode-cli"\n',
  };
  for (const [p, text] of Object.entries(manifests)) {
    mkdirSync(join(root, dirname(p)), { recursive: true });
    writeFileSync(join(root, p), text);
  }
  const files = [...Object.keys(manifests), 'crates/io/src/lib.rs', 'crates/io/src/memo.rs', 'crates/rt/lib/root.rs', 'crates/rt/lib/config.rs', 'crates/rt/lib/config/edit.rs', 'crates/cli/src/main.rs', 'crates/cli/src/run.rs'];
  const chunks = [{ id: 'io', path: 'crates/io', package: 'octocode-io' }, { id: 'rt', path: 'crates/rt', package: 'rt-pkg' }, { id: 'cli', path: 'crates/cli', package: 'octocode-cli' }];
  const empty = { files: [], imports: [], symbols: [], calls: [], externals: [], insights: { findings: {}, issues: [], cycles: { all: [] } } };
  const fn = (file, name) => ({ id: `${file}#${name}`, file, name, kind: 'function' });
  const datas = new Map([
    ['io', { ...empty, files: [node('src/lib.rs'), node('src/memo.rs')], declarations: [fn('src/memo.rs', 'fresh'), fn('src/lib.rs', 'fresh')] }],
    ['rt', { ...empty, files: [node('lib/root.rs'), node('lib/config.rs'), node('lib/config/edit.rs')], declarations: [fn('lib/config/edit.rs', 'apply'), fn('lib/config/edit.rs', 'new')] }],
    ['cli', { ...empty, files: [node('src/main.rs'), node('src/run.rs')], declarations: [fn('src/run.rs', 'go')],
      outsideRoot: [
        { file: 'src/run.rs', line: 1, specifier: 'octocode_io::memo::Memo' },
        { file: 'src/run.rs', line: 2, specifier: 'rt_core::config::edit::apply' },
        { file: 'src/run.rs', line: 3, specifier: 'octocode_io::Thing' },
        { file: 'src/run.rs', line: 4, specifier: 'octocode_io::*' },
        { file: 'src/main.rs', line: 1, specifier: 'octocode_io::memo' },
        { file: 'src/main.rs', line: 2, specifier: 'rt_pkg::config::Edit' },
        { file: 'src/main.rs', line: 3, specifier: 'not_in_repo::x' },
      ],
      unresolvedCalls: [
        { from: 'src/run.rs#go', file: 'src/run.rs', targetName: 'apply', line: 5 },
        { from: 'src/run.rs#go', file: 'src/run.rs', targetName: 'octocode_io::memo::fresh', line: 6 },
        { from: 'src/run.rs#go', file: 'src/run.rs', targetName: 'rt_core::config::edit::Edit::new', line: 7 },
        { from: 'src/run.rs#go', file: 'src/run.rs', targetName: 'octocode_io::memo::fresh().unwrap', line: 8 },
      ].map((u) => ({ ...u, reason: 'not-declared-in-repo' })) }],
  ]);
  const { cross, unresolved, crossCalls, whole } = stitch({ root, chunks, datas, files });
  assert.deepEqual(cross.map((e) => [e.source, e.line, e.target, e.via, e.targetChunk]), [
    ['crates/cli/src/run.rs', 1, 'crates/io/src/memo.rs', 'crate-path', 'io'],
    ['crates/cli/src/run.rs', 2, 'crates/rt/lib/config/edit.rs', 'crate-path', 'rt'],
    ['crates/cli/src/run.rs', 3, 'crates/io/src/lib.rs', 'crate-path', 'io'],
    ['crates/cli/src/run.rs', 4, 'crates/io/src/lib.rs', 'crate-path', 'io'],
    ['crates/cli/src/main.rs', 1, 'crates/io/src/memo.rs', 'crate-path', 'io'],
  ]);
  assert.deepEqual(unresolved.map((u) => [u.specifier, u.reason]), [['rt_pkg::config::Edit', 'names no workspace crate'], ['not_in_repo::x', 'names no workspace crate']]);
  assert.deepEqual(crossCalls.map((e) => [e.source, e.target, e.line]), [['crates/cli/src/run.rs#go', 'crates/rt/lib/config/edit.rs#apply', 5], ['crates/cli/src/run.rs#go', 'crates/io/src/memo.rs#fresh', 6]]);
  assert.equal(whole.coverage.crossChunkImports, 4);
  assert.equal(whole.coverage.crossChunkCalls, 2);
});
