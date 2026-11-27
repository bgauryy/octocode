// Bundler output beside the source graph: an esbuild metafile (a real build with code splitting), a rollup/rolldown
// output bundle and rolldown's analyze-data.json enter through `enrich`; `bundle` lists outputs, an output's bytes by
// file and package, and `--diff` compares what shipped with the source graph's runtime reach from the entry.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { buildSync } from 'esbuild';
import { tempDir } from './test-tmp.mjs';
import { enrichExport } from '../src/enrich.ts';
import { formatResult, runQuery } from '../src/runtime/query.ts';

const SOURCES = {
  'src/worker.ts': `import type { Row } from './lib/types.ts';
import { parseRows } from './lib/parse.ts';
import { formatTotal } from './lib/format.ts';
export const later = () => import('./lib/lazy.ts');
export function run(text: string): string {
  const rows: Row[] = parseRows(text);
  return formatTotal(rows.length);
}
`,
  'src/lib/types.ts': 'export interface Row { key: string }\n',
  'src/lib/parse.ts': `import { EXTRA } from './extra.ts';
export function parseRows(text: string) {
  return text.split(EXTRA).map(key => ({ key }));
}
`,
  'src/lib/extra.ts': "export const EXTRA = '\\n';\n",
  'src/lib/format.ts': `import { pad } from 'tiny-pad';
import { neverCalled } from './unused.ts';
export function formatTotal(total: number) {
  return pad(String(total), 12);
}
export const debug = () => neverCalled();
`,
  'src/lib/unused.ts': "export function neverCalled() {\n  return 'shaken';\n}\n",
  'src/lib/lazy.ts': "export function later() {\n  return 'loaded on demand';\n}\n",
  'node_modules/tiny-pad/package.json': '{"name":"tiny-pad","main":"index.js"}',
  'node_modules/tiny-pad/index.js':
    "import { space } from 'pad-core';\nexport function pad(t, w) { while (t.length < w) t = space + t; return t; }\n",
  'node_modules/pad-core/package.json': '{"name":"pad-core","main":"index.js"}',
  'node_modules/pad-core/index.js': "export const space = ' ';\n",
};

const sha = text => createHash('sha256').update(text).digest('hex');
const imp = (source, target, kind, line = 1) => ({
  source,
  target,
  line,
  lines: [line],
  count: 1,
  confidence: 'high',
  resolution: [kind],
  test: false,
});

// The project, its esbuild build (metafile) and a hand-made source graph that misses parse.ts → extra.ts, the way a
// graph misses an edge it could not resolve.
function project() {
  const dir = realpathSync(tempDir('graph-bundle-'));
  for (const [path, text] of Object.entries(SOURCES)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  }
  const { metafile } = buildSync({
    absWorkingDir: dir,
    entryPoints: ['src/worker.ts'],
    bundle: true,
    splitting: true,
    format: 'esm',
    outdir: 'dist',
    metafile: true,
    write: false,
    logLevel: 'silent',
  });
  const src = Object.keys(SOURCES).filter(p => p.startsWith('src/'));
  const D = {
    section: dir,
    files: src.map(id => ({
      id,
      label: id.split('/').pop(),
      chunk: 'root',
      check: { status: 'ok', bytes: Buffer.byteLength(SOURCES[id]), problems: [] },
    })),
    chunks: [{ id: 'root', path: '.', files: src.length, package: 'fixture', built: true }],
    imports: [
      imp('src/worker.ts', 'src/lib/types.ts', 'type-import', 1),
      imp('src/worker.ts', 'src/lib/parse.ts', 'static-import', 2),
      imp('src/worker.ts', 'src/lib/format.ts', 'static-import', 3),
      imp('src/worker.ts', 'src/lib/lazy.ts', 'dynamic-import', 4),
      imp('src/lib/format.ts', 'src/lib/unused.ts', 'static-import', 2),
    ],
    externals: [
      { id: 'pkg:tiny-pad', count: 1, files: ['src/lib/format.ts'], kinds: { 'src/lib/format.ts': ['static-import'] } },
    ],
    symbols: [],
    calls: [],
  };
  return { dir, metafile, D };
}

// Writes the export and one provider artifact, enriches, and returns the enriched graph.
async function enrich(dir, D, format, artifact, extra = {}) {
  const base = JSON.stringify(D);
  const text = JSON.stringify(artifact);
  writeFileSync(join(dir, 'data.json'), base);
  writeFileSync(join(dir, 'artifact.json'), text);
  const manifest = {
    revision: 'r1',
    baseSha256: sha(base),
    providers: [{ format, file: 'artifact.json', sha256: sha(text), revision: 'r1', ...extra }],
  };
  writeFileSync(join(dir, 'manifest.json'), JSON.stringify(manifest));
  const out = join(dir, `enriched-${format}.json`);
  await enrichExport(join(dir, 'data.json'), join(dir, 'manifest.json'), out);
  return JSON.parse(readFileSync(out, 'utf8'));
}

test('esbuild metafile: every output with its bytes and entry, heaviest first', async () => {
  const { dir, metafile, D } = project();
  const E = await enrich(dir, D, 'esbuild-metafile', metafile);
  const r = runQuery(E, 'bundle');
  assert.equal(r.ok, true, r.error);
  const outputs = Object.entries(metafile.outputs);
  assert.deepEqual(
    r.rows.map(row => [row.id, row.bytes, row.entry]),
    outputs
      .map(([id, o]) => [id, o.bytes, o.entryPoint ?? null])
      .sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
  );
  assert.equal(r.rows.find(row => row.id === 'dist/worker.js').role, 'entry');
  // The entries light up in the graph.
  assert.deepEqual(r.nodes, r.rows.map(row => row.entry).filter(Boolean));
  assert.ok(r.nodes.includes('src/worker.ts'));
});

test('bundle <output>: bytes by source file and by package, the bundler overhead stated, nothing left out', async () => {
  const { dir, metafile, D } = project();
  const E = await enrich(dir, D, 'esbuild-metafile', metafile);
  const r = runQuery(E, 'bundle worker.js');
  assert.equal(r.ok, true, r.error);
  const inputs = Object.entries(metafile.outputs['dist/worker.js'].inputs);
  const files = r.rows.filter(row => row.role === 'file');
  assert.deepEqual(
    files.map(row => [row.id, row.bytes]),
    inputs
      .map(([id, x]) => [id, x.bytesInOutput])
      .sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
  );
  const packages = Object.fromEntries(r.rows.filter(row => row.group === 'package').map(row => [row.id, row.bytes]));
  const sum = prefix => inputs.filter(([id]) => id.startsWith(prefix)).reduce((a, [, x]) => a + x.bytesInOutput, 0);
  assert.deepEqual(packages, {
    'pkg:fixture': sum('src/'),
    'pkg:tiny-pad': sum('node_modules/tiny-pad/'),
    'pkg:pad-core': sum('node_modules/pad-core/'),
  });
  const total = inputs.reduce((a, [, x]) => a + x.bytesInOutput, 0);
  assert.equal(r.stats.bytes, metafile.outputs['dist/worker.js'].bytes);
  assert.equal(r.stats.overhead, metafile.outputs['dist/worker.js'].bytes - total);
  const text = formatResult(r);
  assert.match(text, /src\/lib\/parse\.ts {2}file {2}#\d+ {2}\d+ B/);
  assert.match(text, /pkg:tiny-pad {2}1 row {2}\d+ B/);
});

test('bundle --diff: dropped and unexpected files against the runtime reach of the entry, each with bytes', async () => {
  const { dir, metafile, D } = project();
  const E = await enrich(dir, D, 'esbuild-metafile', metafile);
  const r = runQuery(E, 'bundle dist/worker.js --diff');
  assert.equal(r.ok, true, r.error);
  const by = Object.fromEntries(r.rows.map(row => [row.id, row]));
  // Reached at run time, parsed by esbuild, nothing kept: tree-shaken, with its source size.
  assert.equal(by['src/lib/unused.ts'].role, 'tree-shaken');
  assert.equal(by['src/lib/unused.ts'].bytes, Buffer.byteLength(SOURCES['src/lib/unused.ts']));
  // A type-only import never runs: not in the runtime reach, so not a difference.
  assert.equal(by['src/lib/types.ts'], undefined);
  // Reached behind import() and shipped in the chunk worker.js loads dynamically: no difference either.
  assert.equal(by['src/lib/lazy.ts'], undefined);
  assert.equal(r.stats.dynamicFiles, 1);
  // Shipped, but the source graph does not reach it from the entry: the edge the graph missed, with the importer.
  assert.equal(by['src/lib/extra.ts'].role, 'unexpected');
  assert.deepEqual(by['src/lib/extra.ts'].importedBy, ['src/lib/parse.ts']);
  assert.equal(by['src/lib/extra.ts'].bytes, metafile.outputs['dist/worker.js'].inputs['src/lib/extra.ts'].bytesInOutput);
  // A package reached files import is expected; a package only another package pulls in is not.
  assert.equal(by['node_modules/tiny-pad/index.js'].role, 'package');
  assert.equal(by['node_modules/pad-core/index.js'].role, 'unexpected');
  assert.deepEqual(by['node_modules/pad-core/index.js'].importedBy, ['node_modules/tiny-pad/index.js']);
  assert.deepEqual(Object.keys(by).sort(), [
    'node_modules/pad-core/index.js',
    'node_modules/tiny-pad/index.js',
    'src/lib/extra.ts',
    'src/lib/unused.ts',
  ]);
  assert.equal(r.stats.reach.query, 'reach src/worker.ts --runtime');
  assert.match(formatResult(r), /src\/lib\/extra\.ts {2}unexpected {2}\d+ B {2}via src\/lib\/parse\.ts/);
});

test('bundle answers without bundle evidence, or for an unknown output, say so with candidates', async () => {
  const { dir, metafile, D } = project();
  assert.match(runQuery(D, 'bundle').error, /no bundle evidence.*esbuild-metafile/);
  const E = await enrich(dir, D, 'esbuild-metafile', metafile);
  const none = runQuery(E, 'bundle nope.js');
  assert.equal(none.ok, false);
  assert.match(none.error, /no output/);
  const chunk = Object.keys(metafile.outputs).find(id => !metafile.outputs[id].entryPoint);
  if (chunk) assert.match(runQuery(E, `bundle ${chunk} --diff`).error, /no entry point/);
});

test('rollup or rolldown output bundle: absolute module ids under the root, rendered lengths per output', async () => {
  const { dir, D } = project();
  const bundle = {
    'worker.js': {
      type: 'chunk',
      fileName: 'worker.js',
      isEntry: true,
      isDynamicEntry: false,
      facadeModuleId: join(dir, 'src/worker.ts'),
      imports: ['lazy-1.js', 'node:fs'],
      dynamicImports: [],
      code: 'x'.repeat(500),
      modules: {
        [join(dir, 'src/worker.ts')]: { renderedLength: 200, originalLength: 260 },
        [join(dir, 'src/lib/parse.ts')]: { renderedLength: 120, originalLength: 130 },
        [join(dir, 'node_modules/tiny-pad/index.js')]: { renderedLength: 80, originalLength: 90 },
        '\0commonjsHelpers.js': { renderedLength: 40, originalLength: 40 },
      },
    },
    'lazy-1.js': {
      type: 'chunk',
      fileName: 'lazy-1.js',
      isEntry: false,
      isDynamicEntry: false,
      facadeModuleId: null,
      imports: [],
      dynamicImports: [],
      code: 'y'.repeat(30),
      modules: { [join(dir, 'src/lib/format.ts')]: { renderedLength: 30, originalLength: 50 } },
    },
    'style.css': { type: 'asset', fileName: 'style.css', source: 'body{}' },
  };
  const E = await enrich(dir, D, 'rollup-bundle', bundle);
  const list = runQuery(E, 'bundle');
  assert.deepEqual(
    list.rows.map(row => [row.id, row.bytes, row.entry, row.role]),
    [
      ['worker.js', 500, 'src/worker.ts', 'entry'],
      ['lazy-1.js', 30, null, 'chunk'],
      ['style.css', 6, null, 'asset'],
    ]
  );
  const r = runQuery(E, 'bundle worker.js');
  assert.deepEqual(
    r.rows.filter(row => row.role === 'file').map(row => [row.id, row.bytes]),
    [
      ['src/worker.ts', 200],
      ['src/lib/parse.ts', 120],
      ['node_modules/tiny-pad/index.js', 80],
      ['\\0commonjsHelpers.js', 40],
    ]
  );
  assert.equal(r.rows.find(row => row.id === '(virtual)')?.group, 'package');
  assert.deepEqual(r.stats.externals, ['node:fs']);
  // The chunk worker.js loads statically ships with it: format.ts is no unexpected inclusion and no drop.
  const diff = runQuery(E, 'bundle worker.js --diff');
  assert.equal(diff.rows.find(row => row.id === 'src/lib/format.ts'), undefined);
  assert.equal(diff.rows.find(row => row.id === 'src/lib/unused.ts').role, 'not shipped');
  assert.match(runQuery(E, 'bundle lazy-1.js --diff').error, /no entry point/);
});

test("rolldown analyze-data.json: chunks and modules relative to rolldown's directory; module sizes say what they measure", async () => {
  const { dir, D } = project();
  const analysis = {
    meta: { bundler: 'rolldown', version: '1.0.0' },
    chunks: [
      { id: 'chunk-worker', name: 'worker.js', size: 900, type: 'static-entry', moduleIndices: [0, 1], entryModule: 0, imports: [{ targetChunkIndex: 1, type: 'dynamic' }] },
      { id: 'chunk-lazy', name: 'lazy.js', size: 100, type: 'dynamic-entry', moduleIndices: [2], entryModule: 2 },
    ],
    modules: [
      { id: 'mod-0', path: 'src/worker.ts', size: 300 },
      { id: 'mod-1', path: 'src/lib/parse.ts', size: 500, importers: [0] },
      { id: 'mod-2', path: 'src/lib/lazy.ts', size: 70, importers: [0] },
    ],
  };
  const E = await enrich(dir, D, 'rolldown-analyze', analysis);
  const list = runQuery(E, 'bundle');
  assert.deepEqual(
    list.rows.map(row => [row.id, row.bytes, row.entry, row.role]),
    [
      ['worker.js', 900, 'src/worker.ts', 'entry'],
      ['lazy.js', 100, 'src/lib/lazy.ts', 'dynamic-entry'],
    ]
  );
  const r = runQuery(E, 'bundle worker.js');
  assert.equal(r.stats.measure, 'module');
  assert.match(r.summary, /module size before rendering/);
  assert.deepEqual(
    r.rows.filter(row => row.role === 'file').map(row => [row.id, row.bytes]),
    [
      ['src/lib/parse.ts', 500],
      ['src/worker.ts', 300],
    ]
  );
  // Without bytes in output, the bundler overhead cannot be stated: null, never a number built from other units.
  assert.equal(r.stats.overhead, null);
});

test('a provider whose bytes do not match its digest is refused', async () => {
  const { dir, metafile, D } = project();
  writeFileSync(join(dir, 'data.json'), JSON.stringify(D));
  writeFileSync(join(dir, 'artifact.json'), JSON.stringify(metafile));
  writeFileSync(
    join(dir, 'manifest.json'),
    JSON.stringify({
      revision: 'r1',
      baseSha256: sha(JSON.stringify(D)),
      providers: [{ format: 'esbuild-metafile', file: 'artifact.json', sha256: sha('other'), revision: 'r1' }],
    })
  );
  await assert.rejects(
    enrichExport(join(dir, 'data.json'), join(dir, 'manifest.json'), join(dir, 'out.json')),
    /digest mismatch/
  );
});
