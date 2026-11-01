import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync, existsSync } from 'node:fs';
import { gunzipSync, gzipSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import test from 'node:test';
import { accept, decoder, encode, parseFrame } from './ws.mjs';
import { continuations, describeCall, pagedTool } from './tools.mjs';
import { planChunks, refine } from './plan.mjs';
import { chunkOf } from './stitch.mjs';
import { foldEdges, groupWalk } from './section.mjs';
import { localCli } from './lib.mjs';
import { tempDir } from './test-tmp.mjs';

const here = dirname(fileURLToPath(import.meta.url));

// A browser-side frame: masked, optionally not final, optionally a continuation.
function clientFrame(text, { fin = true, opcode = 1 } = {}) {
  const body = Buffer.from(text);
  const key = Buffer.from([1, 2, 3, 4]);
  const masked = Buffer.from(body.map((b, i) => b ^ key[i & 3]));
  const n = body.length;
  const head = n < 126 ? Buffer.from([(fin ? 0x80 : 0) | opcode, 0x80 | n]) : Buffer.from([(fin ? 0x80 : 0) | opcode, 0x80 | 126, n >> 8, n & 255]);
  return Buffer.concat([head, key, masked]);
}

test('server frames round-trip every length class, including Unicode and 64-bit lengths', () => {
  for (const text of ['hi', 'x'.repeat(300), '🙂'.repeat(20000)]) {
    const f = parseFrame(encode(text));
    assert.equal(f.payload.toString(), text);
    assert.equal(f.fin, true);
  }
});

test('the decoder joins frames split across reads, several per read, and fragmented messages', () => {
  const got = [];
  const feed = decoder({ onMessage: (m) => got.push(m), onControl: () => {}, onError: (e) => got.push(`error ${e.message}`) });
  const whole = Buffer.concat([clientFrame('{"op":"a"}'), clientFrame('שלום'.repeat(100)), clientFrame('part1-', { fin: false }), clientFrame('part2', { opcode: 0 })]);
  for (let i = 0; i < whole.length; i += 7) feed(whole.subarray(i, i + 7));
  assert.deepEqual(got, ['{"op":"a"}', 'שלום'.repeat(100), 'part1-part2']);
  const bad = [];
  decoder({ onMessage: () => {}, onControl: () => {}, onError: (e) => bad.push(e.message) })(encode('unmasked'));
  assert.deepEqual(bad, ['client frames must be masked']);
});

test('a real WebSocket client completes the handshake and exchanges JSON both ways', async () => {
  const server = createServer();
  server.on('upgrade', (req, socket) => accept(req, socket, { onMessage: (text, client) => client.send({ echo: JSON.parse(text), big: 'z'.repeat(70000) }) }));
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const ws = new WebSocket(`ws://127.0.0.1:${server.address().port}/ws`);
  const reply = await new Promise((resolve, reject) => {
    ws.addEventListener('open', () => ws.send(JSON.stringify({ op: 'hello' })));
    ws.addEventListener('message', (m) => resolve(JSON.parse(m.data)));
    ws.addEventListener('error', reject);
  });
  ws.close();
  server.close();
  assert.deepEqual(reply.echo, { op: 'hello' });
  assert.equal(reply.big.length, 70000);
});

test('gzipped binary batches arrive intact and decode in a browser-compatible way', async () => {
  const events = Array.from({ length: 3000 }, (_, i) => ({ seq: i, type: 'rows', rows: [{ id: `src/🙂/${i}.ts`, line: i }] }));
  const server = createServer();
  server.on('upgrade', (req, socket) => { const c = accept(req, socket); c.sendBinary(gzipSync(JSON.stringify({ type: 'batch', events }))); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const ws = new WebSocket(`ws://127.0.0.1:${server.address().port}/ws`);
  ws.binaryType = 'arraybuffer';
  const data = await new Promise((resolve, reject) => { ws.addEventListener('message', (m) => resolve(m.data)); ws.addEventListener('error', reject); });
  ws.close();
  server.close();
  const text = await new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'))).text();
  assert.deepEqual(JSON.parse(text).events, events);
  assert.ok(data.byteLength < JSON.stringify(events).length / 4, 'the batch is compressed');
});

test('the checkout runs the binary the launcher resolves, by its real path', (t) => {
  const root = join(here, '..', '..', '..');
  const resolverPath = join(root, 'packages', 'octocode-native', 'bin', 'resolve-binary.cjs');
  const launcher = join(root, 'packages', 'octocode', 'out', 'octocode.js');
  let resolved;
  try {
    resolved = createRequire(import.meta.url)(resolverPath).resolveNativeBinaryPath();
  } catch {
    t.skip('no native binary installed in this checkout');
    return;
  }
  const [bin, ...rest] = localCli(root, {});
  assert.deepEqual(rest, []);
  // A real path: the warm daemon is keyed by binary path, so a symlinked path would start a second, cold one.
  assert.equal(bin, realpathSync(resolved));
  if (existsSync(launcher)) {
    assert.equal(execFileSync(bin, ['--version'], { encoding: 'utf8' }), execFileSync(process.execPath, [launcher, '--version'], { encoding: 'utf8' }));
  }
});

const viaLauncher = (root) => [process.execPath, join(root, 'packages', 'octocode', 'out', 'octocode.js')];
test('the launcher stays in charge of OCTOCODE_NATIVE_BIN and of a missing resolver', () => {
  const root = join(here, '..', '..', '..');
  assert.deepEqual(localCli(root, { OCTOCODE_NATIVE_BIN: '/dev/octocode' }), viaLauncher(root));
  const empty = tempDir('graph-view-cli-');
  assert.deepEqual(localCli(empty, {}), viaLauncher(empty));
});

test('tool pages follow responsePagination first, then every row continuation, in order', async () => {
  const pages = {
    first: { responsePagination: { next: { tool: 'astSearch', input: { cursor: 'rp' } } }, results: [{ data: { symbols: [{ n: 1 }] } }] },
    rp: { results: [{ data: { symbols: [{ n: 2 }], next: { more: { kind: 'continue', tool: 'astSearch', input: { cursor: 'row' } } } } }] },
    row: { results: [{ data: { symbols: [{ n: 3 }], next: { read: { kind: 'related', input: { cursor: 'ignored' } } } } }] },
  };
  const seen = [];
  const run = async (tool, input) => { seen.push(input.cursor ?? 'first'); return pages[input.cursor ?? 'first']; };
  const out = await pagedTool('astSearch', { queries: [{}] }, { run });
  assert.deepEqual(seen, ['first', 'rp', 'row']);
  assert.deepEqual(out.flatMap((p) => p.results[0].data.symbols.map((s) => s.n)), [1, 2, 3]);
  assert.deepEqual(continuations(pages.row), []);
  await assert.rejects(pagedTool('x', {}, { run: async () => ({ results: [{ data: { next: { a: { kind: 'continue', input: { cursor: 'same' } } } } }] }) }), /Repeated/);
});

test('tool calls describe their page: graph rows and cursors', () => {
  const graph = describeCall(['graphQuery', '--json', JSON.stringify({ queries: [{ operation: 'unresolved', path: '/r/src', target: ['a.rs', 'b.rs'], edges: ['calls'] }], responseChars: 50000 })]);
  assert.deepEqual(graph, { tool: 'graphQuery', detail: 'unresolved · 2 targets · calls', page: 'first' });
  assert.equal(describeCall(['graphBuild', '--json', JSON.stringify({ queries: [{ path: '/r/packages/app' }] })]).detail, 'packages/app');
  assert.equal(describeCall(['lspSearch', '--json', '{"cursor":"abcdefghijklmnop"}']).page, 'cursor ghijklmnop');
  assert.match(describeCall(['lspSearch', '--json', JSON.stringify({ queries: [{ operation: 'callees', path: 'a/b.ts', symbolName: 'run' }] })]).detail, /callees · run@b\.ts/);
});

const REPO = ['package.json', 'README.md', 'packages/a/package.json', 'packages/a/src/x.ts', 'packages/a/src/y.ts', 'packages/b/Cargo.toml', 'packages/b/src/lib.rs', 'packages/notes.md', 'tools/run.mjs', 'tools/nested/pkg/package.json', 'tools/nested/pkg/i.js'];

test('chunks follow package manifests and every file belongs to exactly one chunk', () => {
  const chunks = planChunks(REPO);
  const byPath = Object.fromEntries(chunks.map((c) => [c.path, c]));
  assert.deepEqual(Object.keys(byPath).toSorted(), ['.', 'packages', 'packages/a', 'packages/b', 'tools', 'tools/nested/pkg']);
  assert.deepEqual(byPath['.'].exclude.toSorted(), ['packages', 'tools']);
  assert.equal(byPath.packages.loose, true);
  for (const p of REPO) assert.ok(chunkOf(p, chunks), `${p} has a chunk`);
  assert.equal(chunks.reduce((a, c) => a + c.files, 0), REPO.length);
  assert.equal(new Set(chunks.map((c) => c.id)).size, chunks.length);
  assert.deepEqual(planChunks(REPO, { whole: true }).map((c) => [c.path, c.files]), [['.', REPO.length]]);
});

test('a chunk refused as too broad splits one level and keeps every file', () => {
  const files = [...Array.from({ length: 12 }, (_, i) => `src/big/f${i}.ts`), 'src/small/a.ts', 'src/top.ts'];
  const parts = refine({ path: 'src', files: files.length }, files);
  assert.deepEqual(parts.map((p) => [p.path, p.files, !!p.loose]), [['src', 2, true], ['src/big', 12, false]]);
  assert.equal(refine({ path: 'src/big', files: 12 }, files), null);
});

const fileOf = (id) => id.split('#')[0];
test('witness and symbol groups are walked concurrently and must add up to the whole walk, else the single walk runs', async () => {
  const W = [{ id: 'w1', from: 'a.ts' }, { id: 'w2', from: 'a.ts#f' }, { id: 'w3', from: 'b.ts' }, { id: 'w4', from: 'c.ts' }];
  const calls = [];
  const q = async (_stream, row) => {
    calls.push(row.target ?? 'all');
    return { rows: row.target ? W.filter((w) => row.target.includes(fileOf(w.from))) : W };
  };
  const fields = { operation: 'witnesses' };
  const grouped = await groupWalk(q, 'witnesses', ['c.ts', 'a.ts', 'b.ts'], { jobs: 1, fields, total: async () => 4, max: 2 });
  assert.deepEqual([grouped.grouped, grouped.groups, grouped.rows.map((w) => w.id).toSorted()], [true, 3, ['w1', 'w2', 'w3', 'w4']]);
  assert.deepEqual(calls, [['a.ts'], ['b.ts'], ['c.ts']], 'about four groups per worker, of sorted files');
  calls.length = 0;
  const many = Array.from({ length: 12 }, (_, i) => `f${String(i).padStart(2, '0')}.ts`);
  await groupWalk(async (_s, row) => (calls.push(row.target), { rows: [] }), 'witnesses', many, { jobs: 1, fields, total: async () => 0, max: 2 });
  assert.ok(calls.every((t) => t.length <= 2) && calls.flat().length === 12, 'no group exceeds the schema list maximum');
  // A witness from a file outside every group (here b.ts is missing) makes the union short: the single walk answers.
  calls.length = 0;
  const fallback = await groupWalk(q, 'witnesses', ['a.ts', 'c.ts'], { jobs: 2, fields, total: async () => 4, max: 1 });
  assert.deepEqual([fallback.grouped, fallback.rows.length, calls.at(-1)], [false, 4, 'all']);
});

test('folded edges keep every witness line, the strongest confidence and test-only marks', () => {
  const w = [
    { edge: 'calls', from: 'a#f', to: 'b#g', line: 9, confidence: 'low', test: false },
    { edge: 'calls', from: 'a#f', to: 'b#g', line: 3, confidence: 'high', detail: 'import', test: false },
    { edge: 'calls', from: 'a#f', to: 'b#g', line: 9, confidence: 'medium', test: false },
  ];
  const [e] = foldEdges(w, 'calls');
  assert.deepEqual([e.lines, e.line, e.count, e.confidence, e.test, e.resolution], [[3, 9], 3, 3, 'high', false, ['import']]);
});

// A fake CLI that answers every call the pipeline makes, with witness pages of one row each.
function fakeRepo() {
  const base = tempDir('graph-live-');
  const root = join(base, 'repo');
  mkdirSync(root);
  const src = { 'a.ts': "import { g } from './b';\nexport function f() {\n  g();\n  g();\n}\n", 'b.ts': 'export function g() {\n  return 1;\n}\n', 'notes.md': '# notes\n' };
  for (const [name, text] of Object.entries(src)) writeFileSync(join(root, name), text);
  const witnesses = [
    { edge: 'imports', from: 'a.ts', to: 'b.ts', file: 'a.ts', line: 1, confidence: 'high', detail: 'static-import' },
    { edge: 'calls', from: 'a.ts#f', to: 'b.ts#g', file: 'a.ts', line: 3, confidence: 'high', detail: 'import' },
    { edge: 'calls', from: 'a.ts#f', to: 'b.ts#g', file: 'a.ts', line: 4, confidence: 'high', detail: 'import' },
  ];
  const nodes = { file: [{ id: 'a.ts', kind: 'file', language: 'typescript' }, { id: 'b.ts', kind: 'file', language: 'typescript' }], symbol: [{ id: 'a.ts#f', file: 'a.ts', name: 'f', symbolKind: 'function', line: 2, endLine: 5, exported: true }, { id: 'b.ts#g', file: 'b.ts', name: 'g', symbolKind: 'function', line: 1, endLine: 3, exported: true }] };
  const cli = join(base, 'cli.mjs');
  writeFileSync(cli, `const a = process.argv.slice(2), print = (v) => console.log(JSON.stringify(v));
const out = (data) => print({ results: [{ index: 0, data }] });
const W = ${JSON.stringify(witnesses)}, N = ${JSON.stringify(nodes)}, root = ${JSON.stringify(root)};
const input = JSON.parse(a.find((x) => x.startsWith('{')) ?? '{}'), row = input.queries?.[0] ?? {};
// One witness per page; a source-file target keeps the witnesses whose source is in that file. The cursor carries
// the next offset and the target, as an issued cursor pins its query; pages state no offset (the live contract).
const witnessPage = (o, target) => {
  const list = target ? W.filter((w) => target.includes(w.from.split('#')[0])) : W;
  const more = o + 1 < list.length;
  const next = more ? { next: { nextPage: { kind: 'continue', tool: 'graphQuery', input: { cursor: 'w' + (o + 1) + '|' + (target ?? []).join(',') } } } } : {};
  const pagination = more || o > 0 ? { pagination: { page: o + 1, totalItems: list.length, hasMore: more } } : {};
  out({ graph: { id: 'snap' }, ...pagination, rows: list.slice(o, o + 1), ...next });
};
if (a[0] === 'schema') print({ querySchema: { properties: { target: { anyOf: [{ type: 'string' }, { type: 'array', maxItems: 100 }] } } } });
else if (a[0] === 'structureSearch') out({ files: [{ dir: root, files: ['a.ts', 'b.ts', 'notes.md'] }], summary: { files: 3 } });
else if (a[0] === 'graphBuild') { console.error('graphBuild: parsing 2 files'); out({ id: 'snap', root: row.path, counts: { nodes: 4, edges: 3 } }); }
else if (input.cursor) { const [o, t] = input.cursor.slice(1).split('|'); witnessPage(Number(o), t ? t.split(',') : undefined); }
else if (row.operation === 'witnesses') witnessPage(0, row.target);
else if (row.operation === 'find') out({ graph: { id: 'snap' }, rows: N[row.kind] });
else if (row.operation === 'symbols') out({ graph: { id: 'snap' }, rows: N.symbol.filter((x) => !row.target || row.target.includes(x.file)) });
else out({ graph: { id: 'snap' }, rows: [], summary: { health: {}, byDetector: {} } });`);
  return { root, cli };
}

test('the headless run streams rows whose fold is data.json, and every node and edge has a location', () => {
  const { root, cli } = fakeRepo();
  const out = join(root, '.octocode', 'result');
  mkdirSync(out, { recursive: true });
  const r = spawnSync(process.execPath, [join(here, 'build.mjs'), root, '--out', out, '--lsp', 'off'], { encoding: 'utf8', cwd: root, env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${cli}` } });
  assert.equal(r.status, 0, r.stderr);
  const events = readFileSync(join(out, 'events.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  const D = JSON.parse(readFileSync(join(out, 'data.json'), 'utf8'));
  // Every page arrived as its own event, and the skeleton lists non-code files the graph did not parse.
  assert.equal(events.filter((e) => e.type === 'rows' && e.kind === 'witnesses').length, 3);
  assert.deepEqual(events.find((e) => e.type === 'skeleton').files.toSorted(), ['a.ts', 'b.ts', 'notes.md']);
  const streamed = foldEdges(events.filter((e) => e.type === 'rows' && e.kind === 'witnesses').flatMap((e) => e.rows), 'calls');
  assert.deepEqual(streamed.map((e) => [e.source, e.target, e.lines]), D.calls.map((e) => [e.source, e.target, e.lines]));
  assert.deepEqual(D.calls[0].lines, [3, 4]);
  const located = [...D.files, ...D.symbols, ...D.imports, ...D.calls];
  for (const x of located) {
    assert.ok(x.loc && existsSync(x.loc.abs), `${x.id ?? `${x.source}>${x.target}`} has a real location`);
    const total = readFileSync(x.loc.abs, 'utf8').split('\n').length;
    for (const l of [x.loc.line, x.loc.endLine, ...(x.loc.lines ?? [])].filter(Boolean)) assert.ok(l >= 1 && l <= total, `line ${l} inside ${x.loc.path}`);
  }
  assert.deepEqual(events.filter((e) => e.type === 'phase' && e.state !== 'start').map((e) => e.id), ['list', 'plan', 'graph', 'stitch', 'lsp', 'verify', 'metrics']);
  // Every file carries algorithm metrics, and the stream carried them as columns aligned to one id list.
  assert.ok(D.files.every((f) => Number.isFinite(f.metrics.pagerank) && Number.isInteger(f.metrics.depth)));
  const metrics = events.find((e) => e.type === 'metrics');
  assert.equal(metrics.columns.pagerank.length, metrics.ids.length);
  assert.deepEqual(D.fileEdges.map((e) => [e.source, e.target, e.weight]), [['a.ts', 'b.ts', 3]]);
  assert.ok(events.some((e) => e.type === 'tool' && e.page === 'cursor w2|a.ts'), 'graph continuation pages are logged as tool calls');
  assert.ok(events.some((e) => e.type === 'chunk' && e.state === 'ingesting' && e.build === 'parsing 2 files'), 'build progress reaches the viewer per chunk');
  assert.ok(existsSync(join(out, 'brief.txt')));
  // One chunk: no other chunk to bind calls into, so no unresolved query runs at all.
  assert.equal(events.filter((e) => e.type === 'tool' && /unresolved/.test(e.detail ?? '')).length, 0);
  // Evidence pages are gzipped JSON lines that decompress to every page with its input.
  const evidence = join(out, 'chunks', 'root', 'tmp', 'evidence');
  const pages = readdirSync(evidence).filter((f) => f.endsWith('.jsonl.gz')).flatMap((f) => gunzipSync(readFileSync(join(evidence, f))).toString().trim().split('\n').map((l) => JSON.parse(l)));
  const witnessPages = pages.filter((p) => p.input.queries?.[0]?.operation === 'witnesses' || /^w\d/.test(p.input.cursor ?? ''));
  assert.equal(witnessPages.filter((p) => p.response.results[0].data.rows.length).length, 3, 'every witness page with rows is kept');
  // Every node was checked against its file on disk.
  assert.deepEqual(D.files.map((f) => [f.id, f.check.status, f.check.lines]), [['a.ts', 'ok', 5], ['b.ts', 'ok', 3]]);
  assert.equal(events.filter((e) => e.type === 'verify').at(-1).summary.ok, 2);
});
