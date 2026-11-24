import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { existsSync, mkdirSync, readFileSync, readdirSync, utimesSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { tempDir, fakeCatalog } from './test-tmp.mjs';

const here = fileURLToPath(new URL('../src/runtime/', import.meta.url));

// Parallel chunk builds run several fakes at once, so state is never read-modify-written: builds append one line
// each and every snapshot is its own file.
// A fake CLI whose graph follows the files on disk: a snapshot id is the hash of its directory's files, `stale`
// compares that snapshot's file hashes with the disk, and `b.ts` declaring `h` adds a call from h to a.ts#f.
const FAKE = String.raw`#!/usr/bin/env node
${fakeCatalog}

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
  for (const f of files) S.push({ id: f + '#' + f[0], path: f, symbolName: f[0], kind: 'function', line: 1, endLine: 1, exported: true });
  if (files.includes('a.ts') && files.includes('b.ts')) W.push({ edge: 'imports', from: 'a.ts', to: 'b.ts', path: 'a.ts', line: 1, confidence: 'high' }, { edge: 'calls', from: 'a.ts#a', to: 'b.ts#b', path: 'a.ts', line: 1, confidence: 'high' });
  if (has(dir, 'a.ts', "from 'c'")) W.push({ edge: 'imports', from: 'a.ts', to: 'pkg:c', path: 'a.ts', line: 1, confidence: 'high' });
  if (has(dir, 'b.ts', 'function h')) { S.push({ id: 'b.ts#h', path: 'b.ts', symbolName: 'h', kind: 'function', line: 2, endLine: 2, exported: true }); W.push({ edge: 'calls', from: 'b.ts#h', to: 'a.ts#a', path: 'b.ts', line: 2, confidence: 'high' }); }
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
  out({ graph: id, root: row.path, counts: { nodes: 1, edges: 1 } });
} else if (row.operation === 'stale') {
  const before = existsSync(snapFile(row.graph)) ? JSON.parse(readFileSync(snapFile(row.graph), 'utf8')) : {}, now = hashes(row.path);
  const rows = [...Object.keys(before).filter((p) => now[p] !== before[p]).map((p) => ({ path: p, status: now[p] ? 'changed' : 'missing' })), ...Object.keys(now).filter((p) => !before[p]).map((p) => ({ path: p, status: 'added' }))];
  for (const entry of rows) entry.path = relative(root, join(row.path, entry.path));
  out({ graph: { id: row.graph }, rows, freshness: { fresh: rows.length === 0, analysisCurrent: true } });
} else {
  const g = graph(row.path);
  if (row.operation === 'find') page(g.files.map((id) => ({ id, kind: 'file', language: 'typescript' })));
  else if (row.operation === 'witnesses') page(g.W.filter((w) => inTarget(w.from.split('#')[0])));
  else if (row.operation === 'symbols') page(g.S.filter((s) => inTarget(s.file)).map((s) => ({ ...s, path: relative(root, join(row.path, s.path)) })));
  else out({ graph: { id: row.graph }, rows: [], summary: { health: {}, byDetector: {} } });
}`;

const REPO = {
  'pkg/a/package.json': '{"name":"a"}\n', 'pkg/a/a.ts': "import { b } from './b';\n", 'pkg/a/b.ts': 'export function b() {}\n',
  'pkg/c/package.json': '{"name":"c"}\n', 'pkg/c/c.ts': 'export function c() {}\n',
};

// `src` written under <base>/repo and built from its `sub` directory, the build's root, into <base>/result.
function fakeRepo({ src = REPO, sub = '.' } = {}) {
  const base = tempDir('graph-refresh-');
  const repo = join(base, 'repo');
  const root = join(repo, sub);
  for (const [name, text] of Object.entries(src)) {
    mkdirSync(dirname(join(repo, name)), { recursive: true });
    writeFileSync(join(repo, name), text);
  }
  const cli = join(base, 'cli.cjs');
  writeFileSync(cli, FAKE, { mode: 0o755 });
  // The build root is the workspace: the engine answers paths relative to it, as the fake does. The graph registry
  // lives in this test's own Octocode home.
  const env = { OCTOCODE_CLI: `${process.execPath} ${cli}`, FAKE_ROOT: root, FAKE_STATE: join(base, 'state.json'), WORKSPACE_ROOT: root, OCTOCODE_HOME: join(base, 'home') };
  Object.assign(process.env, env);
  const out = join(base, 'result');
  const r = spawnSync(process.execPath, [join(here, 'commands', 'build.ts'), root, '--out', out, '--lsp', 'off'], { encoding: 'utf8', cwd: root, env: { ...process.env, ...env } });
  assert.equal(r.status, 0, r.stderr);
  return { base, repo, root, out, env: { ...process.env, ...env }, build: r, builds: () => readFileSync(env.FAKE_STATE, 'utf8').trim().split('\n') };
}

const collect = () => {
  const events = [];
  return { events, emit: (e) => events.push(e), of: (type) => events.filter((e) => e.type === type) };
};

test('a refresh rebuilds only the edited chunk, streams its rows again and writes the new edge', async () => {
  const { root, out, builds } = fakeRepo();
  // lib.ts reads OCTOCODE_CLI when it loads, so the module loads after the fake is in place.
  const { refreshRun } = await import('../src/runtime/refresh.ts');
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

test('the refresh command rebuilds the edited chunk headlessly and continues the saved event sequence', () => {
  const { root, out, builds } = fakeRepo();
  const before = readFileSync(join(out, 'events.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  writeFileSync(join(root, 'pkg/a/b.ts'), 'export function b() {}\nexport function h() { a(); }\n');
  const r = spawnSync(process.execPath, [join(here, 'commands', 'refresh.ts'), out], { encoding: 'utf8', cwd: root, env: process.env });
  assert.equal(r.status, 0, r.stderr);
  const summary = JSON.parse(r.stdout);
  assert.deepEqual([summary.chunks, summary.changed], [['pkg-a'], ['pkg/a/b.ts']]);
  assert.deepEqual(builds().slice(2), ['pkg/a']);
  const after = readFileSync(join(out, 'events.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  const added = after.slice(before.length);
  assert.ok(added.length > 0);
  assert.deepEqual(added.map((e) => e.seq), added.map((_, i) => before.at(-1).seq + 1 + i), 'sequence numbers continue the saved log');
  assert.equal(added.at(-1).type, 'refreshed');
  const saved = JSON.parse(readFileSync(join(out, 'data.json'), 'utf8'));
  assert.ok(saved.calls.some((c) => c.source === 'pkg/a/b.ts#h'));
  // The replaced graph stays as previous.json: diff names the cycle the edit created (a.ts ⇄ b.ts).
  const diff = spawnSync(process.execPath, [join(here, 'commands', 'diff.ts'), out, '--json'], { encoding: 'utf8' });
  assert.equal(diff.status, 0, diff.stderr);
  const d = JSON.parse(diff.stdout);
  assert.deepEqual(d.cycles.map((c) => [c.change, c.after]), [['new', ['pkg/a/a.ts', 'pkg/a/b.ts']]]);
  assert.deepEqual(d.edges.added.map((e) => [e.source, e.target]), [['pkg/a/b.ts', 'pkg/a/a.ts']]);
});

test('the refresh command hands a run a live viewer serves to that viewer and prints its summary', async () => {
  const out = tempDir('graph-refresh-served-');
  const asked = [];
  const server = createServer((req, res) => {
    asked.push(req.url);
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ ok: true, ms: 5, chunks: ['pkg-a'], changed: ['pkg/a/a.ts'] }));
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const url = `http://127.0.0.1:${server.address().port}/`;
  writeFileSync(join(out, 'server.json'), JSON.stringify({ pid: process.pid, url, token: 'tok', port: server.address().port, out, root: null }));
  const child = spawn(process.execPath, [join(here, 'commands', 'refresh.ts'), out], { stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', (d) => (stdout += d));
  child.stderr.on('data', (d) => (stderr += d));
  const [code] = await once(child, 'exit');
  server.close();
  assert.equal(code, 0, stderr);
  assert.deepEqual(asked, ['/refresh?t=tok']);
  const summary = JSON.parse(stdout);
  assert.deepEqual([summary.out, summary.chunks, summary.changed, 'ok' in summary], [out, ['pkg-a'], ['pkg/a/a.ts'], false]);
  assert.equal(spawnSync(process.execPath, [join(here, 'commands', 'refresh.ts'), '--help'], { encoding: 'utf8' }).status, 0);
});

// Files edited after the graph was written: every file is dated two hours back and the graph one hour back, so only
// what a test writes next is newer than the graph (dates set, not waited for).
const ageGraph = (fake) => {
  const at = (hours) => new Date(Date.now() - hours * 3600_000);
  for (const p of readdirSync(fake.repo, { recursive: true }))
    if (!String(p).includes('.octocode')) utimesSync(join(fake.repo, String(p)), at(2), at(2));
  utimesSync(join(fake.out, 'data.json'), at(1), at(1));
};
const queryCli = (fake, ...args) => spawnSync(process.execPath, [join(here, 'commands', 'query.ts'), ...args], { encoding: 'utf8', cwd: fake.base, env: fake.env });

test('a build and its refreshes register the result for its root: query <root> finds it from anywhere', () => {
  const fake = fakeRepo();
  const registry = join(fake.env.OCTOCODE_HOME, 'graph-registry');
  assert.equal(readdirSync(registry, { recursive: true }).filter((p) => String(p).endsWith('.json')).length, 1);
  const r = queryCli(fake, fake.root, 'search b', '--json');
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, new RegExp(`answering from ${fake.out.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} \\(registered for`));
  assert.ok(JSON.parse(r.stdout).rows.some((row) => row.id === 'pkg/a/b.ts'));
});

test('query --refresh refreshes the chunks holding edited files first and answers from the refreshed graph', () => {
  const fake = fakeRepo();
  ageGraph(fake);
  writeFileSync(join(fake.root, 'pkg/a/b.ts'), 'export function b() {}\nexport function h() { a(); }\n');
  // Without --refresh: the answer comes from the graph as written and names the option.
  const stale = queryCli(fake, fake.out, 'search h', '--json');
  const before = JSON.parse(stale.stdout);
  assert.deepEqual(before.staleFiles, ['pkg/a/b.ts']);
  assert.match(before.warnings[0], /--refresh/);
  assert.deepEqual(fake.builds().slice(2), []);
  const r = queryCli(fake, fake.out, 'search h', '--json', '--refresh');
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /refreshed 1 changed file, 0 added files and 0 removed files \(rebuilt chunks: pkg-a\)/);
  assert.deepEqual(fake.builds().slice(2), ['pkg/a'], 'only the edited chunk is built again');
  const after = JSON.parse(r.stdout);
  assert.equal(after.staleFiles, undefined);
  assert.ok(after.rows.some((row) => row.symbol === 'pkg/a/b.ts#h'), 'the answer holds the edit');
  // Unchanged since: --refresh answers directly.
  const again = queryCli(fake, fake.out, 'search h', '--json', '--refresh');
  assert.equal(again.status, 0, again.stderr);
  assert.doesNotMatch(again.stderr, /refreshed/);
  assert.deepEqual(fake.builds().slice(3), []);
  // A file touched without a change: the refresh finds nothing to rebuild and marks the graph checked.
  ageGraph(fake);
  const now = new Date();
  utimesSync(join(fake.root, 'pkg/c/c.ts'), now, now);
  const touched = queryCli(fake, fake.out, 'search c', '--json', '--refresh');
  assert.match(touched.stderr, /rebuilt chunks: none/);
  assert.deepEqual(fake.builds().slice(3), []);
  assert.equal(JSON.parse(touched.stdout).staleFiles, undefined);
});

test('MCP query_graph refresh: true refreshes edited chunks before answering', { timeout: 30000 }, async (t) => {
  const fake = fakeRepo();
  ageGraph(fake);
  writeFileSync(join(fake.root, 'pkg/a/b.ts'), 'export function b() {}\nexport function h() { a(); }\n');
  const child = spawn(process.execPath, [join(here, 'commands', 'mcp.ts')], { cwd: fake.base, env: fake.env, stdio: ['pipe', 'pipe', 'ignore'] });
  t.after(() => child.kill());
  const waiting = new Map();
  createInterface({ input: child.stdout }).on('line', (line) => {
    const m = JSON.parse(line);
    waiting.get(m.id)?.(m);
  });
  const ask = (id, method, params) => new Promise((ok) => {
    waiting.set(id, ok);
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  });
  await ask(1, 'initialize', { protocolVersion: '2025-06-18', capabilities: {} });
  const q = await ask(2, 'tools/call', { name: 'query_graph', arguments: { result: fake.root, query: 'search h', refresh: true, format: 'json' } });
  assert.notEqual(q.result.isError, true, q.result.content[0].text);
  const r = JSON.parse(q.result.content[0].text);
  assert.ok(r.rows.some((row) => row.symbol === 'pkg/a/b.ts#h'), q.result.content[0].text);
  assert.equal(r.staleFiles, undefined);
  assert.deepEqual(fake.builds().slice(2), ['pkg/a']);
});

test('a build rooted inside a workspace names the workspace packages it treated as externals and the build that stitches them', () => {
  const fake = fakeRepo({
    src: {
      'package.json': '{"name":"mono","workspaces":["pkg/*"]}\n',
      'pkg/a/package.json': '{"name":"a"}\n', 'pkg/a/a.ts': "import { c } from 'c';\n",
      'pkg/c/package.json': '{"name":"c"}\n', 'pkg/c/c.ts': 'export function c() {}\n',
    },
    sub: 'pkg/a',
  });
  const note = `1 package of the npm workspace at ${fake.repo} was treated as an external, not stitched: c (1 import); their imports end at a package node. To follow them into those packages, build the workspace root (a larger build): `;
  assert.ok(fake.build.stderr.includes(note), fake.build.stderr);
  assert.match(fake.build.stderr, new RegExp(` build ${fake.repo.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} --lsp off\\n`));
  const D = JSON.parse(readFileSync(join(fake.out, 'data.json'), 'utf8'));
  assert.deepEqual(D.coverage.workspaceExternals.map((w) => [w.root, w.packages.map((p) => [p.id, p.paths, p.imports])]), [[fake.repo, [['pkg:c', ['pkg/c'], 1]]]]);
  assert.ok(readFileSync(join(fake.out, 'brief.txt'), 'utf8').includes(`SCOPE ${note}`));
  assert.ok(existsSync(join(fake.out, 'brief.txt')));
});
