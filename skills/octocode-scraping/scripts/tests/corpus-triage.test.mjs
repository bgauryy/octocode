import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, '..', 'corpus-triage.mjs');

let dir;
let stubCli;

function writeSession(rows) {
  mkdirSync(join(dir, 'text'), { recursive: true });
  const lines = rows.map((r) => {
    const bodies = r.parts ?? [r.body ?? 'x'.repeat(r.bytes ?? 1000)];
    const parts = bodies.map((body, index) => {
      const part = `text/${r.pageId}.clean.part-${String(index + 1).padStart(3, '0')}.md`;
      writeFileSync(join(dir, part), body);
      return part;
    });
    return JSON.stringify({
      pageId: r.pageId,
      url: r.url,
      status: 200,
      cleanTextBytes: r.bytes ?? bodies.reduce((sum, body) => sum + Buffer.byteLength(body), 0),
      textParts: parts,
    });
  });
  writeFileSync(join(dir, 'sources.jsonl'), `${lines.join('\n')}\n`);
}

function run(args, env = {}) {
  const res = spawnSync(process.execPath, [script, ...args], {
    encoding: 'utf8',
    env: { ...process.env, ...env },
  });
  let parsed = null;
  const jsonText = (res.stdout || '').trim() || (res.stderr || '').trim();
  try { parsed = JSON.parse(jsonText); } catch { /* leave null */ }
  return { ...res, parsed };
}

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'triage-test-'));
  // Stub Octocode CLI: enforces SemanticQuery and nested query → cell → page output.
  stubCli = join(dir, 'stub-cli.mjs');
  writeFileSync(stubCli, `
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const args = process.argv.slice(2);
if (args[0] === 'scheme') {
  assert.deepEqual(args, ['scheme', 'clasify', '--view', 'query', '--compact']);
  console.log('{"name":"clasify"}');
  process.exit(0);
}
const input = args[2];
assert.deepEqual(args, ['clasify', '--input', input, '--compact']);
const req = JSON.parse(readFileSync(input, 'utf8'));
assert.equal(typeof req.id, 'string');
assert.equal(typeof req.reasoning, 'string');
assert.equal(req.questions.length, 1);
assert.equal(req.questions[0].id, 'relevance');
const continued = req.resources.some((resource) => resource.context.query.offset === 50_000);
const hasNext = Boolean(process.env.TEST_SEMANTIC_CONTINUATION) && !continued;
const results = process.env.TEST_SEMANTIC_EMPTY ? [] : req.resources.map((resource) => {
  const p = resource.context.query.path;
  let choice = 'relevant', confidence = 0.95;
  if (/page-002/.test(p)) { choice = 'unrelated'; confidence = 0.9; }
  if (/page-003/.test(p)) { choice = 'unrelated'; confidence = 0.3; }
  if (/page-004/.test(p)) { choice = 'mention'; confidence = 0.7; }
  const context = { source: 'tool', tool: 'localFetch', resultHash: 'a'.repeat(64), coverage: process.env.TEST_SEMANTIC_COVERAGE || 'bounded', ...(process.env.TEST_SEMANTIC_COVERAGE === 'partial' ? { limitations: ['Only a bounded fragment was available.'] } : {}) };
  return { resourceId: resource.id, questionId: 'relevance', coverage: context.coverage === 'partial' || hasNext ? 'partial' : 'complete', pages: [{ pageIndex: continued ? 1 : 0, status: 'success', requestedModel: 'jev', resolvedModel: 'jev-1.0-mini', answer: { type: 'choice', choice, confidence, probabilities: { [choice]: confidence }, rawFutureField: 'preserved' }, context, usage: { input_tokens: 100, output_tokens: 5 } }] };
});
const query = { queryId: req.id, results };
if (hasNext) {
  query.next = { clasify: { ...req, resources: req.resources.map((resource) => ({ ...resource, context: { tool: 'localFetch', query: { path: resource.context.query.path, reasoning: resource.context.query.reasoning, chunkType: 'bytes', offset: 50_000, limit: 30_000, minify: 'none' } } })) } };
}
console.log(JSON.stringify({ queries: [query] }));
`);
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

test('dry-run composes valid clasify matrices; dedups URLs; routes thin pages without assessment', () => {
  writeSession([
    { pageId: 'page-001', url: 'https://ex.test/guide', bytes: 5000 },
    { pageId: 'page-002', url: 'https://ex.test/guide#section', bytes: 5000 },
    { pageId: 'page-003', url: 'https://ex.test/thin', bytes: 300 },
    { pageId: 'page-004', url: 'https://ex.test/other', bytes: 4000 },
  ]);
  const res = run(['--session-dir', dir, '--goal', 'find pricing', '--dry-run']);
  assert.equal(res.status, 0, res.stderr);
  const out = res.parsed;
  assert.equal(out.ok, true);
  assert.equal(out.judged, 2); // page-001 + page-004; dupe and thin routed out
  assert.deepEqual(out.duplicates, [{ pageId: 'page-002', url: 'https://ex.test/guide#section', duplicateOf: 'page-001' }]);
  assert.equal(out.consider.length, 1);
  assert.equal(out.consider[0].choice, 'thin');
  assert.match(out.consider[0].reason, /thin-extraction/);
  const reqPath = join(dir, 'reports', 'triage', 'request-01.json');
  assert.ok(existsSync(reqPath));
  const req = JSON.parse(readFileSync(reqPath, 'utf8'));
  assert.match(req.id, /^triage-\d+$/);
  assert.ok(req.resources.length >= 1 && req.resources.length <= 25);
  assert.equal(req.questions.length, 1);
  assert.equal(req.questions[0].id, 'relevance');
  assert.equal(req.questions[0].question.type, 'choice');
  assert.ok(req.questions[0].question.criteria.relevant.what);
  assert.ok(req.questions[0].question.criteria.mention.not_for);
  for (const resource of req.resources) {
    assert.match(resource.id, /^r\d+$/);
    assert.equal(resource.context.tool, 'localFetch');
    assert.ok(resource.context.query.path);
    assert.ok(resource.context.query.reasoning);
    assert.equal(resource.context.query.fullContent, true);
    assert.equal(resource.maxChars, 80_000);
  }
});

test('verdict routing: relevant→read, confident unrelated→skip, low-confidence unrelated→consider', () => {
  writeSession([
    { pageId: 'page-001', url: 'https://ex.test/keepme', bytes: 5000 },
    { pageId: 'page-002', url: 'https://ex.test/skipme', bytes: 5000 },
    { pageId: 'page-003', url: 'https://ex.test/lowconf', bytes: 5000 },
    { pageId: 'page-004', url: 'https://ex.test/passing', bytes: 5000 },
  ]);
  const res = run(['--session-dir', dir, '--goal', 'g', '--octocode', `${process.execPath} ${stubCli}`]);
  assert.equal(res.status, 0, res.stderr);
  const out = res.parsed;
  assert.equal(out.ok, true);
  assert.deepEqual(out.read.map((r) => r.pageId), ['page-001']);
  assert.deepEqual(out.skip.map((r) => r.pageId), ['page-002']);
  assert.deepEqual(out.consider.map((r) => r.pageId).sort(), ['page-003', 'page-004']);
  assert.equal(out.clasifyUsage.providerInputTokens, 400);
  assert.ok(existsSync(join(dir, 'reports', 'triage', 'triage.json')));
});

test('provider partial coverage retains a confident unrelated candidate despite small complete-looking file metadata', () => {
  writeSession([{ pageId: 'page-002', url: 'https://ex.test/partial', bytes: 5000 }]);
  const res = run(['--session-dir', dir, '--goal', 'g', '--octocode', `${process.execPath} ${stubCli}`], { TEST_SEMANTIC_COVERAGE: 'partial' });
  assert.equal(res.status, 0, res.stderr);
  assert.deepEqual(res.parsed.skip, []);
  assert.deepEqual(res.parsed.consider.map((row) => row.pageId), ['page-002']);
  assert.equal(res.parsed.consider[0].partialCoverage, true);
  const report = JSON.parse(readFileSync(res.parsed.report, 'utf8'));
  assert.deepEqual(report.judgedRows[0].receipts[0], {
    coverage: 'partial',
    limitations: ['Only a bounded fragment was available.'],
  });
});

test('large resources follow next.clasify and preserve every raw page answer', () => {
  writeSession([{
    pageId: 'page-002',
    url: 'https://ex.test/large',
    parts: [`é${'a'.repeat(89_999)}`],
  }]);
  const res = run([
    '--session-dir', dir,
    '--goal', 'g',
    '--limit', '2',
    '--octocode', `${process.execPath} ${stubCli}`,
  ], { TEST_SEMANTIC_CONTINUATION: '1' });
  assert.equal(res.status, 0, res.stderr);
  assert.equal(res.parsed.resources, 1);
  assert.equal(res.parsed.matrixBatches, 1);
  assert.equal(res.parsed.clasifyUsage.calls, 2);
  assert.deepEqual(res.parsed.skip.map((row) => row.pageId), ['page-002']);
  assert.equal(res.parsed.skip[0].resources, 1);
  const report = JSON.parse(readFileSync(res.parsed.report, 'utf8'));
  assert.equal(report.assessmentPages.length, 2);
  assert.deepEqual(report.assessmentPages.map((page) => page.answer.rawFutureField), ['preserved', 'preserved']);
  assert.ok(existsSync(join(dir, 'reports', 'triage', 'request-02.json')));
});

test('relevant partial resources route only to read, never also to consider', () => {
  writeSession([{ pageId: 'page-001', url: 'https://ex.test/relevant', bytes: 5000 }]);
  const res = run(
    ['--session-dir', dir, '--goal', 'g', '--octocode', `${process.execPath} ${stubCli}`],
    { TEST_SEMANTIC_COVERAGE: 'partial' },
  );
  assert.equal(res.status, 0, res.stderr);
  assert.deepEqual(res.parsed.read.map((row) => row.pageId), ['page-001']);
  assert.deepEqual(res.parsed.consider, []);
  assert.equal(res.parsed.read[0].partialCoverage, true);
});

test('explicit files and symlinks cannot escape the session directory', () => {
  writeSession([{ pageId: 'page-001', url: 'https://ex.test/a', bytes: 2000 }]);
  const outside = join(tmpdir(), `triage-outside-${process.pid}.txt`);
  writeFileSync(outside, 'outside');
  try {
    const res = run(['--session-dir', dir, '--goal', 'g', '--files', outside, '--dry-run']);
    assert.equal(res.status, 2);
    assert.equal(res.parsed.code, 'INVALID_RESOURCE_PATH');
  } finally {
    rmSync(outside, { force: true });
  }
});

test('stdout bounds resource errors while the saved report preserves every error', () => {
  writeSession(Array.from({ length: 21 }, (_, index) => ({
    pageId: `page-${String(index + 1).padStart(3, '0')}`,
    url: `https://ex.test/error-${index + 1}`,
    bytes: 1000,
  })));
  const res = run(
    ['--session-dir', dir, '--goal', 'g', '--octocode', `${process.execPath} ${stubCli}`],
    { TEST_SEMANTIC_EMPTY: '1' },
  );
  assert.equal(res.status, 1);
  assert.equal(res.parsed.errorCount, 21);
  assert.equal(res.parsed.errors.length, 20);
  assert.equal(res.parsed.errorsTruncated, true);
  const report = JSON.parse(readFileSync(res.parsed.report, 'utf8'));
  assert.equal(report.errorCount, 21);
  assert.equal(report.errors.length, 21);
  assert.equal(report.errorsTruncated, false);
});

test('unavailable clasify returns CLASIFY_UNAVAILABLE with lexical fallback hint', () => {
  writeSession([{ pageId: 'page-001', url: 'https://ex.test/a', bytes: 2000 }]);
  const badCli = join(dir, 'bad-cli.mjs');
  writeFileSync(badCli, 'console.error("x Unknown tool: clasify"); process.exit(1);');
  const res = run(['--session-dir', dir, '--goal', 'g', '--octocode', `${process.execPath} ${badCli}`]);
  assert.equal(res.status, 1);
  assert.equal(res.parsed.code, 'CLASIFY_UNAVAILABLE');
  assert.match(res.parsed.hint, /corpus-find/);
});

test('schema check uses the current CLI discovery command without judging candidates', () => {
  const res = run(['--check', '--octocode', `${process.execPath} ${stubCli}`]);
  assert.equal(res.status, 0, res.stderr);
  assert.equal(res.parsed.code, 'CLASIFY_SCHEMA_OK');
});

test('rejects missing args and bad session dir', () => {
  assert.equal(run([]).status, 2);
  assert.equal(run(['--session-dir', join(dir, 'nope'), '--goal', 'g']).parsed.code, 'NO_SESSION');
});
