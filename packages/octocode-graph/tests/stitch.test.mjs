import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { bindCalls, chunkOf, packageKey, resolveImport, resolveRustPath, stitch } from '../src/runtime/stitch.ts';
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
    ['tests', { files: [{ ...node('t.ts'), roles: ['test'] }, node('u.ts')], imports: [], symbols: [], calls: [], externals: [{ id: 'pkg:@x/lib/sub', count: 1, files: ['u.ts'] }], outsideRoot: [{ file: 't.ts', line: 2, specifier: '../lib/a.js' }, { file: 't.ts', line: 3, specifier: '../lib/gone.js' }], insights: { findings: {}, issues: [], cycles: { all: [] } } }],
  ]);
  const { cross, unresolved, whole } = stitch({ root: '/r', chunks, datas, files: ['lib/a.ts', 'tests/t.ts', 'tests/u.ts'] });
  assert.deepEqual(cross.map((e) => `${e.source}>${e.target}:${e.via}`), ['tests/t.ts>lib/a.ts:relative', 'tests/u.ts>pkg:@x/lib:package']);
  assert.deepEqual(unresolved.map((u) => u.specifier), ['../lib/gone.js']);
  assert.deepEqual(whole.files.map((f) => f.id).toSorted(byText), ['lib/a.ts', 'pkg:@x/lib', 'tests/t.ts', 'tests/u.ts']);
  assert.equal(whole.imports.filter((e) => e.crossChunk).length, 2);
  assert.equal(whole.imports.find(e => e.source === 'tests/t.ts').test, true);
  assert.ok(!whole.imports.find(e => e.source === 'tests/u.ts').test);
  assert.equal(whole.files.find(f => f.id === 'pkg:@x/lib').fanIn, 1);
  assert.equal(whole.symbols[0].id, 'lib/a.ts#f');
  assert.equal(whole.insights.issues[0].subject, 'lib/a.ts');
  assert.equal(whole.files.find((f) => f.id === 'lib/a.ts').fanIn, 0);
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
  assert.deepEqual(rows.map((e) => [e.target, e.confidence, e.resolution.join(' '), e.count]), [
    ['lib/a.ts#run', 'medium', 'stitched-import import', 2],
  ]);
});

test('macros and functions are separate namespaces: `name!` binds only a macro, `name(..)` only a function', () => {
  const chunks = [{ id: 'lib', path: 'lib' }, { id: 'app', path: 'app' }];
  // lib/a.rs declares a macro `emit` and a function `emit`; b.rs only a function `log`; c.rs only a macro `trace`.
  const datas = new Map([
    ['lib', { declarations: [
      { ...decl('a.rs', 'emit'), id: 'a.rs#emit@3', kind: 'macro' }, { ...decl('a.rs', 'emit'), kind: 'function' },
      { ...decl('b.rs', 'log'), kind: 'function' }, { ...decl('c.rs', 'trace'), kind: 'macro' },
    ] }],
    ['app', { declarations: [decl('m.rs', 'main')], unresolvedCalls: [
      { from: 'm.rs#main', file: 'm.rs', targetName: 'emit!', line: 3 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'emit', line: 4 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'log!', line: 5 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'trace', line: 6 },
    ].map((u) => ({ ...u, reason: 'not-declared-in-repo' })) }],
  ]);
  const cross = ['a.rs', 'b.rs', 'c.rs'].map((f) => ({ source: 'app/m.rs', target: `lib/${f}`, via: 'crate-path', sourceChunk: 'app', targetChunk: 'lib' }));
  const rows = bindCalls({ chunks, datas, cross });
  assert.deepEqual(rows.map((e) => [e.target, e.lines]).sort(), [['lib/a.rs#emit', [4]], ['lib/a.rs#emit@3', [3]]]);
});

test('a re-exported name binds through one import of the module file, only to an exported declaration', () => {
  const chunks = [{ id: 'lib', path: 'lib' }, { id: 'app', path: 'app' }];
  // config.rs re-exports edit.rs#read; helper.rs, which config.rs also imports, has a private `secret`.
  const datas = new Map([
    ['lib', { declarations: [{ ...decl('edit.rs', 'read'), exported: true }, { ...decl('helper.rs', 'secret'), exported: false }], imports: [{ source: 'config.rs', target: 'edit.rs' }, { source: 'config.rs', target: 'helper.rs' }] }],
    ['app', { declarations: [decl('m.rs', 'main')], unresolvedCalls: [
      // A call whose result is chained is recorded as the whole chain; its first callee is the call at that line.
      { from: 'm.rs#main', file: 'm.rs', targetName: 'read().map_err().and_then', line: 3 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'read().map_err', line: 3 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'read', line: 9 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'secret', line: 5 },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'x.read().y', line: 6 },
    ].map((u) => ({ ...u, reason: 'not-declared-in-repo' })) }],
  ]);
  const cross = [{ source: 'app/m.rs', target: 'lib/config.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'lib' }];
  const rows = bindCalls({ chunks, datas, cross });
  assert.deepEqual(rows.map((e) => [e.target, e.lines, e.count]), [['lib/edit.rs#read', [3, 9], 2]]);
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
    ['app', { files: [{ ...node('main.rs'), roles: ['test'] }], imports: [], symbols: [], calls: [], externals: [{ id: 'pkg:lib', count: 1, files: ['main.rs'] }], unresolvedCalls: [{ from: 'main.rs#main', file: 'main.rs', targetName: 'json', line: 3, reason: 'external-package' }, { from: 'main.rs#main', file: 'main.rs', targetName: 'format', line: 4, reason: 'not-declared-in-repo' }, { from: 'main.rs#main', file: 'main.rs', targetName: 'run', line: 5, reason: 'external-package' }, { from: 'main.rs#go', file: 'main.rs', targetName: 'lib::run', line: 6, reason: 'external-package' }, { from: 'main.rs#go', file: 'main.rs', targetName: 'other::run', line: 7, reason: 'external-package' }], insights: { findings: {}, issues: [], cycles: { all: [] } } }],
  ]);
  const { crossCalls } = stitch({ root: '/r', chunks, datas, files: ['lib/m.rs', 'app/main.rs'] });
  assert.ok(crossCalls.every(c => c.test));
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

test('stitching preserves distinct import sites and a production witness on a mixed edge', () => {
  const chunks = [{id:'app', path:'app'}, {id:'lib', path:'lib'}];
  const sites = [4, 2].map(line => ({ file:'main.js', specifier:'../lib/api.js', line, test:line === 4 }));
  const datas = new Map([['app',{files:[node('main.js')], outsideRoot:sites}], ['lib',{files:[node('api.js')]}]]);
  const { whole } = stitch({root:'/r', chunks, datas, files:['app/main.js','lib/api.js']});
  const [edge] = whole.imports;
  assert.equal(whole.imports.length, 1);
  assert.deepEqual([edge.test, edge.count, edge.line, edge.lines.toSorted((a,b) => a-b)], [false, 2, 2, [2,4]]);
});

test('test-only cross-chunk imports keep a boundary hub eligible for LSP without raising its production rank', () => {
  const libraries = Array.from({ length: 12 }, (_, i) => `lib${i}.ts`);
  const checks = libraries.map((_,i) => `check${i}.ts`);
  const chunks = [{ id:'lib', path:'lib' }, { id:'app', path:'app' }];
  const datas = new Map([
    ['lib', { files:libraries.map(node), imports:libraries.slice(0,-1).map(target => ({ source:libraries[0], target })) }],
    ['app', { files:checks.map(id => ({ ...node(id), roles:['test'] })), outsideRoot:checks.map(file => ({ file, specifier:`../lib/${libraries.at(-1)}`, line:1 })) }],
  ]);
  const files = [...libraries.map(f => `lib/${f}`), ...checks.map(f => `app/${f}`)];
  const { whole } = stitch({ root:'/r', chunks, datas, files });
  const target = `lib/${libraries.at(-1)}`;
  assert.equal(whole.files.find(f => f.id === target).fanIn, 0);
  assert.ok(whole.hubFiles.includes(target));
});

test('an import that did not link binds through the caller\'s own module, a Type::member binds the member, the own file wins, two matches bind nothing', () => {
  const chunks = [{ id: 'base', path: 'base' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['base', { declarations: [
      { ...decl('result.rs', 'Continuation.related_input'), kind: 'method', exported: true },
      { ...decl('config.rs', 'read_private_config'), exported: true },
      { ...decl('cut.rs', 'cut'), exported: true },
      { ...decl('a.rs', 'dup'), exported: true }, { ...decl('b.rs', 'dup'), exported: true },
    ] }],
    ['app', {
      declarations: [decl('trust.rs', 'listed'), decl('cfg.rs', 'load'), decl('cuts.rs', 'cut'), decl('cuts.rs', 'probe')],
      // trust.rs reaches base through `crate::cfg`, a module of its own chunk that re-exports base's config.
      imports: [{ source: 'trust.rs', target: 'cfg.rs' }],
      unresolvedCalls: [
        { from: 'trust.rs#listed', file: 'trust.rs', targetName: 'Continuation::related_input', line: 515, reason: 'other' },
        { from: 'trust.rs#listed', file: 'trust.rs', targetName: 'read_private_config', line: 20, reason: 'other' },
        { from: 'cuts.rs#probe', file: 'cuts.rs', targetName: 'cut', line: 91, reason: 'other' },
        { from: 'trust.rs#listed', file: 'trust.rs', targetName: 'dup', line: 30, reason: 'ambiguous' },
        { from: 'trust.rs#listed', file: 'trust.rs', targetName: 'client.related_input', line: 31, reason: 'other' },
        // A name nothing imported (a macro such as `format!`) never binds through the caller's own modules.
        { from: 'trust.rs#listed', file: 'trust.rs', targetName: 'read_private_config', line: 40, reason: 'not-declared-in-repo' },
      ],
    }],
  ]);
  const cross = [
    { source: 'app/trust.rs', target: 'base/result.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'base' },
    { source: 'app/trust.rs', target: 'base/a.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'base' },
    { source: 'app/trust.rs', target: 'base/b.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'base' },
    { source: 'app/cfg.rs', target: 'base/config.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'base' },
    { source: 'app/cuts.rs', target: 'base/cut.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'base' },
  ];
  const rows = bindCalls({ chunks, datas, cross });
  assert.deepEqual(rows.map((e) => [e.source, e.target, e.lines, e.resolution[1]]).toSorted(), [
    ['app/trust.rs#listed', 'base/config.rs#read_private_config', [20], 'own-module'],
    ['app/trust.rs#listed', 'base/result.rs#Continuation.related_input', [515], 'import'],
  ]);
});

test('a workspace crate path binds its item or its Type::member', () => {
  const chunks = [{ id: 'base', path: 'base' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['base', { declarations: [{ ...decl('memo.rs', 'fresh'), exported: true }, { ...decl('memo.rs', 'Memo.new'), kind: 'method', exported: true }] }],
    ['app', { declarations: [decl('m.rs', 'main')], unresolvedCalls: [
      { from: 'm.rs#main', file: 'm.rs', targetName: 'base_io::memo::fresh', line: 3, reason: 'other' },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'base_io::memo::Memo::new', line: 4, reason: 'constructor-target-unavailable' },
    ] }],
  ]);
  const crateItem = (path) => path.startsWith('base_io::memo::') ? { file: 'base/memo.rs', rest: path.split('::').slice(2) } : null;
  const rows = bindCalls({ chunks, datas, cross: [], crateItem });
  assert.deepEqual(rows.map((e) => [e.target, e.resolution[1]]).toSorted(), [['base/memo.rs#Memo.new', 'crate-path'], ['base/memo.rs#fresh', 'crate-path']]);
});

test('a receiver call typed by another crate binds the method of that type; a chain row keeps its head only', () => {
  const chunks = [{ id: 'base', path: 'base' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['base', { declarations: [{ ...decl('client.rs', 'Client.open'), kind: 'method', exported: true }, { ...decl('client.rs', 'Client.close'), kind: 'method', exported: true }] }],
    ['app', { declarations: [decl('m.rs', 'main'), decl('m.rs', 'other')], unresolvedCalls: [
      // `client: &base_io::client::Client` then `client.open(..)`: the engine reports the receiver's type as written.
      { from: 'm.rs#main', file: 'm.rs', targetName: 'client.open', receiverType: 'base_io::client::Client', line: 3, reason: 'not-declared-in-repo' },
      // A type that does not declare the method binds nothing; an Arc of it names Arc, which declares nothing here.
      { from: 'm.rs#main', file: 'm.rs', targetName: 'client.flush', receiverType: 'base_io::client::Client', line: 4, reason: 'not-declared-in-repo' },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'shared.close', receiverType: 'Arc<base_io::client::Client>', line: 5, reason: 'not-declared-in-repo' },
      // A chain row's receiver type is its outer receiver's, never the head call's.
      { from: 'm.rs#other', file: 'm.rs', targetName: 'client.close().await.expect', receiverType: 'base_io::client::Client', line: 6, reason: 'not-declared-in-repo' },
    ] }],
  ]);
  const crateItem = (path) => path.startsWith('base_io::client::') ? { file: 'base/client.rs', rest: path.split('::').slice(2) } : null;
  const rows = bindCalls({ chunks, datas, cross: [], crateItem });
  assert.deepEqual(rows.map((e) => [e.source, e.target, e.lines, e.resolution[1]]), [['app/m.rs#main', 'base/client.rs#Client.open', [3], 'crate-path']]);
});

test('a Rust type\'s member declared in another file of its crate binds, as an impl block anywhere in the crate is that type\'s', () => {
  const chunks = [{ id: 'base', path: 'base' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    // client.rs declares the type; client/sync.rs holds `impl Client { pub fn open_doc }`; two trait impls give fmt twice.
    ['base', { declarations: [
      { ...decl('client.rs', 'Client'), kind: 'struct', exported: true },
      { ...decl('client/sync.rs', 'Client.open_doc'), kind: 'method', exported: true },
      { ...decl('client/show.rs', 'Client.fmt'), kind: 'method', exported: true },
      { ...decl('client/debug.rs', 'Client.fmt'), kind: 'method', exported: true },
    ] }],
    ['app', { declarations: [decl('m.rs', 'main')], unresolvedCalls: [
      { from: 'm.rs#main', file: 'm.rs', targetName: 'client.open_doc', receiverType: 'base_io::client::Client', line: 3, reason: 'not-declared-in-repo' },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'client.fmt', receiverType: 'base_io::client::Client', line: 4, reason: 'not-declared-in-repo' },
    ] }],
  ]);
  const crateItem = (path) => path.startsWith('base_io::client::') ? { file: 'base/client.rs', rest: path.split('::').slice(2) } : null;
  const rows = bindCalls({ chunks, datas, cross: [], crateItem });
  assert.deepEqual(rows.map((e) => [e.target, e.lines]), [['base/client/sync.rs#Client.open_doc', [3]]]);
});

test('an imported Rust type binds its member in another file of its crate; a type the import does not declare binds nothing', () => {
  const chunks = [{ id: 'base', path: 'base' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['base', { declarations: [
      { ...decl('client.rs', 'Client'), kind: 'struct', exported: true },
      { ...decl('client/sync.rs', 'Client.open_doc'), kind: 'method', exported: true },
      { ...decl('other.rs', 'Other.open_doc'), kind: 'method', exported: true },
    ] }],
    // `use base_io::client::Client;` then `client: &Client` and `client.open_doc(..)`.
    ['app', { declarations: [decl('m.rs', 'main')], unresolvedCalls: [
      { from: 'm.rs#main', file: 'm.rs', targetName: 'client.open_doc', receiverType: 'Client', line: 3, reason: 'not-declared-in-repo' },
      { from: 'm.rs#main', file: 'm.rs', targetName: 'other.open_doc', receiverType: 'Other', line: 4, reason: 'not-declared-in-repo' },
    ] }],
  ]);
  const cross = [{ source: 'app/m.rs', target: 'base/client.rs', via: 'crate-path', sourceChunk: 'app', targetChunk: 'base' }];
  const rows = bindCalls({ chunks, datas, cross });
  assert.deepEqual(rows.map((e) => [e.target, e.lines, e.resolution[1]]), [['base/client/sync.rs#Client.open_doc', [3], 'import']]);
});

test('imports between chunks keep the kind each chunk recorded, and names taken stay with their files', () => {
  const chunks = [{ id: 'lib', path: 'lib', package: '@x/lib' }, { id: 'app', path: 'app' }];
  const datas = new Map([
    ['lib', { files: [node('a.ts')], imports: [], symbols: [], calls: [], externals: [], uses: [{ source: 'a.ts', target: 'a.ts#f', line: 1, detail: 'import' }], insights: { findings: {}, issues: [], cycles: { all: [] } } }],
    ['app', {
      files: [node('m.ts'), node('n.ts')], imports: [], symbols: [], calls: [],
      externals: [{ id: 'pkg:@x/lib', count: 2, files: ['m.ts', 'n.ts'], kinds: { 'm.ts': ['external', 'type-import'], 'n.ts': ['external', 'dynamic-import'] } }],
      outsideRoot: [{ file: 'm.ts', line: 4, specifier: '../lib/a.js', importKind: 'type-import' }],
      uses: [{ source: 'm.ts', target: 'n.ts#g', line: 2, detail: 'import' }],
      insights: { findings: {}, issues: [], cycles: { all: [] } },
    }],
  ]);
  const { whole } = stitch({ root: '/r', chunks, datas, files: ['lib/a.ts', 'app/m.ts', 'app/n.ts'] });
  const kinds = Object.fromEntries(whole.imports.map((e) => [`${e.source}>${e.target}`, e.resolution.toSorted()]));
  assert.deepEqual(kinds['app/m.ts>lib/a.ts'], ['relative', 'type-import']);
  assert.deepEqual(kinds['app/m.ts>pkg:@x/lib'], ['external', 'package', 'type-import']);
  assert.deepEqual(kinds['app/n.ts>pkg:@x/lib'], ['dynamic-import', 'external', 'package']);
  assert.deepEqual(whole.uses.map((u) => `${u.source}>${u.target}`).toSorted(byText), ['app/m.ts>app/n.ts#g', 'lib/a.ts>lib/a.ts#f']);
});

test('a workspace package the engine reports as out of root stitches to that package, with its kind', () => {
  const chunks = [{ id: 'lib', path: 'lib', package: '@x/lib' }, { id: 'app', path: 'app' }];
  const empty = { imports: [], symbols: [], calls: [], externals: [], insights: { findings: {}, issues: [], cycles: { all: [] } } };
  const datas = new Map([
    ['lib', { ...empty, files: [node('a.ts')] }],
    ['app', { ...empty, files: [node('m.ts')], outsideRoot: [
      { file: 'm.ts', line: 1, specifier: '@x/lib/sub', importKind: 'type-import' },
      { file: 'm.ts', line: 2, specifier: '@x/elsewhere' },
    ] }],
  ]);
  const { cross, unresolved, whole } = stitch({ root: '/r', chunks, datas, files: ['lib/a.ts', 'app/m.ts'] });
  assert.deepEqual(cross.map((e) => `${e.source}>${e.target}:${e.via}`), ['app/m.ts>pkg:@x/lib:package']);
  assert.deepEqual(unresolved.map((u) => u.specifier), ['@x/elsewhere']);
  const edge = whole.imports.find((e) => e.source === 'app/m.ts');
  assert.deepEqual(edge.resolution.toSorted(), ['package', 'type-import']);
  assert.ok(whole.files.some((f) => f.id === 'pkg:@x/lib' && f.kind === 'package'));
});

test('imports stitched between chunks carry the load the engine recorded for their kind', () => {
  const chunks = [{ id: 'lib', path: 'lib', package: '@x/lib' }, { id: 'app', path: 'app' }];
  const empty = { imports: [], symbols: [], calls: [], externals: [], insights: { findings: {}, issues: [], cycles: { all: [] } } };
  const datas = new Map([
    ['lib', { ...empty, files: [node('a.ts')] }],
    ['app', {
      ...empty,
      files: [node('m.ts'), node('n.ts')],
      outsideRoot: [{ file: 'm.ts', line: 4, specifier: '../lib/a.js', importKind: 'future-import', load: 'type' }],
      externals: [{ id: 'pkg:@x/lib', count: 1, files: ['n.ts'], kinds: { 'n.ts': ['external', 'later-import'] }, loads: { 'later-import': 'deferred' } }],
    }],
  ]);
  const { whole } = stitch({ root: '/r', chunks, datas, files: ['lib/a.ts', 'app/m.ts', 'app/n.ts'] });
  const edge = (s) => whole.imports.find((e) => e.source === s);
  assert.deepEqual(edge('app/m.ts').loads, { 'future-import': 'type' });
  assert.deepEqual(edge('app/n.ts').loads, { 'later-import': 'deferred' });
});
