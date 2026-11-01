import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';

const here = dirname(fileURLToPath(import.meta.url));

// Parallel chunk builds run several fakes at once, so state is never read-modify-written: builds append one line
// each and every snapshot is its own file.
// A fake CLI whose graph follows the files on disk: a snapshot id is the hash of its directory's files, `stale`
// compares that snapshot's file hashes with the disk, and `b.ts` declaring `h` adds a call from h to a.ts#f.
const FAKE = String.raw`
const { createHash } = require('node:crypto');
const { appendFileSync, existsSync, readFileSync, readdirSync, writeFileSync } = require('node:fs');
const { join, relative } = require('node:path');
const [root, stateFile] = [process.env.FAKE_ROOT, process.env.FAKE_STATE];
const a = process.argv.slice(2), print = (v) => console.log(JSON.stringify(v));
const out = (data) => print({ results: [{ index: 0, data }] });
const input = JSON.parse(a.find((x) => x.startsWith('{')) ?? '{}'), row = input.queries?.[0] ?? {};
const snapFile = (id) => stateFile + '.' + id + '.json';
const list = (dir) => readdirSync(dir, { recursive: true, withFileTypes: true }).filter((d) => d.isFile()).map((d) => relative(dir, join(d.parentPath, d.name))).filter((p) => !p.startsWith('.octocode'));
const hashes = (dir) => Object.fromEntries(list(dir).map((p) => [p, createHash('sha1').update(readFileSync(join(dir, p))).digest('hex')]));
const code = (dir) => list(dir).filter((p) => p.endsWith('.ts')).toSorted();
const has = (dir, f, text) => existsSync(join(dir, f)) && readFileSync(join(dir, f), 'utf8').includes(text);
function graph(dir) {
  const files = code(dir), W = [], S = [];
  for (const f of files) S.push({ id: f + '#' + f[0], file: f, name: f[0], symbolKind: 'function', line: 1, endLine: 1, exported: true });
  if (files.includes('a.ts') && files.includes('b.ts')) W.push({ edge: 'imports', from: 'a.ts', to: 'b.ts', file: 'a.ts', line: 1, confidence: 'high' }, { edge: 'calls', from: 'a.ts#a', to: 'b.ts#b', file: 'a.ts', line: 1, confidence: 'high' });
  if (has(dir, 'b.ts', 'function h')) { S.push({ id: 'b.ts#h', file: 'b.ts', name: 'h', symbolKind: 'function', line: 2, endLine: 2, exported: true }); W.push({ edge: 'calls', from: 'b.ts#h', to: 'a.ts#a', file: 'b.ts', line: 2, confidence: 'high' }); }
  return { files, W, S };
}
const page = (rows) => out({ graph: { id: row.graph }, rows });
const inTarget = (file) => !row.target || row.target.includes(file);
if (a[0] === 'schema') print({ querySchema: { properties: { target: { anyOf: [{ type: 'string' }, { type: 'array', maxItems: 100 }] } } } });
else if (a[0] === 'structureSearch') { const files = list(root); out({ files: [{ dir: root, files }], summary: { files: files.length } }); }
else if (a[0] === 'graphBuild') {
  const h = hashes(row.path), id = 'snap-' + createHash('sha1').update(row.path + JSON.stringify(h)).digest('hex').slice(0, 12);
  appendFileSync(stateFile, relative(root, row.path) + '\n');
  writeFileSync(snapFile(id), JSON.stringify(h));
  out({ id, root: row.path, counts: { nodes: 1, edges: 1 } });
} else if (row.operation === 'stale') {
  const before = existsSync(snapFile(row.graph)) ? JSON.parse(readFileSync(snapFile(row.graph), 'utf8')) : {}, now = hashes(row.path);
  const rows = [...Object.keys(before).filter((p) => now[p] !== before[p]).map((p) => ({ file: p, status: now[p] ? 'changed' : 'missing' })), ...Object.keys(now).filter((p) => !before[p]).map((p) => ({ file: p, status: 'added' }))];
  out({ graph: { id: row.graph }, rows, freshness: { fresh: rows.length === 0, analysisCurrent: true } });
} else {
  const g = graph(row.path);
  if (row.operation === 'find') page(g.files.map((id) => ({ id, kind: 'file', language: 'typescript' })));
  else if (row.operation === 'witnesses') page(g.W.filter((w) => inTarget(w.from.split('#')[0])));
  else if (row.operation === 'symbols') page(g.S.filter((s) => inTarget(s.file)));
  else out({ graph: { id: row.graph }, rows: [], summary: { health: {}, byDetector: {} } });
}`;

function fakeRepo() {
  const base = tempDir('graph-refresh-');
  const root = join(base, 'repo');
  const src = {
    'pkg/a/package.json': '{"name":"a"}\n', 'pkg/a/a.ts': "import { b } from './b';\n", 'pkg/a/b.ts': 'export function b() {}\n',
    'pkg/c/package.json': '{"name":"c"}\n', 'pkg/c/c.ts': 'export function c() {}\n',
  };
  for (const [name, text] of Object.entries(src)) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), text);
  }
  const cli = join(base, 'cli.cjs');
  writeFileSync(cli, FAKE);
  const env = { OCTOCODE_CLI: `${process.execPath} ${cli}`, FAKE_ROOT: root, FAKE_STATE: join(base, 'state.json') };
  Object.assign(process.env, env);
  const out = join(base, 'result');
  const r = spawnSync(process.execPath, [join(here, 'build.mjs'), root, '--out', out, '--lsp', 'off'], { encoding: 'utf8', cwd: root, env: { ...process.env, ...env } });
  assert.equal(r.status, 0, r.stderr);
  return { root, out, builds: () => readFileSync(env.FAKE_STATE, 'utf8').trim().split('\n') };
}

const collect = () => {
  const events = [];
  return { events, emit: (e) => events.push(e), of: (type) => events.filter((e) => e.type === type) };
};

test('a refresh rebuilds only the edited chunk, streams its rows again and writes the new edge', async () => {
  const { root, out, builds } = fakeRepo();
  // lib.mjs reads OCTOCODE_CLI when it loads, so the module loads after the fake is in place.
  const { refreshRun } = await import('./refresh.mjs');
  assert.deepEqual(builds().toSorted(), ['pkg/a', 'pkg/c']);
  writeFileSync(join(root, 'pkg/a/b.ts'), 'export function b() {}\nexport function h() { a(); }\n');
  const run = collect();
  const D = await refreshRun({ root, out, emit: run.emit });
  assert.deepEqual(builds().slice(2), ['pkg/a'], 'only the edited chunk is built again');
  const refreshed = run.of('refreshed');
  assert.equal(refreshed.length, 1);
  assert.deepEqual([refreshed[0].chunks, refreshed[0].changed, refreshed[0].added, refreshed[0].removed], [['pkg-a'], ['pkg/a/b.ts'], [], []]);
  const saved = JSON.parse(readFileSync(join(out, 'data.json'), 'utf8'));
  for (const d of [D, saved]) assert.ok(d.calls.some((c) => c.source === 'pkg/a/b.ts#h' && c.target === 'pkg/a/a.ts#a'), 'the new call is in the graph');
  assert.ok(saved.files.some((f) => f.id === 'pkg/c/c.ts'), 'the unchanged chunk keeps its data from disk');
  const reset = run.of('chunk-reset');
  assert.deepEqual(reset.map((e) => [e.id, e.files.toSorted()]), [['pkg-a', ['pkg/a/a.ts', 'pkg/a/b.ts']]]);
  const at = (pred) => run.events.findIndex(pred);
  const order = [at((e) => e.type === 'chunk-reset'), at((e) => e.type === 'rows'), at((e) => e.type === 'stitched'), at((e) => e.type === 'verify'), at((e) => e.type === 'metrics'), at((e) => e.type === 'refreshed')];
  assert.ok(order.every((i, k) => i >= 0 && (k === 0 || i > order[k - 1])), `events in order: ${order.join(", ")}`);
  assert.deepEqual([...new Set(run.of('rows').map((e) => e.chunk))], ['pkg-a']);
  assert.ok(run.of('rows').some((e) => e.kind === 'witnesses' && e.rows.some((w) => w.from === 'pkg/a/b.ts#h')), 'the new edge streams as rows');
  assert.equal(run.events.find((e) => e.type === 'phase' && e.id === 'lsp').state, 'skipped');

  // Nothing changed since: no rebuild and no graph events, only the refreshed summary.
  const again = collect();
  await refreshRun({ root, out, emit: again.emit });
  assert.deepEqual(builds().slice(3), []);
  assert.deepEqual(again.events.filter((e) => e.type !== 'tool').map((e) => e.type), ['refreshed']);
  assert.deepEqual(again.of('refreshed')[0].chunks, []);

  // An added file joins its chunk; an added manifest changes the plan and the whole root runs again.
  writeFileSync(join(root, 'pkg/c/d.ts'), 'export function d() {}\n');
  const add = collect();
  await refreshRun({ root, out, emit: add.emit });
  assert.deepEqual([add.of('refreshed')[0].chunks, add.of('refreshed')[0].added], [['pkg-c'], ['pkg/c/d.ts']]);
  assert.ok(JSON.parse(readFileSync(join(out, 'data.json'), 'utf8')).files.some((f) => f.id === 'pkg/c/d.ts'));
  mkdirSync(join(root, 'pkg/e'));
  writeFileSync(join(root, 'pkg/e/package.json'), '{"name":"e"}\n');
  writeFileSync(join(root, 'pkg/e/e.ts'), 'export function e() {}\n');
  const plan = collect();
  await refreshRun({ root, out, emit: plan.emit });
  const last = plan.of('refreshed')[0];
  assert.equal(last.plan, true);
  assert.deepEqual(last.chunks.toSorted(), ['pkg-a', 'pkg-c', 'pkg-e']);
  assert.ok(plan.of('note').some((e) => /chunk plan changed/.test(e.text)));
  const quiet = collect();
  await refreshRun({ root, out, emit: quiet.emit });
  assert.deepEqual(quiet.of('refreshed')[0].chunks, [], 'the listing of the plan run is kept for the next refresh');
});
