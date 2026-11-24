// CPU and heap profiles beside the source graph: `enrich` maps each frame to a graph file and its enclosing symbol
// (through the source map for a bundled frame) and keeps every cost it cannot map, by reason; `hot` ranks files or
// functions by self cost with their total, optionally inside the runtime reach of a file.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { buildSync } from 'esbuild';
import { tempDir } from './test-tmp.mjs';
import { enrichExport } from '../src/enrich.ts';
import { formatResult, runQuery } from '../src/runtime/query.ts';

const LIB = `export function checksum(rows) {
  let sum = 0;
  rows.forEach(row => { sum = (sum * 31 + row) % 1000003; });
  return sum;
}
export function other(rows) {
  return rows.concat('other');
}
`;
const MAIN = `import { checksum, other } from './lib.js';

export function run() {
  return [checksum([1, 2, 3]), other([])];
}
`;
const sha = text => createHash('sha256').update(text).digest('hex');

// The project (lib.js, main.js and an esbuild bundle of them with a linked map) and its hand-made graph.
function project() {
  const dir = realpathSync(tempDir('graph-profile-'));
  const files = { 'src/lib.js': LIB, 'src/main.js': MAIN, 'scripts/tool.js': 'tool();\n' };
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  }
  const built = buildSync({
    entryPoints: [join(dir, 'src/main.js')],
    bundle: true,
    format: 'esm',
    sourcemap: 'linked',
    outfile: join(dir, 'dist/out.js'),
    write: false,
    logLevel: 'silent',
  });
  mkdirSync(join(dir, 'dist'), { recursive: true });
  for (const f of built.outputFiles) writeFileSync(f.path, f.contents);
  const sym = (file, name, line, endLine) => ({ id: `${file}#${name}`, file, name, kind: 'function', line, endLine, exported: true });
  const D = {
    section: dir,
    files: ['src/lib.js', 'src/main.js'].map(id => ({ id, label: id.split('/').pop(), chunk: 'root' })),
    chunks: [{ id: 'root', path: '.', files: 2, package: 'fixture', built: true }],
    imports: [{ source: 'src/main.js', target: 'src/lib.js', line: 1, lines: [1], count: 1, confidence: 'high', resolution: ['static-import'], test: false }],
    symbols: [sym('src/lib.js', 'checksum', 1, 5), sym('src/lib.js', 'other', 6, 8), sym('src/main.js', 'run', 3, 5)],
    calls: [],
  };
  return { dir, D };
}

// The 0-based position V8 gives a function: its parameter list.
function v8Position(text, header) {
  const at = text.indexOf(header) + header.indexOf('(');
  const before = text.slice(0, at).split('\n');
  return { lineNumber: before.length - 1, columnNumber: before.at(-1).length };
}

const frame = (functionName, url = '', lineNumber = -1, columnNumber = -1) => ({ functionName, scriptId: '0', url, lineNumber, columnNumber });

// Self time per node, in µs: checksum 200 (+50 in a closure inside it, +25 recursive), run 35, other 70 through the
// bundle's map, lib.js's module scope 7; then what maps to no graph file: the bundle's module scope 3 (code of many
// sources), tiny-pad 30, a native 20, node internals 10, a script outside the graph 15, a file outside the root 5,
// (program) 40, (garbage collector) 60, (idle) 500.
function cpuProfile(dir) {
  const url = p => pathToFileURL(join(dir, p)).href;
  const out = readFileSync(join(dir, 'dist/out.js'), 'utf8');
  const other = v8Position(out, 'function other(');
  const nodes = [
    [1, frame('(root)'), [2, 7, 8, 9, 13, 16, 17]],
    [2, frame('run', url('src/main.js'), 2, 19), [3, 6, 12, 14, 15]],
    [3, frame('checksum', url('src/lib.js'), 0, 24), [4, 11]],
    [4, frame('', url('src/lib.js'), 2, 15), [5]],
    [5, frame('pad', url('node_modules/tiny-pad/index.js'), 0, 15), []],
    [6, frame('parse'), []],
    [7, frame('(program)'), []],
    [8, frame('(garbage collector)'), []],
    [9, frame('(idle)'), []],
    [11, frame('checksum', url('src/lib.js'), 0, 24), []],
    [12, frame('other', url('dist/out.js'), other.lineNumber, other.columnNumber), []],
    [13, frame('runMain', 'node:internal/modules/run_main', 10, 2), []],
    [14, frame('tool', url('scripts/tool.js'), 0, 4), []],
    [15, frame('far', 'file:///elsewhere/x.js', 0, 0), []],
    // V8 names a script's module scope '' at 0:0: no function of the file, and in a bundle no single source.
    [16, frame('', url('src/lib.js'), 0, 0), []],
    [17, frame('', url('dist/out.js'), 0, 0), []],
  ].map(([id, callFrame, children]) => ({ id, callFrame, hitCount: 0, children }));
  const samples = [3, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 2];
  const durations = [100, 100, 50, 30, 20, 40, 60, 500, 25, 70, 10, 15, 5, 7, 3, 35];
  // timeDeltas[i] is the gap before sample i, so sample i lasts timeDeltas[i + 1]; the last one lasts until endTime.
  const timeDeltas = [1, ...durations.slice(0, -1)];
  const startTime = 1000;
  const endTime = startTime + timeDeltas.reduce((a, b) => a + b, 0) + durations.at(-1);
  return { nodes, startTime, endTime, samples, timeDeltas };
}

async function enrich(dir, D, providers) {
  const base = JSON.stringify(D);
  writeFileSync(join(dir, 'data.json'), base);
  const entries = providers.map(([format, name, value]) => {
    const text = JSON.stringify(value);
    writeFileSync(join(dir, name), text);
    return { format, file: name, sha256: sha(text), revision: 'r1' };
  });
  writeFileSync(join(dir, 'manifest.json'), JSON.stringify({ revision: 'r1', baseSha256: sha(base), providers: entries }));
  const out = join(dir, `enriched-${providers.map(p => p[1]).join('-')}.json`);
  await enrichExport(join(dir, 'data.json'), join(dir, 'manifest.json'), out);
  return JSON.parse(readFileSync(out, 'utf8'));
}

test('cpuprofile: self and total per file and per enclosing symbol, recursion counted once, bundled frames mapped', async () => {
  const { dir, D } = project();
  const E = await enrich(dir, D, [['cpuprofile', 'cpu.cpuprofile', cpuProfile(dir)]]);
  const files = runQuery(E, 'hot files');
  assert.equal(files.ok, true, files.error);
  assert.deepEqual(
    files.rows.map(r => [r.id, r.loc.line, r.self, r.total]),
    [
      ['src/lib.js', 1, 352, 382],
      ['src/main.js', 3, 35, 450],
    ]
  );
  const fns = runQuery(E, 'hot functions');
  assert.deepEqual(
    fns.rows.map(r => [r.symbol, r.loc.path, r.loc.line, r.self, r.total]),
    [
      ['src/lib.js#checksum', 'src/lib.js', 1, 275, 305],
      ['src/lib.js#other', 'src/lib.js', 6, 70, 70],
      ['src/main.js#run', 'src/main.js', 3, 35, 450],
      [undefined, 'src/lib.js', 1, 7, 7],
    ]
  );
  assert.equal(fns.rows[0].unit, 'us');
  assert.equal(fns.rows[3].frame, '(module scope)');
  const [provider] = E.enrichment.providers;
  assert.deepEqual(provider.sourceMaps.map(m => [m.bundle, m.map, m.frames]), [['dist/out.js', join(dir, 'dist/out.js.map'), 1]]);
});

test('hot states every cost it could not map, by reason, package and file; nothing is dropped', async () => {
  const { dir, D } = project();
  const E = await enrich(dir, D, [['cpuprofile', 'cpu.cpuprofile', cpuProfile(dir)]]);
  const r = runQuery(E, 'hot');
  assert.equal(r.stats.sampled, 1070);
  assert.equal(r.stats.idle, 500);
  assert.equal(r.stats.mapped, 387);
  assert.deepEqual(r.stats.unmapped, {
    '(garbage collector)': 60,
    '(idle)': 500,
    '(program)': 40,
    'bundle module scope': 3,
    native: 20,
    'node internals': 10,
    node_modules: 30,
    'not in graph': 15,
    'outside root': 5,
  });
  const unmapped = Object.values(r.stats.unmapped).reduce((a, b) => a + b, 0);
  assert.equal(unmapped + r.stats.mapped, r.stats.sampled);
  assert.deepEqual(r.stats.unmappedPackages, { 'pkg:tiny-pad': 30 });
  assert.deepEqual(r.stats.unmappedFiles, {
    '/elsewhere/x.js': { reason: 'outside root', self: 5 },
    'dist/out.js': { reason: 'bundle module scope', self: 3 },
    'scripts/tool.js': { reason: 'not in graph', self: 15 },
  });
  const text = formatResult(r);
  assert.match(text, /src\/lib\.js:1 {2}file {2}#1 {2}self 352 µs \(61\.8%\) · total 382 µs \(67\.0%\)/);
  assert.match(text, /unmapped/);
});

test('hot --from: only the runtime reach of a file; the mapped cost outside it is stated', async () => {
  const { dir, D } = project();
  const E = await enrich(dir, D, [['cpuprofile', 'cpu.cpuprofile', cpuProfile(dir)]]);
  const r = runQuery(E, 'hot functions all --from src/lib.js');
  assert.equal(r.ok, true, r.error);
  assert.deepEqual(r.rows.map(x => x.symbol ?? x.key), ['src/lib.js#checksum', 'src/lib.js#other', 'src/lib.js:(module)']);
  assert.equal(r.stats.outsideReach, 35);
  assert.equal(r.stats.reach.query, 'reach src/lib.js --runtime');
  const top1 = runQuery(E, 'hot functions 1');
  assert.equal(top1.rows.length, 1);
  assert.match(top1.summary, /1 of 4 functions.*hot functions all/);
  assert.equal(runQuery(E, 'hot files --from nope.js').ok, false);
});

test('heapprofile: sampled bytes per file and function from the allocation tree', async () => {
  const { dir, D } = project();
  const url = p => pathToFileURL(join(dir, p)).href;
  const heap = {
    head: {
      callFrame: frame('(root)'),
      selfSize: 0,
      id: 1,
      children: [
        {
          callFrame: frame('run', url('src/main.js'), 2, 19),
          selfSize: 1024,
          id: 2,
          children: [{ callFrame: frame('checksum', url('src/lib.js'), 0, 24), selfSize: 4096, id: 3, children: [] }],
        },
        { callFrame: frame('(V8 API)'), selfSize: 512, id: 4, children: [] },
      ],
    },
    samples: [{ size: 4096, nodeId: 3, ordinal: 1 }, { size: 1024, nodeId: 2, ordinal: 2 }],
  };
  const E = await enrich(dir, D, [
    ['heapprofile', 'mem.heapprofile', heap],
    ['cpuprofile', 'cpu.cpuprofile', cpuProfile(dir)],
  ]);
  const r = runQuery(E, 'hot functions --heap');
  assert.equal(r.ok, true, r.error);
  assert.deepEqual(r.rows.map(x => [x.symbol, x.self, x.total, x.unit]), [
    ['src/lib.js#checksum', 4096, 4096, 'bytes'],
    ['src/main.js#run', 1024, 5120, 'bytes'],
  ]);
  assert.deepEqual(r.stats.unmapped, { native: 512 });
  // Without --heap, the CPU profile answers.
  assert.equal(runQuery(E, 'hot functions').rows[0].unit, 'us');
  assert.match(runQuery(D, 'hot').error, /no profile evidence.*cpuprofile/);
});
