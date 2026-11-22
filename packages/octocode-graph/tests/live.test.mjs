import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, realpathSync, symlinkSync, writeFileSync, existsSync } from 'node:fs';
import { gunzipSync, gzipSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import test from 'node:test';
import { accept, decoder, encode, parseFrame } from '../src/runtime/ws.mjs';
import { continuations, describeCall, pagedTool } from '../src/runtime/tools.mjs';
import { planChunks, refine } from '../src/runtime/plan.mjs';
import { chunkOf } from '../src/runtime/stitch.mjs';
import { foldEdges } from '../src/runtime/section.mjs';
import { tempDir, fakeCatalog } from './test-tmp.mjs';

const here = fileURLToPath(new URL('../src/runtime/', import.meta.url));

for (const [script, args] of Object.entries({ 'preflight.mjs': ['--json'], 'wiki.mjs': ['--help'], 'query.mjs': ['--help'], 'serve.mjs': ['--help'], 'render.mjs': ['--help'] })) {
  test(`a symlinked runtime dispatches ${script} like the canonical path`, () => {
    const cwd = tempDir('graph-view-install-');
    const installed = join(cwd, 'installed');
    symlinkSync(dirname(here), installed, 'junction');
    // Preflight must report a missing CLI even when no Octocode build is installed.
    const options = { cwd, encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: join(cwd, 'absent-cli') } };
    const direct = spawnSync(process.execPath, [join(here, script), ...args], options);
    const linked = spawnSync(process.execPath, [join(installed, 'runtime', script), ...args], options);
    if (args.includes('--help')) assert.equal(direct.status, 0, direct.stderr);
    assert.ok(direct.stdout, direct.stderr);
    assert.equal(linked.status, direct.status, linked.stderr);
    assert.equal(linked.stdout, direct.stdout);
  });
}

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


test('tool pages follow the reply\'s next first, then every row continuation, in order', async () => {
  const pages = {
    first: { next: { tool: 'astSearch', input: { cursor: 'rp' } }, results: [{ data: { symbols: [{ n: 1 }] } }] },
    rp: { results: [{ data: { symbols: [{ n: 2 }], next: { more: { kind: 'continue', tool: 'astSearch', input: { cursor: 'row' } } } } }] },
    row: { results: [{ data: { symbols: [{ n: 3 }], next: { read: { kind: 'related', input: { cursor: 'ignored' } } } } }] },
  };
  const seen = [];
  const run = async (tool, input) => { seen.push(input.cursor ?? 'first'); return pages[input.cursor ?? 'first']; };
  const out = await pagedTool('astSearch', { queries: [{}] }, { run });
  assert.deepEqual(seen, ['first', 'rp', 'row']);
  assert.deepEqual(out.flatMap((p) => p.results.flatMap((r) => r.data.symbols.map((s) => s.n))), [1, 2, 3]);
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

test('a refusal can split small directory siblings deep under a single parent', () => {
  const files = ['src/deep/a/x.ts', 'src/deep/b/y.ts'];
  const chunks = refine({ path: '.', files: files.length }, files);
  assert.deepEqual(chunks?.map((c) => c.path).sort(), ['src/deep/a', 'src/deep/b']);
});

test('the planner uses published graph extensions and retains source-bearing fixtures', () => {
  const files = ['npm/a/package.json', 'npm/a/README.md', 'fixtures/package.json', 'fixtures/example.Custom', 'notes.md'];
  const extensions = new Set(['custom']);
  const chunks = planChunks(files, { extensions });
  assert.deepEqual(chunks.map((c) => c.path), ['fixtures']);
  assert.deepEqual(planChunks(['notes.md'], { extensions, whole: true }), []);
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
  const base = realpathSync(tempDir('graph-live-'));
  const root = join(base, 'repo');
  mkdirSync(root);
  const src = { 'a.ts': "import { g } from './b';\nexport function f() {\n  g();\n  g();\n}\n", 'b.ts': 'export function g() {\n  return 1;\n}\n', 'notes.md': '# notes\n' };
  for (const [name, text] of Object.entries(src)) writeFileSync(join(root, name), text);
  const witnesses = [
    { edge: 'imports', from: 'a.ts', to: 'b.ts', path: 'a.ts', line: 1, confidence: 'high', detail: 'static-import' },
    { edge: 'calls', from: 'a.ts#f', to: 'b.ts#g', path: 'a.ts', line: 3, confidence: 'high', detail: 'import' },
    { edge: 'calls', from: 'a.ts#f', to: 'b.ts#g', path: 'a.ts', line: 4, confidence: 'high', detail: 'import' },
  ];
  const nodes = { file: [{ id: 'a.ts', kind: 'file', language: 'typescript' }, { id: 'b.ts', kind: 'file', language: 'typescript' }], symbol: [{ id: 'a.ts#f', path: 'a.ts', symbolName: 'f', kind: 'function', line: 2, endLine: 5, exported: true }, { id: 'b.ts#g', path: 'b.ts', symbolName: 'g', kind: 'function', line: 1, endLine: 3, exported: true }] };
  const cli = join(base, 'cli.mjs');
  writeFileSync(cli, `#!/usr/bin/env node
${fakeCatalog}
const a = process.argv.slice(2), print = (v) => console.log(JSON.stringify(v));
const out = (data, next) => print({ results: [{ index: 0, data }], ...(next ? { next } : {}) });
const W = ${JSON.stringify(witnesses)}, N = ${JSON.stringify(nodes)}, root = ${JSON.stringify(root)};
const input = JSON.parse(a.find((x) => x.startsWith('{')) ?? '{}'), row = input.queries?.[0] ?? {};
// One witness per page; a source-file target keeps the witnesses whose source is in that file. The cursor carries
// the next offset and the target, as an issued cursor pins its query.
const witnessPage = (o, target) => {
  const list = target ? W.filter((w) => target.includes(w.from.split('#')[0])) : W;
  const more = o + 1 < list.length;
  const next = more ? { kind: 'continue', tool: 'graphQuery', input: { cursor: 'w' + (o + 1) + '|' + (target ?? []).join(',') } } : undefined;
  const pagination = more || o > 0 ? { pagination: { unit: "items", offset: o, length: list.slice(o, o + 1).length, total: list.length } } : {};
  out({ graph: { id: 'snap' }, ...pagination, rows: list.slice(o, o + 1) }, next);
};
if (a[0] === 'schema') print({ querySchema: { $defs: { targets: { type: 'array', maxItems: 100 } }, properties: { target: { anyOf: [{ type: 'string' }, { $ref: '#/$defs/targets' }] } } } });
else if (a[0] === 'structureSearch' && process.env.TEST_REFUSE_LIST) out({errorCode:'outsideAllowedRoots', errorClass:'refused', error:'scope is outside the workspace'});
else if (a[0] === 'structureSearch') out({ files: [{ dir: root, files: ['a.ts', 'b.ts', 'notes.md'] }], summary: { files: 3 } });
else if (a[0] === 'graphBuild') { console.error('# graphBuild: parsing 2 files'); out({ graph: 'snap', root: row.path, counts: { nodes: 4, edges: 3 } }); }
else if (input.cursor) { const [o, t] = input.cursor.slice(1).split('|'); witnessPage(Number(o), t ? t.split(',') : undefined); }
else if (a[0] === 'lspSearch') print({ results: input.queries.map((q, index) => ({
  index, data: q.symbolName === 'f' ? {kind:'matches', files:[{path:root+'/b.ts',matches:[{symbolName:'g',line:1,sites:[{line:3}]}]}]} : {kind:'empty'},
})) });
else if (row.operation === 'witnesses') witnessPage(0, row.target);
else if (row.operation === 'find') out({ graph: { id: 'snap' }, rows: N[row.kind] });
else if (row.operation === 'symbols') out({ graph: { id: 'snap' }, rows: N.symbol.filter((x) => !row.target || row.target.includes(x.file)) });
else out({ graph: { id: 'snap' }, rows: [], summary: { health: {}, byDetector: {} } });`, { mode: 0o755 });
  return { root, cli };
}

test('an incompatible CLI stops before listing files or building a chunk', () => {
  const root = tempDir('graph-schema-error-');
  const cli = join(root, 'cli.mjs');
  writeFileSync(cli, `console.log(JSON.stringify({error:'incompatible schema',errorCode:'invalidInput'}));`);
  const r = spawnSync(process.execPath, [join(here, 'build.mjs'), root, '--out', root], {
    encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${cli}` },
  });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /incompatible schema/);
  const events = readFileSync(join(root, 'events.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.ok(!events.some((e) => e.type === 'chunk' || e.tool === 'structureSearch'));
});

test('a refused file listing stops with the native cause', () => {
  const { root, cli } = fakeRepo();
  const out = join(root, '.octocode', 'result');
  const run = spawnSync(process.execPath, [join(here, 'build.mjs'), root, '--out', out], {
    cwd: root, encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: cli, TEST_REFUSE_LIST: '1' },
  });
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /outsideAllowedRoots/);
  const events = readFileSync(join(out, 'events.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.ok(!events.some(e => e.tool === 'graphBuild' || e.text?.includes('git ls-files')));
});

test('runs reuse one frozen image and replacing the source cannot change a running image', async () => {
  const { freezeExecutable } = await import('../src/runtime/lib/pin-cli.mjs');
  const folder = tempDir('graph-pin-');
  const source = join(folder, 'octocode');
  writeFileSync(source, 'first');
  const cache = join(folder, 'cache');
  const first = await freezeExecutable(source, cache);
  assert.equal(await freezeExecutable(source, cache), first);
  writeFileSync(source, 'second');
  const second = await freezeExecutable(source, cache);
  assert.notEqual(second, first);
  assert.equal(readFileSync(first, 'utf8'), 'first');
  assert.equal(readFileSync(second, 'utf8'), 'second');
});

test('a graph without an advertised LSP tool keeps its calls and skips confirmation', () => {
  const { root, cli } = fakeRepo();
  const out = join(root, '.octocode', 'result');
  const run = spawnSync(process.execPath, [join(here, 'build.mjs'), root, '--out', out], {
    cwd: root, encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: cli, TEST_NO_LSP: '1' },
  });
  assert.equal(run.status, 0, run.stderr);
  const graph = JSON.parse(readFileSync(join(out, 'data.json'), 'utf8'));
  assert.deepEqual(graph.calls[0].lines, [3, 4]);
  assert.match(graph.callStats.lsp, /lspSearch unavailable/);
  assert.equal(graph.callStats.probed, 0);
  const events = readFileSync(join(out, 'events.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line));
  assert.ok(events.some(e => e.id === 'lsp' && e.state === 'skipped' && e.note === graph.callStats.lsp));
  assert.ok(!events.some(e => e.tool === 'lspSearch'));
});

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
  assert.ok(events.some((e) => e.type === 'tool' && e.page === 'cursor w2|'), 'graph continuation pages are logged as tool calls');
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

test('the stitched graph answers a query before the first LSP probe and publishes a final result', () => {
  const {root, cli} = fakeRepo();
  const out = join(root, '.octocode', 'result');
  const source = `
    import assert from 'node:assert/strict';
    import {existsSync, readFileSync} from 'node:fs';
    import {runPipeline, defaults} from ${JSON.stringify(new URL('../src/runtime/pipeline.mjs', import.meta.url).href)};
    import {runQuery} from ${JSON.stringify(new URL('../src/runtime/query.mjs', import.meta.url).href)};
    let live, ready = false, batches = 0;
    const out = ${JSON.stringify(out)};
    const data = await runPipeline({root: ${JSON.stringify(root)}, out, opts: defaults(), publish: read => { live = read; }, emit(e) {
      if (e.type === 'graph-ready') {
        ready = true;
        assert.equal(existsSync(out+'/data.json'), false);
        const answer = runQuery(live(), 'deps a.ts');
        assert.equal(answer.lspStatus, 'pending');
        assert.ok(answer.nodes.includes('b.ts'), JSON.stringify(answer));
        assert.equal(JSON.parse(readFileSync(e.data)).callStats.lsp, 'pending');
      }
      if (e.type === 'lsp') {
        assert.ok(ready);
        assert.equal(e.stats.lsp, 'pending');
        assert.equal(live().callStats.probed, e.probed);
        if (e.stats.both) assert.equal(runQuery(live(), 'callees a.ts#f').rows[0].via, 'both');
        batches++;
      }
    }});
    assert.ok(ready && batches > 0);
    assert.equal(data.callStats.both, 1);
    assert.notEqual(data.callStats.lsp, 'pending');
    assert.equal(JSON.parse(readFileSync(out+'/data.json')).callStats.lsp, data.callStats.lsp);
    assert.equal(live, null);
  `;
  const result = spawnSync(process.execPath, ['--input-type=module','-e',source], {cwd:root,encoding:'utf8',env:{...process.env,OCTOCODE_CLI:cli}});
  assert.equal(result.status, 0, result.stderr);
});

for (const script of ['build.mjs', 'serve.mjs']) {
  test(`${script} rejects invalid concurrency before creating the run`, () => {
    const root = tempDir('graph-invalid-jobs-');
    const out = join(root, 'result');
    for (const flag of ['--jobs', '--ingest-jobs']) for (const value of ['oops', '0', '-1', '1.5', 'Infinity', '9007199254740992', '']) {
      const r = spawnSync(process.execPath, [join(here, script), root, '--out', out, ...(script === 'serve.mjs' ? ['--no-open', '--exit-when-done'] : []), flag, value], {
        env: { ...process.env, OCTOCODE_CLI: join(root, 'missing-cli') }, encoding: 'utf8',
      });
      assert.equal(r.status, 2, r.stderr);
      assert.ok(r.stderr.includes(flag));
      assert.equal(existsSync(out), false);
    }
  });

  test(`${script} preserves prior output when the directory is reused`, () => {
    const root = tempDir('graph-reused-out-');
    const out = join(root, 'saved');
    mkdirSync(out);
    const log = '{"seq":0,"type":"run","root":"/original"}\n';
    writeFileSync(join(out, 'events.jsonl'), log);
    const r = spawnSync(process.execPath, [join(here, script), root, '--out', out, ...(script === 'serve.mjs' ? ['--no-open', '--exit-when-done'] : [])], {
      env: { ...process.env, OCTOCODE_CLI: join(root, 'missing-cli') }, encoding: 'utf8',
    });
    assert.equal(r.status, 1);
    assert.match(r.stderr, /new.*directory/i);
    assert.equal(readFileSync(join(out, 'events.jsonl'), 'utf8'), log);
    assert.deepEqual(readdirSync(out), ['events.jsonl']);
  });
}

test('replay refuses an old event stream containing two run headers', () => {
  const out = tempDir('graph-mixed-log-');
  const events = ['first', 'second'].map(root => JSON.stringify({ seq: 0, type: 'run', root }));
  writeFileSync(join(out, 'events.jsonl'), events.join('\n'));
  const r = spawnSync(process.execPath, [join(here, 'serve.mjs'), '--replay', out, '--no-open', '--exit-when-done'], { encoding: 'utf8' });
  assert.equal(r.status, 1);
  assert.match(r.stderr, /multiple runs/i);
  assert.equal(existsSync(join(out, 'server.json')), false);
});

test('a full refresh can repeat its run header while event numbers advance', async () => {
  const { readEvents } = await import('../src/runtime/pipeline.mjs');
  const dir = tempDir('graph-refresh-log-');
  const events = [0, 1].map(seq => ({ seq, type: 'run', root: '/original' }));
  writeFileSync(join(dir, 'events.jsonl'), events.map(e => JSON.stringify(e)).join('\n'));
  assert.deepEqual(readEvents(dir), events);
});

test('the graph package resolves its engine through the declared octocode dependency outside a checkout', () => {
  const root = tempDir('graph-mcp-dependency-');
  const req = createRequire(import.meta.url);
  const pkg = req('../package.json');
  assert.ok(pkg.dependencies?.octocode);
  const bin = join(root, 'native');
  writeFileSync(bin, '#!/bin/sh\necho graph-package-test\n', { mode: 0o755 });
  const env = { ...process.env, NODE_ENV: 'development', OCTOCODE_NATIVE_BIN: bin };
  delete env.OCTOCODE_CLI;
  const code = `import { cliOk } from ${JSON.stringify(new URL('../src/runtime/lib.mjs', import.meta.url))}; process.exit(await cliOk(['--version']) ? 0 : 1)`;
  const r = spawnSync(process.execPath, ['--input-type=module', '-e', code], { cwd: root, env: { ...env, PATH: root }, encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
});

test('concurrent builds record complete tool events in their own result directories', () => {
  const { root, cli } = fakeRepo();
  const source = `
    import assert from 'node:assert/strict';
    import { join } from 'node:path';
    import { defaults, runPipeline } from ${JSON.stringify(new URL('../src/runtime/pipeline.mjs', import.meta.url).href)};
    const root = process.cwd();
    const logs = [[], []];
    await Promise.all(logs.map((events, i) => runPipeline({
      root, out: join(root, '.octocode', String(i)), opts: {...defaults(), lsp:'off'},
      emit: event => { if (event.type === 'tool') events.push(event); },
    })));
    for (const events of logs) {
      assert.equal(events.filter(e => e.tool === 'structureSearch' && e.state === 'start').length, 1);
      const ids = state => events.filter(e => e.state === state).map(e => e.id).sort();
      assert.deepEqual(ids('start'), ids('done'));
    }
  `;
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', source], { cwd: root, env: {...process.env, OCTOCODE_CLI:cli}, encoding:'utf8' });
  assert.equal(child.status, 0, child.stderr);
});

test('a failed listen leaves the requested result directory available for a retry', async t => {
  const { startServer } = await import('../src/runtime/serve.mjs');
  const listener = createServer();
  await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => listener.close(resolve)));
  const root = tempDir('graph-view-port-');
  const out = join(root, 'saved');
  await assert.rejects(startServer({root, out, port:listener.address().port, open:false}), {code:'EADDRINUSE'});
  assert.equal(existsSync(join(out, 'events.jsonl')), false);
});

for (const script of ['build.mjs', 'serve.mjs']) {
  test(`${script} validates option spelling and values before creating output`, () => {
    const root = tempDir('graph-cli-options-'), out = join(root, 'result');
    const cases = [['--lsp', 'of'], ['--job', '1'], ['--jobs=oops'], ['--jobs', '1', '--jobs', 'oops']];
    if (script === 'serve.mjs') cases.push(['--port', 'oops'], ['--port', '65536']);
    for (const args of cases) {
      const r = spawnSync(process.execPath, [join(here, script), root, '--out', out, ...(script === 'serve.mjs' ? ['--no-open', '--exit-when-done'] : []), ...args], {
        env: { ...process.env, OCTOCODE_CLI: join(root, 'missing-cli') }, encoding: 'utf8',
      });
      assert.equal(r.status, 2, args.join(' '));
      assert.ok(r.stderr.includes(args[0].split('=')[0]), r.stderr);
      assert.equal(existsSync(out), false);
    }
  });
}

test('a backpressured viewer resumes from the log while another drains normally', async () => {
  const {eventStream} = await import('../src/runtime/lib/event-stream.mjs');
  const log = [{seq:0, text:'one indivisible event'.repeat(30)}, {seq:4}, {seq:7}];
  const stream = eventStream(log);
  const turn = () => new Promise(resolve => setImmediate(resolve));
  function subscriber(blocks) {
    const messages = [];
    const client = {byteTarget:1, writable:true, sendBinary(payload) {
      messages.push(JSON.parse(gunzipSync(payload)));
      if (blocks && messages.length === 1) client.writable = false;
    }};
    stream.clients.add(client);
    stream.hello(client, 0);
    return {client, messages};
  }
  const slow = subscriber(true), fast = subscriber(false);
  await turn();
  log.push({seq:9}); stream.schedule();
  for (let i = 0; i < log.length; i++) await turn();
  assert.deepEqual(slow.messages.flatMap(m=>m.events),log.slice(0,1));
  assert.deepEqual(fast.messages.flatMap(m=>m.events),log);
  slow.client.writable = true;
  stream.schedule();
  for (let i = 0; i < log.length; i++) await turn();
  assert.deepEqual(slow.messages,fast.messages);
  assert.ok(!slow.messages.at(-1).replay);
});

test('a failed or closed socket cannot apply queued pages past the last good batch', async () => {
  const {orderedInbox} = await import('../src/assets/live-connection.js');
  const applied = [], errors = [];
  const reader = orderedInbox(JSON.parse, n=>applied.push(n), e=>errors.push(e));
  await Promise.all(['1','incomplete','3'].map(s=>reader.push(s)));
  assert.deepEqual(applied,[1]);
  assert.equal(errors.length,1);
  let release;
  const waiting = new Promise(resolve=>{release=resolve;});
  const old = orderedInbox(()=>waiting, n=>applied.push(n), e=>errors.push(e));
  const pending = old.push('2');
  await Promise.resolve();
  old.close();
  release(2);
  await pending;
  assert.deepEqual(applied,[1]);
});

test('a file input keeps its own graph scope and uses its directory for evidence paths', async () => {
  const { runOptions } = await import('../src/runtime/lib/run-options.mjs');
  const project = tempDir('file-graph-');
  const file = join(project, 'main.ts');
  writeFileSync(file, 'export const main = 1;\n');
  writeFileSync(join(project, 'sibling.ts'), 'export const sibling = 2;\n');
  const out = join(project, 'result');
  const options = runOptions([file, '--out', out]);
  assert.equal(options.root, realpathSync(project));
  assert.equal(options.opts.file, 'main.ts');
  const { listFiles } = await import('../src/runtime/pipeline.mjs');
  const events = [];
  assert.deepEqual(await listFiles(project, event => events.push(event), options.opts.file), ['main.ts']);
  assert.deepEqual(events[0].files, ['main.ts']);
  assert.throws(() => runOptions([join(project, 'absent.ts'), '--out', out]), /ENOENT|not found/);
  assert.equal(existsSync(out), false);
});
