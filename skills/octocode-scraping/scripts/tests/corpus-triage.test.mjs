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
    const part = `text/${r.pageId}.clean.part-001.md`;
    writeFileSync(join(dir, part), r.body ?? 'x'.repeat(r.bytes ?? 1000));
    return JSON.stringify({
      pageId: r.pageId,
      url: r.url,
      status: 200,
      cleanTextBytes: r.bytes ?? (r.body ?? '').length ?? 1000,
      textParts: [part],
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
  // Stub octocode CLI: answers `tools --scheme jev` and `tools jev --input <f>`
  // with verdicts keyed off each query's file basename.
  stubCli = join(dir, 'stub-cli.mjs');
  writeFileSync(stubCli, `
import { readFileSync } from 'node:fs';
const args = process.argv.slice(2);
if (args[0] === 'tools' && args.includes('--scheme')) { console.log('{"name":"jev"}'); process.exit(0); }
const input = args[args.indexOf('--input') + 1];
const req = JSON.parse(readFileSync(input, 'utf8'));
const results = req.queries.map((q, index) => {
  if (typeof q.reasoning !== 'string' || !q.reasoning.trim()) throw new Error('Missing nonblank Jev reasoning');
  const p = q.context.query.path;
  let choice = 'relevant', confidence = 0.95;
  if (/page-002/.test(p)) { choice = 'unrelated'; confidence = 0.9; }
  if (/page-003/.test(p)) { choice = 'unrelated'; confidence = 0.3; }
  if (/page-004/.test(p)) { choice = 'mention'; confidence = 0.7; }
  return { index, data: { model: 'stub', answer: { type: 'choice', choice, confidence, probabilities: { [choice]: confidence } }, usage: { input_tokens: 100, output_tokens: 5 } } };
});
console.log(JSON.stringify({ results }));
`);
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

test('dry-run composes valid jev batches; dedups URLs; routes thin pages without jev', () => {
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
  assert.ok(req.queries.length >= 1 && req.queries.length <= 5);
  for (const q of req.queries) {
    assert.equal(typeof q.reasoning, 'string');
    assert.match(q.reasoning, /whether.*read/i);
    assert.equal(q.context.tool, 'localFetch');
    assert.ok(q.context.query.path);
    assert.ok(q.context.query.reasoning);
    assert.equal(q.context.query.fullContent, true);
    assert.equal(q.question.type, 'choice');
    assert.ok(q.question.criteria.relevant.what);
    assert.ok(q.question.criteria.mention.not_for);
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
  assert.equal(out.jevUsage.providerInputTokens, 400);
  assert.ok(existsSync(join(dir, 'reports', 'triage', 'triage.json')));
});

test('unavailable jev returns JEV_UNAVAILABLE with lexical fallback hint', () => {
  writeSession([{ pageId: 'page-001', url: 'https://ex.test/a', bytes: 2000 }]);
  const badCli = join(dir, 'bad-cli.mjs');
  writeFileSync(badCli, 'console.error("x Unknown tool: jev"); process.exit(1);');
  const res = run(['--session-dir', dir, '--goal', 'g', '--octocode', `${process.execPath} ${badCli}`]);
  assert.equal(res.status, 1);
  assert.equal(res.parsed.code, 'JEV_UNAVAILABLE');
  assert.match(res.parsed.hint, /corpus-find/);
});

test('rejects missing args and bad session dir', () => {
  assert.equal(run([]).status, 2);
  assert.equal(run(['--session-dir', join(dir, 'nope'), '--goal', 'g']).parsed.code, 'NO_SESSION');
});
