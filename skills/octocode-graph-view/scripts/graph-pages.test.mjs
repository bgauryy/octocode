import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { RESPONSE_CHARS, readGraph, scopePaths } from './graph-pages.mjs';
import { requireCompleteToolResponse } from './lib.mjs';
import { tempDir } from './test-tmp.mjs';

// A graphQuery response as the CLI prints it: one result row whose data carries the page and its continuations.
const response = (data) => ({ results: [{ index: 0, data: { graph: { id: 'snapshot' }, ...data } }] });
const more = (cursor) => ({ kind: 'continue', tool: 'graphQuery', input: { cursor } });
// A fake CLI that answers a fresh query with `first` and each continuation cursor from `pages`.
// The runtime serves a row's page walk as the reply's one `next` (it continues every unfinished row): a fixture
// that writes the walk on its row (`next.nextPage`) is served the same way.
const served = (page) => {
  if (!page || page.next || !Array.isArray(page.results)) return page;
  const out = structuredClone(page);
  for (const result of out.results) {
    const walk = result?.data?.next?.nextPage;
    if (!walk) continue;
    delete result.data.next.nextPage;
    if (!Object.keys(result.data.next).length) delete result.data.next;
    out.next ??= walk;
  }
  return out;
};
const fake = (first, pages, received = []) => async (tool, input) => {
  received.push({ tool, input });
  return served(input.cursor ? pages[input.cursor] : first);
};
// One page of `total` (pagination.total) rows in all; `next` present means more pages follow.
const page = (n, total, rows, next) => response({ pagination: { unit: "items", offset: 0, length: rows.length, total }, rows, ...(next ? { next } : {}) });

test('graph pages retain Unicode, long values, nested lists and cycle members exactly once', async () => {
  const long = 'שלום🙂'.repeat(1000);
  const first = response({ pagination: { unit: "items", offset: 0, length: 1, total: 2 }, rows: [{ title: long, nodes: ['a'], nodesTruncated: true }], summary: { witnesses: ['w1'] }, next: { nextPage: more('p2'), cycle0: more('c1'), 'summary.witnesses': more('w1') } });
  const pages = {
    c1: response({ pagination: { unit: "items", offset: 0, length: 1, total: 3 }, rows: ['b'], next: { nextPage: more('c2') } }),
    c2: response({ pagination: { unit: "items", offset: 0, length: 1, total: 3 }, rows: ['c'] }),
    w1: response({ pagination: { unit: "items", offset: 0, length: 1, total: 2 }, rows: ['w2'] }),
    p2: response({ pagination: { unit: "items", offset: 0, length: 1, total: 2 }, rows: [{ title: 'second', nodes: ['d'] }] }),
  };
  const saved = [];
  const received = [];
  const result = await readGraph({ operation: 'cycles', path: 'src' }, { run: fake(first, pages, received), save: (input, raw) => saved.push(structuredClone({ input, page: raw })) });
  assert.equal(result.rows[0].title, long);
  assert.deepEqual(result.rows.map((r) => r.nodes), [['a', 'b', 'c'], ['d']]);
  assert.deepEqual(result.summary.witnesses, ['w1', 'w2']);
  assert.equal(result.next, undefined);
  assert.equal(result.pagination, undefined);
  assert.equal(saved.length, 5);
  assert.equal(saved[0].page.results[0].data.rows[0].nodes.length, 1, 'raw pages are saved before joining');
  assert.ok(received.every((r) => r.tool === 'graphQuery'));
});

test('row paths from the workspace root come back as snapshot keys; the raw page is saved as served', async () => {
  const first = { results: [{ index: 0, data: { graph: { id: 'snapshot', root: 'packages/cli' }, pagination: { unit: 'items', offset: 0, length: 2, total: 3 }, rows: [{ id: 'src/a.ts#f', path: 'packages/cli/src/a.ts', line: 3 }, { path: 'packages/package.json', status: 'changed' }], next: { nextPage: more('p2') } } }] };
  const pages = { p2: { results: [{ index: 0, data: { graph: { id: 'snapshot', root: 'packages/cli' }, pagination: { unit: 'items', offset: 0, length: 1, total: 3 }, rows: [{ from: 'src/b.ts', path: 'packages/cli/src/b.ts', sites: [{ path: 'packages/cli/src/c.ts', line: 1 }] }], related: [{ kind: 'related', tool: 'lspSearch', input: { queries: [{ path: 'packages/cli/src/b.ts' }] } }] } }] } };
  const saved = [];
  const result = await readGraph({ operation: 'symbols', path: 'packages/cli' }, { run: fake(first, pages), save: (input, raw, data) => saved.push({ raw: structuredClone(raw), data }) });
  assert.deepEqual(result.rows.map((r) => r.path), ['src/a.ts', '../package.json', 'src/b.ts']);
  assert.equal(result.rows[2].sites[0].path, 'src/c.ts');
  assert.equal(saved[1].data.related[0].input.queries[0].path, 'packages/cli/src/b.ts', 'calls keep their paths');
  assert.equal(saved[0].raw.results[0].data.rows[0].path, 'packages/cli/src/a.ts', 'evidence is saved as served');
  assert.equal(saved[0].data.rows[0].path, 'src/a.ts');
});

test('a fresh query carries the response budget; continuations go back unchanged', async () => {
  const received = [];
  const first = response({ pagination: { unit: "items", offset: 0, length: 1, total: 2 }, rows: [1], next: { nextPage: more('c') } });
  const result = await readGraph({ operation: 'unresolved', target: ['a.rs', 'b.rs'], reasons: ['not-declared-in-repo'] }, { run: fake(first, { c: response({ pagination: { unit: "items", offset: 0, length: 1, total: 2 }, rows: [2] }) }, received) });
  assert.deepEqual(result.rows, [1, 2]);
  assert.deepEqual(received.map((r) => r.input), [{ queries: [{ operation: 'unresolved', target: ['a.rs', 'b.rs'], reasons: ['not-declared-in-repo'] }], responseChars: RESPONSE_CHARS }, { cursor: 'c' }]);
});

test('a split row and its later page are read through the reply\'s one next', async () => {
  // The first reply splits its row in two parts; the second part's reply carries the call for the row's next page.
  const first = { ...response({ pagination: { unit: "items", offset: 0, length: 1, total: 3 }, rows: [1] }), next: { tool: 'graphQuery', input: { cursor: 'rp' } } };
  const pages = { rp: { ...response({ pagination: { unit: "items", offset: 0, length: 1, total: 3 }, rows: [2] }), next: more('row') }, row: response({ pagination: { unit: "items", offset: 0, length: 1, total: 3 }, rows: [3] }) };
  const received = [];
  first.results[0].rowPart = { part: 1, of: 2 };
  pages.rp.results[0].rowPart = { part: 2, of: 2 };
  assert.deepEqual((await readGraph({ operation: 'hubs' }, { run: fake(first, pages, received), collectionPaths: ['/data/rows'] })).rows, [1, 2, 3]);
  assert.deepEqual(received.slice(1).map((r) => r.input.cursor), ['rp', 'row']);
});

test('missing, repeated, overlapping and failed pages cannot claim completion', async () => {
  await assert.rejects(readGraph({}, { run: async () => page(1, 2, [1]) }), /required continuation/);
  await assert.rejects(readGraph({}, { run: fake(page(1, 3, [1], { nextPage: more('same') }), { same: page(2, 3, [2], { nextPage: more('same') }) }) }), /Repeated/);
  await assert.rejects(readGraph({}, { run: fake(page(1, 2, [1], { nextPage: more('over') }), { over: page(2, 2, [2, 3]) }) }), /repeat rows/);
  await assert.rejects(readGraph({}, { run: async () => ({ results: [{ data: { errorCode: 'notFound', error: 'no snapshot for this exact path' } }] }) }), /notFound/);
  await assert.rejects(readGraph({}, { run: fake(page(1, 1, [{ nodes: ['a'] }], { 'other.list': more('x') }), {}) }), /Unknown graph evidence list/);
});

test('a page in the retired shape (results, total, offset) fails instead of reading as empty', async () => {
  await assert.rejects(readGraph({}, { run: async () => response({ total: 1, offset: 0, results: [1] }) }), /no rows array/);
});

test('partial tool evidence is rejected while optional leads stay optional', () => {
  assert.throws(() => requireCompleteToolResponse({ results: [{ data: { next: { nextPage: { kind: 'continue' } } } }] }), /Incomplete/);
  assert.throws(() => requireCompleteToolResponse({ results: [{ data: { isPartial: true } }] }), /Incomplete/);
  assert.doesNotThrow(() => requireCompleteToolResponse({ related: [{ kind: 'related' }], results: [{ data: { pagination: { hasMore: false } } }] }));
});

test('brief-only rendering retains long evidence and does not write HTML or alter source data', () => {
  const folder = tempDir('graph-brief-');
  const long = `${'🙂'.repeat(2000)}.ts`;
  const source = JSON.stringify({ section: '/repo', coverage: { files: 2, imports: 1, calls: 0 }, files: [{ id: long }, { id: 'b.ts' }], imports: [{ source: 'b.ts', target: long }], symbols: [], calls: [] });
  const data = join(folder, 'data.json');
  writeFileSync(data, source);
  const script = join(dirname(fileURLToPath(import.meta.url)), 'render.mjs');
  const result = spawnSync(process.execPath, [script, '--data', data], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(join(folder, 'report.html')), false);
  assert.ok(readFileSync(join(folder, 'brief-full.txt'), 'utf8').includes(`PAGERANK (weighted, importance flows to dependencies): ${long}`));
  assert.equal(readFileSync(data, 'utf8'), source);
});

const edge = (source, target) => ({ source, target });

test('the brief leads with communities ranked by summed PageRank, before any file ranking', () => {
  const folder = tempDir('graph-brief-communities-');
  // Two triangles joined by one edge: two communities; the one the other depends on holds more PageRank.
  const files = ['a/x.ts', 'a/y.ts', 'a/z.ts', 'b/p.ts', 'b/q.ts', 'b/r.ts'].map((id) => ({ id }));
  const imports = [edge('a/x.ts', 'a/y.ts'), edge('a/y.ts', 'a/z.ts'), edge('a/z.ts', 'a/x.ts'), edge('b/p.ts', 'b/q.ts'), edge('b/q.ts', 'b/r.ts'), edge('b/r.ts', 'b/p.ts'), edge('a/x.ts', 'b/p.ts')];
  const data = join(folder, 'data.json');
  writeFileSync(data, JSON.stringify({ section: '/repo', coverage: { files: 6, imports: imports.length, calls: 0 }, files, imports, symbols: [], calls: [] }));
  const result = spawnSync(process.execPath, [join(dirname(fileURLToPath(import.meta.url)), 'render.mjs'), '--data', data], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const lines = readFileSync(join(folder, 'brief.txt'), 'utf8').split('\n');
  const at = lines.findIndex((l) => l.startsWith('COMMUNITIES by summed PageRank: '));
  assert.ok(at > 0 && at < lines.findIndex((l) => l.startsWith('PAGERANK')), lines.join('\n'));
  const ranks = [...lines[at].matchAll(/PageRank ([\d.e-]+)/g)].map((m) => Number(m[1]));
  assert.equal(ranks.length, 2, lines[at]);
  assert.ok(ranks[0] >= ranks[1], lines[at]);
  assert.match(lines[at], /^COMMUNITIES by summed PageRank: b\b.*top b\//);
});

test('a frozen snapshot retains large source files inside the result workspace', t => {
  if (spawnSync('rsync', ['--version']).status !== 0) { t.skip('rsync is required by the frozen-copy fallback'); return; }
  const folder = tempDir('graph-frozen-');
  const source = '🙂'.repeat(300_000);
  writeFileSync(join(folder, 'large.ts'), source);
  const fakeCli = join(folder, 'cli.mjs');
  writeFileSync(fakeCli, `const [,,tool,,json] = process.argv;
const out = (data) => console.log(JSON.stringify({results:[{index:0,data}]}));
const row = tool === 'schema' ? {} : JSON.parse(json ?? '{}').queries?.[0] ?? {};
if(tool==='schema') console.log(JSON.stringify({querySchema:{properties:{target:{anyOf:[{type:'string'},{type:'array',maxItems:100}]}}}}));
else if(tool==='graphBuild') {
  if(row.path === ${JSON.stringify(folder)}) { out({errorCode:'executionFailed',error:'the tree changed during graph analysis'}); process.exit(5); }
  out({id:'frozen',root:row.path,counts:{nodes:0,edges:0,nodesByKind:{file:1}}});
} else if(tool==='structureSearch') out({files:[{dir:${JSON.stringify(folder)},files:['large.ts']}]});
else out({graph:{id:'frozen'},rows:[],summary:{health:{},byDetector:{}}});`);
  const out = join(folder, '.octocode', 'result');
  const script = join(dirname(fileURLToPath(import.meta.url)), 'build.mjs');
  const result = spawnSync(process.execPath, [script, folder, '--out', out, '--lsp', 'off'], { encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${fakeCli}` } });
  assert.equal(result.status, 0, result.stderr);
  const data = JSON.parse(readFileSync(join(out, 'chunks', 'root', 'data.json'), 'utf8'));
  assert.equal(data.frozen, true);
  assert.ok(data.snapshot.root.startsWith(join(out, 'chunks', 'root', 'tmp', 'frozen-')));
  assert.equal(readFileSync(join(data.snapshot.root, 'large.ts'), 'utf8'), source);
});

test('graph answers restate workspace-relative file references as snapshot keys, ids that are not paths stay', () => {
  const data = {
    graph: { id: 'snapshot', root: 'app' },
    rows: [
      { id: 'app/src/spec.ts', kind: 'file', path: 'app/src/spec.ts' },
      { id: 'src/spec.ts#parse', kind: 'function', path: 'app/src/spec.ts' },
      { id: 'pkg:zod', kind: 'package' },
      { id: '6d872d9fa444', subject: 'app/src/a.ts', evidence: { members: ['app/src/a.ts', 'app/src/b.ts'], witness: [{ from: 'app/src/a.ts', to: 'app/src/b.ts' }], dirs: ['app'], mentionedIn: { a: ['app/docs/README.md:3'] } } },
      { path: 'package.json' },
    ],
    related: [{ input: { target: 'app/src/spec.ts' } }],
  };
  const scoped = scopePaths(structuredClone(data));
  assert.deepEqual(scoped.rows.map((row) => row.id), ['src/spec.ts', 'src/spec.ts#parse', 'pkg:zod', '6d872d9fa444', undefined]);
  assert.equal(scoped.rows[0].path, 'src/spec.ts');
  assert.equal(scoped.rows[3].subject, 'src/a.ts');
  assert.deepEqual(scoped.rows[3].evidence, { members: ['src/a.ts', 'src/b.ts'], witness: [{ from: 'src/a.ts', to: 'src/b.ts' }], dirs: ['.'], mentionedIn: { a: ['docs/README.md:3'] } });
  assert.equal(scoped.rows[4].path, '../package.json');
  assert.deepEqual(scoped.related, data.related);
});
