// V15 and V16: the shared graph cache. A second build of an unchanged tree into a new directory pages no chunk and
// probes no declaration; changing one file, the engine, one option or a config outside the chunks misses exactly the
// keys that hold it, and each miss names the key part that changed. `cache prune` keeps the cache under its bound.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { gunzipSync, gzipSync } from 'node:zlib';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { tempDir, fakeCatalog } from './test-tmp.mjs';

const here = fileURLToPath(new URL('../src/runtime/', import.meta.url));

// A fake engine whose graph follows the files on disk (one declaration per file, `a.ts` calling `b.ts`), with an
// LSP route and a callees answer per declaration. Every paging query and every callees probe appends to the log.
const FAKE = String.raw`#!/usr/bin/env node
${fakeCatalog}
const { createHash } = require('node:crypto');
const { appendFileSync, existsSync, readFileSync, readdirSync, writeFileSync } = require('node:fs');
const { join, relative } = require('node:path');
const [root, log] = [process.env.FAKE_ROOT, process.env.FAKE_LOG];
const a = process.argv.slice(2), print = (v) => console.log(JSON.stringify(v));
const out = (data) => print({ results: [{ index: 0, data }] });
const input = JSON.parse(a.find((x) => x.startsWith('{')) ?? '{}'), row = input.queries?.[0] ?? {};
const list = (dir) => readdirSync(dir, { recursive: true, withFileTypes: true }).filter((d) => d.isFile()).map((d) => relative(dir, join(d.parentPath, d.name))).filter((p) => !p.startsWith('.octocode'));
const code = (dir) => list(dir).filter((p) => p.endsWith('.ts')).toSorted();
function graph(dir) {
  const files = code(dir), W = [], S = [];
  // Two declarations per file: lookups of one file's declarations run concurrently.
  for (const f of files) S.push({ id: f + '#' + f[0], path: f, symbolName: f[0], kind: 'function', line: 1, endLine: 1, exported: true }, { id: f + '#' + f[0] + '2', path: f, symbolName: f[0] + '2', kind: 'function', line: 2, endLine: 2, exported: true });
  if (files.includes('a.ts') && files.includes('b.ts')) W.push({ edge: 'imports', from: 'a.ts', to: 'b.ts', path: 'a.ts', line: 1, confidence: 'high' }, { edge: 'calls', from: 'a.ts#a', to: 'b.ts#b', path: 'a.ts', line: 1, confidence: 'high' });
  return { files, W, S };
}
const page = (rows) => out({ graph: { id: row.graph }, rows });
if (a[0] === 'schema') print({ querySchema: { properties: { target: { anyOf: [{ type: 'string' }, { type: 'array', maxItems: 100 }] } } } });
else if (a[0] === 'lsp-server') print({ routes: [{ extension: '.ts', available: true, server: { command: process.execPath, args: ['--stdio'], source: 'bundled', languageId: 'typescript' } }] });
else if (a[0] === 'structureSearch') { const files = list(root); out({ files: [{ dir: root, files }], summary: { files: files.length } }); }
else if (a[0] === 'lspSearch') {
  if (row.operation === 'documentSymbols') print({ results: [] });
  else {
    appendFileSync(log, input.queries.map((q) => 'callees ' + relative(root, q.path) + '#' + q.symbolName + '\n').join(''));
    // a's callee is b in the sibling b.ts, at its declaration line.
    print({ results: input.queries.map((q, index) => ({ index, data: { files: q.symbolName === 'a' ? [{ path: relative(root, q.path), matches: [{ symbolName: 'b', line: 1, path: relative(root, join(q.path, '..', 'b.ts')), sites: [{ line: 1 }] }] }] : [] } })) });
  }
}
else if (a[0] === 'graphBuild') {
  const files = Object.fromEntries(list(row.path).map((p) => [p, readFileSync(join(row.path, p), 'utf8')]));
  const id = 'snap-' + createHash('sha1').update(row.path + JSON.stringify(files) + JSON.stringify(row.exclude ?? [])).digest('hex').slice(0, 12);
  appendFileSync(log, 'graphBuild ' + relative(root, row.path) + '\n');
  writeFileSync(log + '.' + id + '.json', JSON.stringify(files));
  out({ graph: id, root: row.path, counts: { nodes: code(row.path).length, edges: 1 } });
} else {
  if (row.operation !== 'stale') appendFileSync(log, 'page ' + row.operation + '\n');
  const g = graph(row.path);
  if (row.operation === 'stale') {
    // The files that differ from the snapshot's, as the engine's stale check reports them.
    const before = existsSync(log + '.' + row.graph + '.json') ? JSON.parse(readFileSync(log + '.' + row.graph + '.json', 'utf8')) : {};
    const rows = list(row.path).filter((p) => before[p] !== readFileSync(join(row.path, p), 'utf8')).map((p) => ({ path: relative(root, join(row.path, p)), status: 'changed' }));
    out({ graph: { id: row.graph }, rows, freshness: { fresh: rows.length === 0, analysisCurrent: true } });
  }
  else if (row.operation === 'find') page(g.files.map((id) => ({ id, kind: 'file', language: 'typescript' })));
  else if (row.operation === 'witnesses') page(g.W);
  else if (row.operation === 'symbols') page(g.S.map((s) => ({ ...s, path: relative(root, join(row.path, s.path)) })));
  else out({ graph: { id: row.graph }, rows: [], summary: { health: {}, byDetector: {} } });
}`;

const REPO = {
  'package.json': '{"name":"root","private":true}\n',
  'pkg/a/package.json': '{"name":"a"}\n',
  'pkg/a/a.ts': "import { b } from './b';\nexport function a() { b(); }\n",
  'pkg/a/b.ts': 'export function b() {}\n',
  'pkg/c/package.json': '{"name":"c"}\n',
  'pkg/c/c.ts': 'export function c() {}\n',
};

function setup() {
  const base = tempDir('graph-cache-');
  const root = join(base, 'repo');
  for (const [name, text] of Object.entries(REPO)) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), text);
  }
  const cli = join(base, 'cli.cjs');
  writeFileSync(cli, FAKE, { mode: 0o755 });
  const log = join(base, 'calls.log');
  writeFileSync(log, '');
  const home = join(base, 'home');
  const env = {
    ...process.env,
    OCTOCODE_CLI: `${process.execPath} ${cli}`,
    FAKE_ROOT: root,
    FAKE_LOG: log,
    WORKSPACE_ROOT: root,
    OCTOCODE_HOME: home,
  };
  delete env.OCTOCODE_GRAPH_HOME;
  let n = 0;
  // One build into a new directory; its calls (paging queries, callees probes) and its `cache` events.
  const build = (...flags) => {
    const before = readFileSync(log, 'utf8').split('\n').filter(Boolean).length;
    const out = join(base, `out-${++n}`);
    const r = spawnSync(
      process.execPath,
      [
        join(here, 'commands', 'build.ts'),
        root,
        '--out',
        out,
        '--lsp',
        'all',
        '--explain-cache',
        ...flags,
      ],
      { encoding: 'utf8', cwd: root, env }
    );
    assert.equal(r.status, 0, r.stderr);
    const calls = readFileSync(log, 'utf8')
      .split('\n')
      .filter(Boolean)
      .slice(before);
    const events = readFileSync(join(out, 'events.jsonl'), 'utf8')
      .split('\n')
      .filter(Boolean)
      .map(l => JSON.parse(l));
    const cache = events.filter(e => e.type === 'cache');
    return {
      out,
      stderr: r.stderr,
      summary: JSON.parse(r.stdout),
      data: JSON.parse(readFileSync(join(out, 'data.json'), 'utf8')),
      pages: calls.filter(c => c.startsWith('page ')).length,
      probes: calls
        .filter(c => c.startsWith('callees '))
        .map(c => c.slice(8))
        .toSorted(),
      chunk: id => cache.filter(e => e.scope === 'chunk' && e.id === id).at(-1),
      lsp: file => cache.filter(e => e.scope === 'lsp' && e.id === file).at(-1),
    };
  };
  return { base, root, cli, home, env, build };
}

// What two builds of the same content must agree on (paths and times aside). The LSP pass appends the symbols and
// calls it learns in the order batches finish, so those compare as sets.
const byKey = (rows, key) =>
  rows.toSorted((x, y) => (key(x) < key(y) ? -1 : key(x) > key(y) ? 1 : 0));
const graphOf = d => ({
  files: d.files.map(({ id, kind, language, chunk, roles }) => ({
    id,
    kind,
    language,
    chunk,
    roles,
  })),
  imports: d.imports,
  calls: byKey(d.calls, c => `${c.source}>${c.target}`),
  symbols: byKey(d.symbols, x => x.id),
  externals: d.externals,
  chunks: d.chunks,
});

test('V15: a second build of an unchanged tree pages no chunk and probes no declaration', () => {
  const { build } = setup();
  const first = build();
  assert.ok(first.pages > 0, 'the first build pages its chunks');
  assert.deepEqual(first.probes, [
    'pkg/a/a.ts#a',
    'pkg/a/a.ts#a2',
    'pkg/a/b.ts#b',
    'pkg/a/b.ts#b2',
    'pkg/c/c.ts#c',
    'pkg/c/c.ts#c2',
  ]);
  assert.deepEqual(first.summary.cache.lsp, { hit: 0, miss: 6 });
  assert.deepEqual(first.summary.cache.chunks, { hit: 0, miss: 2 });
  assert.equal(first.chunk('pkg-a').reason.part, 'none');

  const second = build();
  assert.equal(second.pages, 0, 'no chunk is paged again');
  assert.deepEqual(second.probes, [], 'no LSP probe runs');
  assert.deepEqual(second.summary.cache, {
    enabled: true,
    chunks: { hit: 2, miss: 0 },
    lsp: { hit: 6, miss: 0 },
  });
  assert.deepEqual(
    graphOf(second.data),
    graphOf(first.data),
    'data.json equals the first build apart from paths and times'
  );
  assert.equal(second.data.callStats.cached, 6);
  assert.equal(second.data.callStats.probed, 0);
  assert.ok(
    second.data.calls.some(
      c =>
        c.source === 'pkg/a/a.ts#a' &&
        c.target === 'pkg/a/b.ts#b' &&
        c.via === 'both'
    ),
    'the cached answer confirms the call'
  );
  // The restored chunk has its result and evidence in the new directory.
  for (const id of ['pkg-a', 'pkg-c']) {
    assert.ok(existsSync(join(second.out, 'chunks', id, 'data.json.gz')));
    assert.ok(
      existsSync(join(second.out, 'chunks', id, 'evidence', 'ingest.json.gz'))
    );
  }
  assert.match(second.stderr, /cache chunk pkg-a hit/);
});

test('V15: one file, the engine, one option and an outside config each miss exactly the keys that hold them', () => {
  const { root, cli, build } = setup();
  build();
  // One file: its chunk misses and names it; its declaration's probe group and the caller whose answer points into
  // it miss; the other chunk and its declarations hit.
  writeFileSync(
    join(root, 'pkg/a/b.ts'),
    'export function b() { return 1; }\n'
  );
  const file = build();
  assert.equal(file.chunk('pkg-a').state, 'miss');
  assert.deepEqual(file.chunk('pkg-a').reason.part, 'file');
  assert.match(
    file.chunk('pkg-a').reason.detail,
    /^b\.ts [0-9a-f]{12} → [0-9a-f]{12}$/
  );
  assert.equal(file.chunk('pkg-c').state, 'hit');
  assert.deepEqual(
    file.probes,
    ['pkg/a/a.ts#a', 'pkg/a/b.ts#b', 'pkg/a/b.ts#b2'],
    'a.ts#a points into b.ts; a.ts#a2 and c.ts are reused'
  );
  assert.deepEqual(file.summary.cache.lsp, { hit: 3, miss: 3 });
  assert.equal(file.lsp('pkg/a/b.ts').reason.part, 'file');
  assert.equal(file.lsp('pkg/a/a.ts').reason.part, 'callee');
  assert.equal(file.lsp('pkg/c/c.ts').state, 'hit');

  // The engine: a new executable misses every chunk and every answer.
  writeFileSync(cli, `${readFileSync(cli, 'utf8')}\n// engine v2\n`, {
    mode: 0o755,
  });
  const engine = build();
  for (const id of ['pkg-a', 'pkg-c'])
    assert.equal(engine.chunk(id).reason.part, 'engine');
  assert.equal(engine.probes.length, 6);
  assert.equal(engine.lsp('pkg/c/c.ts').reason.part, 'engine');

  // One option that changes output: every chunk's exclude list.
  const option = build('--exclude', 'generated');
  for (const id of ['pkg-a', 'pkg-c'])
    assert.equal(option.chunk(id).reason.part, 'option exclude');
  assert.deepEqual(
    option.probes,
    [],
    'an exclude changes no declaration: the answers stay'
  );
  // Back to the default options: both entries are still there.
  assert.deepEqual(build().summary.cache.chunks, { hit: 2, miss: 0 });

  // A config the engine reads outside every chunk (the root manifest) misses through the entries' inputs.
  writeFileSync(
    join(root, 'package.json'),
    '{"name":"root","private":true,"workspaces":["pkg/*"]}\n'
  );
  const config = build();
  for (const id of ['pkg-a', 'pkg-c']) {
    assert.equal(config.chunk(id).reason.part, 'input');
    assert.match(config.chunk(id).reason.detail, /package\.json/);
  }
  assert.equal(config.lsp('pkg/c/c.ts').reason.part, 'configs');
});

test('V15: --no-cache neither reads nor writes; a corrupt entry is a logged miss that is rewritten', () => {
  const { home, build } = setup();
  const off = build('--no-cache');
  assert.equal(off.summary.cache.enabled, false);
  assert.equal(off.chunk('pkg-a').state, 'off');
  assert.ok(
    !existsSync(join(home, 'graph-cache', 'chunks')),
    'nothing is written'
  );
  build();
  // Corrupt every chunk entry's meta.json and one LSP answer.
  const chunks = join(home, 'graph-cache', 'chunks');
  for (const s of readdirSync(chunks).filter(d => d !== 'sections'))
    for (const k of readdirSync(join(chunks, s)))
      writeFileSync(join(chunks, s, k, 'meta.json'), '{not json');
  const again = build();
  assert.equal(again.chunk('pkg-a').state, 'miss');
  assert.ok(again.pages > 0);
  const notes = readFileSync(join(again.out, 'events.jsonl'), 'utf8')
    .split('\n')
    .filter(Boolean)
    .map(l => JSON.parse(l))
    .filter(e => e.type === 'note');
  assert.ok(
    notes.some(e => /graph cache: corrupt entry/.test(e.text)),
    'the corrupt entry is logged'
  );
  assert.deepEqual(
    build().summary.cache.chunks,
    { hit: 2, miss: 0 },
    'and rewritten'
  );
});

test('V16: cache stats counts entries; prune --max-bytes evicts the least recently used under the bound', () => {
  const { root, env, build } = setup();
  build();
  writeFileSync(
    join(root, 'pkg/c/c.ts'),
    'export function c() { return 2; }\n'
  );
  build();
  const run = (...args) => {
    const r = spawnSync(
      process.execPath,
      [join(here, 'commands', 'cache.ts'), ...args],
      { encoding: 'utf8', env }
    );
    return {
      status: r.status,
      out: r.stdout ? JSON.parse(r.stdout) : null,
      stderr: r.stderr,
    };
  };
  const stats = run('stats').out;
  assert.equal(stats.byKind.chunk.entries, 3, 'pkg-a once, pkg-c twice');
  assert.ok(stats.byKind.lsp.entries >= 3);
  assert.equal(
    stats.entries,
    Object.values(stats.byKind).reduce((a, k) => a + k.entries, 0)
  );
  const bound = Math.floor(stats.bytes / 2);
  const pruned = run('prune', '--max-bytes', String(bound)).out;
  assert.ok(
    pruned.after.bytes <= bound,
    `pruned to ${pruned.after.bytes} <= ${bound}`
  );
  assert.equal(pruned.before.bytes - pruned.removed.bytes, pruned.after.bytes);
  assert.ok(pruned.removed.entries > 0);
  assert.equal(run('stats').out.bytes, pruned.after.bytes);
  assert.equal(run('prune').status, 2, 'prune needs a bound');
  assert.equal(run('prune', '--max-bytes', '0').out.after.entries, 0);
});

test('derived entries round-trip by namespace and parts; a corrupt one is a logged miss', async () => {
  const { getDerived, putDerived } =
    await import('../src/runtime/graph-cache.ts');
  const home = tempDir('graph-cache-derived-');
  const logs = [];
  const opts = { home, log: t => logs.push(t) };
  assert.equal(
    getDerived('package-closure', { name: 'x', version: '1.0.0' }, opts),
    null
  );
  putDerived(
    'package-closure',
    { name: 'x', version: '1.0.0' },
    { bytes: 10 },
    opts
  );
  assert.deepEqual(
    getDerived('package-closure', { version: '1.0.0', name: 'x' }, opts),
    { bytes: 10 },
    'part order does not change the key'
  );
  assert.equal(
    getDerived('package-closure', { name: 'x', version: '1.0.1' }, opts),
    null
  );
  const dir = join(home, 'derived', 'package-closure');
  const [s] = readdirSync(dir);
  const [f] = readdirSync(join(dir, s));
  writeFileSync(join(dir, s, f), gzipSync('{"format":1'));
  assert.equal(
    getDerived('package-closure', { name: 'x', version: '1.0.0' }, opts),
    null
  );
  assert.match(logs[0], /corrupt entry/);
  assert.ok(
    !existsSync(join(dir, s, f)),
    'removed so the next write replaces it'
  );
  assert.ok(gunzipSync);
});

test('V15: a refresh fills the shared cache, so a build of the refreshed tree into a new directory reuses it', () => {
  const { root, env, build } = setup();
  const first = build();
  writeFileSync(
    join(root, 'pkg/c/c.ts'),
    'export function c() { return 3; }\n'
  );
  const r = spawnSync(
    process.execPath,
    [join(here, 'commands', 'refresh.ts'), first.out, '--lsp', 'all'],
    { encoding: 'utf8', cwd: root, env }
  );
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout).chunks, ['pkg-c']);
  const after = build();
  assert.equal(after.pages, 0, 'the chunk the refresh rebuilt is reused');
  assert.deepEqual(after.probes, []);
  assert.deepEqual(after.summary.cache.chunks, { hit: 2, miss: 0 });
});

// The package ships without the config contract, so it spells the setting variables itself; a variable the
// contract adds or renames must reach the settings digest, or a snapshot built under other settings is reused.
test('the settings digest covers every variable the config contract binds to a setting', async () => {
  const { SETTING_ENV } = await import(
    new URL('../src/runtime/graph-cache.ts', import.meta.url).href
  );
  const { CONFIG_FIELDS } = await import('@octocodeai/config');
  const bound = CONFIG_FIELDS.flatMap(field =>
    field.env ? [field.env.name] : []
  );
  assert.ok(bound.length > 0);
  assert.deepEqual([...SETTING_ENV].toSorted(), bound.toSorted());
});
