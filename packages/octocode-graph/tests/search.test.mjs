// Search over a saved graph and its sidecar: BM25F over symbol documents (doc comments, declarations data.json
// leaves out), filters, signature filters, test code after production code, the context map under a token
// budget, and the optional provider rerank. Fixtures are hand-built graphs over source written to a temp directory.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { gzipSync } from 'node:zlib';
import { computeMetrics } from '../src/runtime/metrics.ts';
import { runQuery } from '../src/runtime/query.ts';
import { attachSearchIndex, stem, words } from '../src/runtime/search-index.ts';
import {
  SEARCH_INDEX,
  buildSearchIndex,
  commentAbove,
  docstringBelow,
  leadingComment,
  readSearchIndex,
  withSearchIndex,
} from '../src/runtime/search-sidecar.ts';
import { withProvider, RERANK } from '../src/runtime/search-provider.ts';
import { tempDir } from './test-tmp.mjs';

const SOURCES = {
  'src/net.ts': `// Network helpers for the viewer connection.
import { log } from './log.ts';

/** Sends the request again after a failure, waiting longer each attempt (exponential backoff). */
export function sendWithBackoff(url: string, tries: number): Promise<Response> {
  return fetch(url);
}

export function send(url: string): Promise<Response> {
  log(url);
  return sendWithBackoff(url, 1);
}

/**
 * Parses one frame header.
 */
export function parseHeader(buf: Uint8Array): Header | null {
  return null;
}
`,
  'src/log.ts': `export function log(text: string): void {}
// Never called: only the chunk's declarations list it.
export function rotateLogs(): void {}
`,
  'src/cycles.py': `"""Cycle detection over the import graph."""


@cache
def count_cycles(graph):
    """Count the strongly connected groups of several files."""
    return 0
`,
  'test/net.test.ts': `// Checks sendWithBackoff retries.
export function retriesTest(): void {}
`,
};

const symbol = (id, line, endLine, extra = {}) => ({
  id,
  file: id.split('#')[0],
  name: id.split('#')[1].split('.').pop(),
  kind: 'function',
  line,
  endLine,
  exported: true,
  calls: 0,
  ...extra,
});

// A result directory: data.json (call-graph symbols only), one chunk holding every declaration, and the source.
function fixture({ signatures = true } = {}) {
  const root = tempDir('octocode-graph-search-');
  const out = join(root, '.out');
  for (const [path, text] of Object.entries(SOURCES)) {
    mkdirSync(join(root, path, '..'), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  const sig = (params, returns) => (signatures ? { params, returns } : {});
  const file = (id, language, roles = []) => ({
    id,
    label: id.split('/').pop(),
    group: 'src',
    language,
    roles,
    fanIn: 0,
    fanOut: 0,
    chunk: 'root',
    check: { status: 'ok', bytes: Buffer.byteLength(SOURCES[id]), problems: [] },
  });
  const symbols = [
    symbol('src/net.ts#sendWithBackoff', 5, 7, sig('(url: string, tries: number)', 'Promise<Response>')),
    symbol('src/net.ts#send', 9, 12, sig('(url: string)', 'Promise<Response>')),
    symbol('src/log.ts#log', 1, 1),
    symbol('test/net.test.ts#retriesTest', 2, 2),
  ];
  const D = {
    section: root,
    files: [
      file('src/net.ts', 'typescript'),
      file('src/log.ts', 'typescript'),
      file('src/cycles.py', 'python'),
      file('test/net.test.ts', 'typescript', ['test']),
    ],
    imports: [
      { source: 'src/net.ts', target: 'src/log.ts', line: 2, lines: [2], count: 1, confidence: 'high', resolution: ['static-import'], test: false },
    ],
    symbols,
    calls: [
      { source: 'src/net.ts#send', target: 'src/net.ts#sendWithBackoff', line: 11, lines: [11], count: 1, confidence: 'high', resolution: ['local'], test: false, via: 'graph' },
      { source: 'src/net.ts#send', target: 'src/log.ts#log', line: 10, lines: [10], count: 1, confidence: 'high', resolution: ['import'], test: false, via: 'graph' },
      { source: 'test/net.test.ts#retriesTest', target: 'src/net.ts#sendWithBackoff', line: 2, lines: [2], count: 1, confidence: 'high', resolution: ['import'], test: true, via: 'graph' },
    ],
    chunks: [{ id: 'root', path: '.', files: 4, built: true }],
  };
  computeMetrics(D);
  mkdirSync(join(out, 'chunks', 'root'), { recursive: true });
  const declarations = [
    ...symbols.map(({ calls: _calls, ...d }) => d),
    symbol('src/net.ts#parseHeader', 17, 19, sig('(buf: Uint8Array)', 'Header | null')),
    symbol('src/log.ts#rotateLogs', 3, 3, sig('()', 'void')),
    symbol('src/cycles.py#count_cycles', 5, 7),
  ];
  writeFileSync(join(out, 'chunks', 'root', 'data.json.gz'), gzipSync(JSON.stringify({ declarations })));
  const dataPath = join(out, 'data.json');
  writeFileSync(dataPath, JSON.stringify(D));
  return { root, out, dataPath };
}

// Every sidecar build reads and writes a cache of its own, never the user's Octocode home.
const cache = () => ({ home: tempDir('octocode-graph-search-cache-'), log: () => {} });
const load = dataPath => withSearchIndex(JSON.parse(readFileSync(dataPath, 'utf8')), dataPath, { cache: cache() });
const top = (r, n = 1) => r.rows.slice(0, n).map(x => x.symbol ?? x.id);

test('words split identifiers; the stemmer joins a word and its inflections', () => {
  assert.deepEqual(words('parseArgs HTTPServer snake_case kebab-case v2Beta'), ['parse', 'args', 'http', 'server', 'snake', 'case', 'kebab', 'case', 'v2', 'beta']);
  for (const [a, b] of [['retry', 'retries'], ['count', 'counted'], ['parse', 'parsed'], ['cycle', 'cycles'], ['connect', 'connecting']])
    assert.equal(stem(a), stem(b), `${a} ~ ${b}`);
  assert.notEqual(stem('file'), stem('fil'));
});

test('doc comments: the block above a declaration past decorators, a Python docstring, a file header', () => {
  const ts = SOURCES['src/net.ts'].split('\n');
  assert.equal(commentAbove(ts, 4, 'slash'), 'Sends the request again after a failure, waiting longer each attempt (exponential backoff).');
  assert.equal(commentAbove(ts, 16, 'slash'), 'Parses one frame header.');
  assert.equal(commentAbove(ts, 8, 'slash'), '', 'a blank line ends the comment block');
  const py = SOURCES['src/cycles.py'].split('\n');
  assert.equal(docstringBelow(py, 4), 'Count the strongly connected groups of several files.');
  assert.equal(commentAbove(['/// One.', '#[inline]', 'fn f() {}'], 2, 'slash'), 'One.', 'a Rust attribute sits between');
  assert.equal(leadingComment(py, 'src/cycles.py'), 'Cycle detection over the import graph.');
  assert.equal(leadingComment(ts, 'src/net.ts'), 'Network helpers for the viewer connection.');
});

test('the sidecar: every declaration and doc comment, keyed by the data.json it was built from', () => {
  const { dataPath, root } = fixture();
  const sidecar = buildSearchIndex(dataPath, { cache: false });
  assert.deepEqual(sidecar.symbols.map(s => s.id).toSorted(), ['src/cycles.py#count_cycles', 'src/log.ts#rotateLogs', 'src/net.ts#parseHeader']);
  assert.match(sidecar.docs['src/net.ts#sendWithBackoff'], /exponential backoff/);
  assert.match(sidecar.docs['src/cycles.py#count_cycles'], /strongly connected/);
  assert.deepEqual(sidecar.unread, []);
  assert.deepEqual(readSearchIndex(dataPath), sidecar);
  // Another data.json makes it stale; a source file edited since the graph is not read, and is listed.
  writeFileSync(dataPath, readFileSync(dataPath, 'utf8').replace('"section"', '"mode":"whole","section"'));
  assert.equal(readSearchIndex(dataPath), null);
  writeFileSync(join(root, 'src/log.ts'), `${SOURCES['src/log.ts']}// edited\n`);
  assert.deepEqual(buildSearchIndex(dataPath, { cache: false }).unread, [{ file: 'src/log.ts', reason: 'changed since the graph was written' }]);
});

test('a chunk part comes from the shared cache when its chunk result is unchanged, in any result directory', () => {
  const shared = cache();
  const first = fixture();
  const built = buildSearchIndex(first.dataPath, { cache: shared });
  // Another result directory of the same chunk result: its part is a hit, so no source is read (all of it is gone).
  const again = join(first.root, '.again');
  mkdirSync(join(again, 'chunks', 'root'), { recursive: true });
  writeFileSync(join(again, 'chunks', 'root', 'data.json.gz'), readFileSync(join(first.out, 'chunks', 'root', 'data.json.gz')));
  writeFileSync(join(again, 'data.json'), readFileSync(first.dataPath));
  for (const path of Object.keys(SOURCES)) rmSync(join(first.root, path));
  const reused = buildSearchIndex(join(again, 'data.json'), { cache: shared });
  assert.deepEqual(reused.docs, built.docs);
  assert.deepEqual(reused.unread, []);
  // Without the cache the same build reads the (missing) source.
  assert.equal(buildSearchIndex(join(again, 'data.json'), { cache: cache() }).unread.length, Object.keys(SOURCES).length);
});

test('withSearchIndex builds the sidecar on the first search only, then reads it', () => {
  const { dataPath, out } = fixture();
  const D = load(dataPath);
  runQuery(D, 'callers send');
  assert.equal(existsSync(join(out, SEARCH_INDEX)), false, 'no search, no sidecar');
  const first = runQuery(D, 'search backoff');
  assert.equal(existsSync(join(out, SEARCH_INDEX)), true);
  assert.equal(first.stats.index.sidecar, 'attached');
  assert.deepEqual(runQuery(load(dataPath), 'search backoff').rows, first.rows, 'a second load reads the same answer');
});

test('BM25F finds code by its comment and finds declarations nothing calls; exact names stay first', () => {
  const D = load(fixture().dataPath);
  const intent = runQuery(D, 'search which code retries after a failure');
  assert.equal(top(intent)[0], 'src/net.ts#sendWithBackoff');
  assert.deepEqual([intent.rows[0].match, intent.rows[0].found], ['text', ['text']]);
  assert.ok(intent.rows[0].words.includes('failure'));
  assert.deepEqual(top(runQuery(D, 'search where are dependency cycles counted')), ['src/cycles.py#count_cycles']);
  assert.deepEqual(top(runQuery(D, 'search rotate logs')), ['src/log.ts#rotateLogs'], 'a declaration nothing calls');
  // `send` is held by sendWithBackoff's name and comment, but the exact name ranks first.
  const exact = runQuery(D, 'search send');
  assert.deepEqual([top(exact)[0], exact.rows[0].match], ['src/net.ts#send', 'exact']);
  assert.deepEqual(exact.next.read.input.queries[0].ranges, [{ startLine: 9, endLine: 12 }]);
  // Without a sidecar: names, paths and calls still rank, and no doc comment does.
  const bare = JSON.parse(readFileSync(fixture().dataPath, 'utf8'));
  attachSearchIndex(bare, null);
  assert.equal(runQuery(bare, 'search exponential backoff').stats.index.sidecar, 'none');
  assert.equal(top(runQuery(bare, 'search exponential backoff'))[0], 'src/net.ts#sendWithBackoff', 'by its name word');
  assert.equal(runQuery(bare, 'search failure').rows.length, 0);
});

test('filters by kind, language, role, path and export; an unknown value lists the values the graph has', () => {
  const D = load(fixture().dataPath);
  assert.equal(runQuery(D, 'search net --kind file').rows.every(r => r.kind === 'file'), true);
  assert.deepEqual(runQuery(D, 'search cycles --lang python').rows.map(r => r.id).every(id => id.endsWith('.py')), true);
  assert.deepEqual(top(runQuery(D, 'search retries --role test')), ['test/net.test.ts#retriesTest']);
  assert.equal(runQuery(D, 'search send --path src/log.ts').rows.every(r => r.id === 'src/log.ts'), true);
  assert.equal(runQuery(D, 'search log --exported').rows.every(r => r.exported), true);
  const wrong = runQuery(D, 'search net --kind gadget');
  assert.equal(wrong.ok, false);
  assert.match(wrong.error, /no kind is gadget in this graph; it has .*function/);
  assert.match(runQuery(D, 'search net --role nope').error, /file role/);
  assert.equal(runQuery(D, 'search net --kind').ok, false, 'a filter needs a value');
});

test('test code ranks after production code unless --tests', () => {
  const D = load(fixture().dataPath);
  // The test file's comment holds both words; production code holds one.
  const rows = runQuery(D, 'search checks backoff').rows;
  const firstTest = rows.findIndex(r => r.id.startsWith('test/'));
  assert.ok(firstTest > 0 && rows.slice(firstTest).every(r => r.id.startsWith('test/')), 'every test row after every production row');
  assert.equal(runQuery(D, 'search checks backoff --tests').rows[0].id, 'test/net.test.ts');
});

test('signature filters keep recorded matches only, say how many unrecorded ones they left out, and --unsigned keeps those after the matches', () => {
  const D = load(fixture().dataPath);
  const r = runQuery(D, 'search --returns Response');
  assert.deepEqual(r.rows.map(x => x.symbol).toSorted(), ['src/net.ts#send', 'src/net.ts#sendWithBackoff']);
  assert.ok(r.stats.signatureUnknown > 0);
  assert.match(r.summary, /^2 matches for returns Response \(/);
  assert.match(r.summary, /with no recorded signature left out \(--unsigned keeps them\)/);
  const all = runQuery(D, 'search --returns Response --unsigned');
  const firstUnknown = all.rows.findIndex(x => x.signature === 'unknown');
  assert.ok(firstUnknown >= 2 && all.rows.slice(firstUnknown).every(x => x.signature === 'unknown'));
  assert.equal(all.rows.some(x => x.symbol === 'src/net.ts#parseHeader' || x.kind === 'file'), false, 'mismatches and files drop');
  assert.equal(all.rows.length - firstUnknown, r.stats.signatureUnknown);
  assert.deepEqual(top(runQuery(D, 'search --param Uint8Array --returns Header')), ['src/net.ts#parseHeader']);
  assert.deepEqual(top(runQuery(D, 'search --returns Promise<Response> --param tries')), ['src/net.ts#sendWithBackoff']);
  assert.equal(runQuery(D, 'search --returns Head').rows.some(x => x.symbol === 'src/net.ts#parseHeader'), false, 'a whole word');
  const unsigned = load(fixture({ signatures: false }).dataPath);
  const none = runQuery(unsigned, 'search send --returns Response');
  assert.match(none.warnings.join(' '), /records no parameter or return types/);
  assert.equal(none.rows.every(x => x.signature === 'unknown'), true);
});

test('context: the named file first, rows in rank order within the token budget, and a cursor that walks the rest once', () => {
  const D = load(fixture().dataPath);
  const full = runQuery(D, 'context src/net.ts --tokens 100000');
  assert.equal(full.rows[0].id, 'src/net.ts');
  assert.equal(full.rows[0].role, 'named');
  assert.equal(full.stats.rest, 0);
  assert.equal(full.next, undefined);
  const ranked = full.rows.map(r => r.symbol ?? r.id);
  const seen = [];
  let r = runQuery(D, 'context src/net.ts --tokens 12');
  for (;;) {
    assert.ok(r.stats.tokens <= 12 || r.rows.length === 1, 'within the budget, or one oversized row');
    seen.push(...r.rows.map(x => x.symbol ?? x.id));
    if (!r.next) break;
    assert.match(r.summary, /cut by the budget/);
    r = runQuery(D, r.next.query);
  }
  assert.deepEqual(seen, ranked, 'every row exactly once, in rank order');
  assert.ok(ranked.includes('src/log.ts#log') && !ranked.includes('src/cycles.py#count_cycles'), 'what it uses ranks; what it never touches does not');
  assert.equal(runQuery(D, 'context send').rows[0].symbol, 'src/net.ts#send');
  assert.equal(runQuery(D, 'context src/net.ts --tokens 0').ok, false);
  assert.equal(runQuery(D, 'context').ok, false);
});

test('--provider: reranks the best rows by the provider answer; with no provider nothing changes and nothing is sent', async () => {
  const D = load(fixture().dataPath);
  const asked = runQuery(D, 'search send request --provider');
  assert.equal(asked.stats.provider, 'requested');
  const calls = [];
  const plain = await withProvider(runQuery(D, 'search send request'), async args => calls.push(args));
  assert.deepEqual(calls, [], 'not requested: no call');
  assert.equal(plain.stats.provider, undefined);
  const refused = await withProvider(asked, async () => {
    throw new Error('octocode clasify --json: {"error":"Tool clasify is off: it needs classification.api","errorCode":"missingConfiguration"}');
  });
  assert.deepEqual(refused.rows, asked.rows);
  assert.equal(refused.stats.provider, 'not configured');
  assert.match(refused.warnings.at(-1), /nothing was sent/);
  // The provider scores the last row highest: it moves up, and every judged row names the model.
  let request;
  const n = Math.min(RERANK, asked.rows.length);
  const reranked = await withProvider(asked, async args => {
    request = JSON.parse(args[2]);
    const resources = Object.fromEntries(Array.from({ length: n }, (_, i) => [`r${i + 1}`, { answers: { answers: { score: i === n - 1 ? 3 : 0 } }, usage: { calls: 1 } }]));
    return { results: [{ data: { model: 'test-model', resources } }] };
  });
  assert.equal(args0(request).mainGoal, 'Code search: send request');
  assert.equal(Object.keys(args0(request).resources).length, n);
  assert.equal(reranked.rows.length, asked.rows.length);
  const moved = reranked.rows.find(r => r.movedFrom === n);
  assert.ok(moved && moved.rank < n && moved.found.includes('provider'));
  assert.equal(reranked.rows.slice(0, n).every(r => r.provider?.model === 'test-model'), true);
  assert.deepEqual(reranked.stats.provider, { model: 'test-model', judged: n, moved: reranked.stats.provider.moved, calls: n, inputTokens: 0, outputTokens: 0 });
});

const args0 = request => request.queries[0];
