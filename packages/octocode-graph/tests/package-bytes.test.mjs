// F7 and V7: an external package's bytes come from a bundle when the build finds one, else from its installed
// closure, with the method and its evidence stated; a package nothing measures keeps null bytes and says why.
import assert from 'node:assert/strict';
import { mkdirSync, readdirSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import {
  discoverBundles,
  installedClosure,
  measurePackages,
  runtimeSpecifiers,
} from '../src/runtime/package-closure.ts';
import { packageBytes } from '../src/runtime/package-bytes.ts';
import { bundleFormat, readWebpackStats } from '../src/runtime/bundle-ingest.ts';

const FILES = {
  'package.json': '{"name":"app","private":true}\n',
  'src/worker.js': "import { greet } from 'tiny-a';\nconsole.log(greet('x'));\n",
  'node_modules/tiny-a/package.json': JSON.stringify({ name: 'tiny-a', version: '1.2.0', exports: { '.': { import: './esm/index.mjs', require: './cjs/index.cjs' }, './package.json': './package.json' } }),
  'node_modules/tiny-a/esm/index.mjs': [
    "import { helper } from './helper.mjs';",
    "import b from 'tiny-b';",
    "export * from './more.mjs';",
    '// import gone from "commented-out";',
    'export function greet(name) {',
    '  return helper(name) + b.size;',
    '}',
    "export const later = () => import('tiny-optional');",
    '',
  ].join('\n'),
  'node_modules/tiny-a/esm/helper.mjs': "export function helper(n) {\n  return `hi ${n}`;\n}\n",
  'node_modules/tiny-a/esm/more.mjs': 'export const more = 1;\n',
  'node_modules/tiny-a/cjs/index.cjs': "module.exports = { greet: n => 'hi ' + n };\n",
  'node_modules/tiny-b/package.json': JSON.stringify({ name: 'tiny-b', version: '0.3.1', main: 'lib/main' }),
  'node_modules/tiny-b/lib/main.js': "const x = require('./x');\nrequire('node:fs');\n/* require('also-commented') */\nmodule.exports = { size: x.length, label: 'require(\"in-a-string\")' };\n",
  'node_modules/tiny-b/lib/x.js': "module.exports = 'abcdef';\n",
};

function project(extra = {}) {
  const root = realpathSync(tempDir('graph-bytes-'));
  for (const [name, text] of Object.entries({ ...FILES, ...extra })) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), text);
  }
  return root;
}
const size = (root, ...files) => files.reduce((a, f) => a + statSync(join(root, f)).size, 0);

test('runtime specifiers: static, export-from, import() and require; type imports and comments load nothing', () => {
  const typed = "import type { Shape } from 'types-only';\nexport type { Other } from 'types-too';\nimport typed from 'typed-default';\n";
  assert.deepEqual(runtimeSpecifiers(typed).map(s => s.specifier), ['typed-default']);
  const specs = runtimeSpecifiers(FILES['node_modules/tiny-a/esm/index.mjs']).map(s => `${s.kind} ${s.specifier}`);
  assert.deepEqual(specs.toSorted(), ['import ./helper.mjs', 'import ./more.mjs', 'import tiny-b', 'import tiny-optional'].toSorted());
  assert.deepEqual(runtimeSpecifiers(FILES['node_modules/tiny-b/lib/main.js']).map(s => s.specifier), ['./x', 'node:fs', 'in-a-string']);
});

test('installed closure: the entry Node resolves for the load, every file it loads across packages, bytes summed', () => {
  const root = project();
  const home = tempDir('graph-bytes-home-');
  const c = installedClosure('tiny-a', join(root, 'src'), 'import', { home });
  assert.equal(typeof c, 'object', c);
  assert.equal(c.entry, join(root, 'node_modules/tiny-a/esm/index.mjs'));
  const loaded = ['node_modules/tiny-a/esm/index.mjs', 'node_modules/tiny-a/esm/helper.mjs', 'node_modules/tiny-a/esm/more.mjs', 'node_modules/tiny-b/lib/main.js', 'node_modules/tiny-b/lib/x.js'];
  assert.equal(c.files, loaded.length);
  assert.equal(c.bytes, size(root, ...loaded));
  assert.deepEqual(c.packages.toSorted(), ['tiny-a@1.2.0', 'tiny-b@0.3.1']);
  // What it could not resolve is listed with the file naming it: never guessed, never dropped.
  assert.deepEqual(c.unresolved.map(u => u.specifier).toSorted(), ['in-a-string', 'tiny-optional']);
  // A require loads the CommonJS entry.
  const r = installedClosure('tiny-a', join(root, 'src'), 'require', { home });
  assert.equal(r.entry, join(root, 'node_modules/tiny-a/cjs/index.cjs'));
  assert.equal(r.files, 1);
  // Cached by name@version, and checked against what it walked: a reinstalled dependency measures again.
  assert.ok(readdirSync(join(home, 'derived', 'package-closure')).length > 0);
  writeFileSync(join(root, 'node_modules/tiny-b/lib/x.js'), "module.exports = 'a much longer string than before';\n");
  assert.equal(installedClosure('tiny-a', join(root, 'src'), 'import', { home }).bytes, c.bytes, 'same versions: the cached closure answers');
  writeFileSync(join(root, 'node_modules/tiny-b/package.json'), JSON.stringify({ name: 'tiny-b', version: '0.4.0', main: 'lib/main' }));
  assert.equal(installedClosure('tiny-a', join(root, 'src'), 'import', { home }).bytes, size(root, ...loaded), 'a new tiny-b version: measured again');
  assert.match(installedClosure('missing-pkg', join(root, 'src'), 'import', { home }), /not installed: no node_modules\/missing-pkg/);
});

const graph = externals => ({
  files: [{ id: 'src/worker.js', language: 'javascript' }, { id: 'tool.py', language: 'python' }],
  externals,
});

test('measurePackages: installed closure, builtin and missing packages each state their method or reason', () => {
  const root = project();
  const D = graph([
    { id: 'pkg:tiny-a', count: 1, files: ['src/worker.js'], kinds: { 'src/worker.js': ['static-import'] } },
    { id: 'pkg:node:fs', count: 1, files: ['src/worker.js'], kinds: { 'src/worker.js': ['external-builtin'] } },
    { id: 'pkg:not-there', count: 1, files: ['src/worker.js'], kinds: { 'src/worker.js': ['static-import'] } },
    { id: 'pkg:requests', count: 1, files: ['tool.py'], kinds: { 'tool.py': ['python-import'] } },
  ]);
  D.externals = measurePackages(D, { root, bases: [], cache: false });
  const a = packageBytes(D, 'pkg:tiny-a');
  assert.equal(a.bytesMethod, 'installed-closure');
  assert.equal(a.bytes, installedClosure('tiny-a', join(root, 'src'), 'import', { cache: false }).bytes);
  assert.deepEqual([a.bytesEvidence.source, a.bytesEvidence.version, a.bytesEvidence.files], ['node_modules/tiny-a/esm/index.mjs', '1.2.0', 5]);
  assert.deepEqual(a.bytesEvidence.unresolved.map(u => u.from).toSorted(), ['node_modules/tiny-a/esm/index.mjs', 'node_modules/tiny-b/lib/main.js']);
  for (const id of ['pkg:node:fs', 'pkg:not-there', 'pkg:requests']) {
    const p = packageBytes(D, id);
    assert.equal(p.bytes, null, id);
    assert.equal(p.bytesMethod, null, id);
    assert.ok(p.bytesEvidence.reason, id);
  }
  assert.match(packageBytes(D, 'pkg:node:fs').bytesEvidence.reason, /builtin/);
  assert.match(packageBytes(D, 'pkg:not-there').bytesEvidence.reason, /not installed/);
  assert.match(packageBytes(D, 'pkg:requests').bytesEvidence.reason, /python/);
  assert.equal(packageBytes(D, 'pkg:other'), null);
  assert.match(packageBytes({ externals: [{ id: 'pkg:x' }] }, 'pkg:x').bytesEvidence.reason, /not measured/);
});

test('V7: an esbuild metafile found under dist/ gives bundle bytes; the installed closure is within 2x of them', async () => {
  const { build } = await import('esbuild');
  const root = project();
  const closure = installedClosure('tiny-a', join(root, 'src'), 'import', { cache: false });
  const result = await build({ absWorkingDir: root, entryPoints: ['src/worker.js'], bundle: true, platform: 'node', format: 'esm', outdir: 'dist',
    external: ['tiny-optional'], metafile: true, write: true, logLevel: 'silent' });
  writeFileSync(join(root, 'dist', 'meta.json'), JSON.stringify(result.metafile));
  assert.deepEqual(discoverBundles([root]).map(f => [f.file.slice(root.length + 1), f.format]), [['dist/meta.json', 'esbuild-metafile']]);
  const D = graph([{ id: 'pkg:tiny-a', count: 1, files: ['src/worker.js'], kinds: { 'src/worker.js': ['static-import'] } }]);
  D.externals = measurePackages(D, { root, bases: [], cache: false });
  const a = packageBytes(D, 'pkg:tiny-a');
  assert.equal(a.bytesMethod, 'bundle');
  assert.deepEqual([a.bytesEvidence.source, a.bytesEvidence.measure], ['dist/meta.json', 'output']);
  const inBundle = Object.entries(result.metafile.outputs['dist/worker.js'].inputs).filter(([p]) => p.startsWith('node_modules/tiny-a/')).reduce((s, [, x]) => s + x.bytesInOutput, 0);
  assert.equal(a.bytes, inBundle, 'the bundle bytes of tiny-a are its inputs in the output');
  // The closure counts tiny-b too (what loading tiny-a costs); the bundle attributes tiny-b to tiny-b.
  const both = Object.entries(result.metafile.outputs['dist/worker.js'].inputs).filter(([p]) => p.startsWith('node_modules/')).reduce((s, [, x]) => s + x.bytesInOutput, 0);
  const ratio = closure.bytes / both;
  assert.ok(ratio >= 0.5 && ratio <= 2, `installed closure ${closure.bytes} B vs bundle ${both} B: ratio ${ratio.toFixed(2)}`);
});

test('webpack stats: modules by chunk become bundle relations, and the format is recognized by shape', () => {
  const stats = {
    assets: [{ name: 'main.js', size: 900 }, { name: '1.js', size: 120 }],
    entrypoints: { main: { chunks: [0] } },
    chunks: [
      { id: 0, entry: true, initial: true, files: ['main.js'], children: [1], origins: [{ request: './src/index.js' }] },
      { id: 1, entry: false, initial: false, files: ['1.js'], children: [] },
    ],
    modules: [
      { identifier: '/repo/src/index.js', name: './src/index.js', size: 200, chunks: [0], reasons: [] },
      { identifier: '/repo/node_modules/lib/a.js', name: './node_modules/lib/a.js + 1 modules', size: 500, chunks: [0], modules: [
        { name: './node_modules/lib/a.js', size: 300, reasons: [{ moduleName: './src/index.js', type: 'harmony import specifier' }] },
        { name: './node_modules/lib/b.js', size: 200, reasons: [] },
      ] },
      { identifier: '/repo/src/lazy.js', name: './src/lazy.js', size: 100, chunks: [1], reasons: [{ moduleName: './src/index.js', type: 'import()' }] },
      { identifier: 'external "react"', name: 'external "react"', size: 42, chunks: [0] },
    ],
  };
  assert.equal(bundleFormat(stats), 'webpack-stats');
  assert.equal(bundleFormat({ inputs: {}, outputs: {} }), 'esbuild-metafile');
  assert.equal(bundleFormat({ modules: [], chunks: [{ moduleIndices: [] }] }), 'rolldown-analyze');
  assert.equal(bundleFormat({ 'a.js': { fileName: 'a.js', type: 'chunk' } }), 'rollup-bundle');
  assert.equal(bundleFormat({ name: 'app', version: '1.0.0' }), null);
  const { relations, record } = readWebpackStats(stats, { artifact: 'stats.json', sourceRoot: '/repo' });
  const of = kind => relations.filter(r => r.relation === kind);
  assert.deepEqual(of('OUTPUTS').map(r => [r.target, r.details.bytes, r.details.entry, r.details.kind, r.details.measure]), [
    ['bundle:main.js', 900, 'src/index.js', 'entry', 'module'],
    ['bundle:1.js', 120, null, 'dynamic-entry', 'module'],
  ]);
  assert.deepEqual(of('BUNDLES').map(r => [r.source, r.target, r.details.bytes]), [
    ['bundle:main.js', 'src/index.js', 200],
    ['bundle:main.js', 'node_modules/lib/a.js', 300],
    ['bundle:main.js', 'node_modules/lib/b.js', 200],
    ['bundle:1.js', 'src/lazy.js', 100],
  ]);
  assert.deepEqual(of('LOADS').map(r => [r.source, r.target, r.details.kind]), [['bundle:main.js', 'bundle:1.js', 'dynamic']]);
  assert.deepEqual(of('EXTERNAL').map(r => [r.source, r.target]), [['bundle:main.js', 'external:react']]);
  assert.deepEqual(of('BUNDLE_IMPORTS').map(r => [r.source, r.target, r.details.kind]), [
    ['src/index.js', 'node_modules/lib/a.js', 'harmony import specifier'],
    ['src/index.js', 'src/lazy.js', 'import()'],
  ]);
  assert.deepEqual(record, { measure: 'module', outputs: 2 });
});
