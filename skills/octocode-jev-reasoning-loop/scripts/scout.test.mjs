import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { locateSpans, buildScoutRequest, applyPolicy, runScout } from './scout.mjs';

const LEVELS4 = 4; // default taxonomy length
const DEFAULTS = [
  { level: 'none' }, { level: 'mentions' }, { level: 'imports' }, { level: 'implements' }
];

async function dir(files) {
  const d = await mkdtemp(join(tmpdir(), 'scout-'));
  for (const [n, b] of Object.entries(files)) await writeFile(join(d, n), b);
  return d;
}

test('locateSpans merges all anchor windows with anchors and coverage', async () => {
  const d = await dir({ 'a.txt': ['x', 'hash here', 'y', 'z', 'w', 'q', 'r', 's', 't', 'u', 'digest too', 'v'].join('\n') });
  try {
    const { spans, coverage } = locateSpans('a.txt', ['hash', 'digest'], { rootDir: d, allowedRoots: [d], window: 1 });
    assert.equal(spans.length, 2);
    assert.equal(spans[0].source, 'a.txt:L1-L3');
    assert.equal(spans[1].source, 'a.txt:L10-L12');
    assert.ok(coverage > 0 && coverage <= 1);
  } finally { await rm(d, { recursive: true, force: true }); }
});

test('locateSpans stays sandboxed and redacts', async () => {
  const d = await dir({ 's.txt': 'token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123 hash' });
  try {
    assert.throws(() => locateSpans('../out.txt', ['x'], { rootDir: d, allowedRoots: [d] }), /sandbox|escape/i);
    const { spans } = locateSpans('s.txt', ['hash'], { rootDir: d, allowedRoots: [d] });
    assert.match(spans[0].content, /«redacted-token»/);
  } finally { await rm(d, { recursive: true, force: true }); }
});

test('buildScoutRequest: one shared state, one score question per candidate, structured criteria', () => {
  const located = {
    'a.mjs': { spans: [{ source: 'a.mjs:L1-L2', content: 'code' }], coverage: 0.5, fileChars: 8 },
    'b.mjs': { spans: [], coverage: 0, fileChars: 9 }
  };
  const req = buildScoutRequest({ claim: 'does X', candidates: ['a.mjs', 'b.mjs'] }, located);
  assert.equal(Object.keys(req.questions).length, 2);
  assert.equal(req.questions.a_mjs.type, 'score');
  assert.equal(req.questions.a_mjs.criteria.length, LEVELS4);
  assert.equal(typeof req.questions.a_mjs.instructions, 'object'); // structured, not prose
  assert.equal(req.state.candidates['b.mjs'], 'no anchor matches in this file');
});

test('applyPolicy implements frozen v2 exactly', () => {
  const loc = { spans: [{ source: 's' }] };
  const noLoc = { spans: [] };
  // no evidence -> skip
  assert.equal(applyPolicy({}, noLoc, DEFAULTS).action, 'skip');
  // argmax top -> read
  assert.equal(applyPolicy({ score: 3, probabilities: { 0: 0, 1: 0, 2: 0.1, 3: 0.9 } }, loc, DEFAULTS).action, 'read');
  // P(top) <= 0.25 -> skip
  assert.equal(applyPolicy({ score: 2, probabilities: { 0: 0, 1: 0, 2: 0.9, 3: 0.1 } }, loc, DEFAULTS).action, 'skip');
  // middle mass on top -> gray_read (fail-open)
  assert.equal(applyPolicy({ score: 2.4, probabilities: { 0: 0, 1: 0, 2: 0.55, 3: 0.45 } }, loc, DEFAULTS).action, 'gray_read');
});

test('items mode: pre-fetched rows judged without locate, bounded and redacted', () => {
  const dry = runScout({
    claim: 'which PR addresses flaky retry tests',
    items: [
      { id: 'PR#12', content: 'Fix flaky retry test by pinning timers token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123', source: 'octo/repo#12' },
      { id: 'PR#9', content: 'Update README badges' }
    ]
  }, { dryRun: true });
  assert.equal(dry.status, 'dry-run');
  assert.equal(dry.request.questions.PR_12.type, 'score');
  assert.match(JSON.stringify(dry.request.state.candidates['PR#12']), /«redacted-token»/);
  assert.equal(dry.candidates['PR#9'].coverage, 1);
  // exactly one of items|candidates
  assert.throws(() => runScout({ claim: 'x', items: [{ id: 'a', content: 'b' }, { id: 'c', content: 'd' }], candidates: ['x', 'y'] }), /exactly one/);
});

test('multi-dimension: shared state, per-dimension questions, veto only demotes reads', () => {
  const dims = [
    { key: 'relevance', role: 'primary', levels: [{ level: 'no' }, { level: 'maybe' }, { level: 'yes' }] },
    { key: 'is_fix', role: 'veto', levels: [{ level: 'not_a_fix' }, { level: 'fix' }] }
  ];
  const dry = runScout({
    claim: 'which PR fixes X', dimensions: dims,
    items: [{ id: 'a', content: 'fixes X properly' }, { id: 'b', content: 'docs update' }]
  }, { dryRun: true });
  assert.deepEqual(Object.keys(dry.request.questions).sort(), ['a__is_fix', 'a__relevance', 'b__is_fix', 'b__relevance']);
  // exactly one primary enforced
  assert.throws(() => runScout({
    claim: 'x', dimensions: [{ key: 'a', role: 'veto', levels: dims[0].levels }],
    items: [{ id: 'a', content: 'y' }, { id: 'b', content: 'z' }]
  }, { dryRun: true }), /exactly one primary/);
  // question budget enforced
  assert.throws(() => runScout({
    claim: 'x',
    dimensions: [1, 2, 3, 4].map(n => ({ key: `d${n}`, role: n === 1 ? 'primary' : 'info', levels: dims[0].levels })),
    items: Array.from({ length: 7 }, (_, i) => ({ id: `i${i}`, content: 'c' }))
  }, { dryRun: true }), /at most 24 questions/);
});

test('runScout validates input and supports dry-run without any Jev call', async () => {
  const d = await dir({ 'a.mjs': 'const h = createHash("sha256")', 'b.mjs': 'export const x = 1' });
  try {
    assert.throws(() => runScout({ claim: 'x', anchors: ['y'], candidates: ['only-one'] }), /2\.\.12/);
    assert.throws(() => runScout({ anchors: ['y'], candidates: ['a', 'b'] }), /claim/);
    const dry = runScout({ claim: 'computes a hash', anchors: ['createHash'], candidates: ['a.mjs', 'b.mjs'], root: d }, { dryRun: true });
    assert.equal(dry.status, 'dry-run');
    assert.equal(dry.request.questions.a_mjs.type, 'score');
    assert.deepEqual(dry.candidates['a.mjs'].spans, ['a.mjs:L1-L1']);
    assert.equal(dry.candidates['b.mjs'].coverage, 0);
  } finally { await rm(d, { recursive: true, force: true }); }
});
