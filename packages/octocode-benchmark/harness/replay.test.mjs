import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { ENVELOPE_KEYS, evidenceOf, missingFrom, rowsOf } from './evidence.mjs';
import { runPath } from './lib.mjs';
import { HOME, compareCall, extractCalls, observe } from './replay.mjs';
import { roundIn, scratch, storeRun } from './testing.mjs';

const SCHEMA = { collectionPaths: ['/data/files', '/data/files/*/matches'], collectionRefs: [] };
const lines = (from, to) => Array.from({ length: to - from + 1 }, (_, i) => `${from + i}\tline ${from + i}`).join('\n');

/** A stand-in CLI: `pages` are the replies of one walk, linked by cursors. */
function fakeRun(pages) {
  return ([, json]) => {
    const input = JSON.parse(json);
    const at = input.cursor === undefined ? 0 : Number(input.cursor);
    const reply = { ...pages[at], ...(at + 1 < pages.length ? { next: { tool: 'localFetch', input: { cursor: String(at + 1) } } } : {}) };
    return { status: 0, stdout: JSON.stringify(reply), stderr: '' };
  };
}
const page = (content, offset, extra = {}) => ({ results: [{ index: 0, data: { content, totalLines: 6, pagination: { unit: 'lines', offset, length: content.split('\n').length }, ...extra } }] });
const INPUT = { queries: [{ path: 'src/a.rs' }] };
const observed = pages => observe({ tool: 'localFetch', input: INPUT, run: fakeRun(pages), schema: SCHEMA });
const compare = (base, cand, declared) => compareCall({ tool: 'localFetch', declared, observeOn: side => observed(side === 'base' ? base : cand) });

test('evidence is the lines, hits and values of the rows, not their envelope', () => {
  const rows = [{ index: 0, next: { read: 'x' }, data: { commitSha: 'abc', content: '1\ta\n2\tb\nunnumbered', hints: ['try'], pagination: { offset: 0 }, files: [{ path: 'f.ts', matches: [{ line: 3, value: 'x()', role: 'call' }], patch: '+a\n+a' }] } }];
  const items = [...evidenceOf(rows, { queries: [{ path: 'q.ts' }] }).keys()];
  assert.ok(items.includes('q0 line q.ts:1\ta') && items.includes('q0 line q.ts:2\tb'));
  assert.ok(items.includes('q0 match f.ts:3'));
  assert.ok(items.includes('q0 field /commitSha = "abc"'));
  assert.equal(items.filter(item => item.startsWith('q0 patch f.ts +a#')).length, 2, 'a repeated patch line counts each time');
  assert.ok(items.some(item => item.includes('unnumbered')));
  for (const key of ENVELOPE_KEYS) assert.ok(!items.some(item => item.includes(`/${key}`)), key);
  assert.equal(evidenceOf([{ index: 0, data: { errorCode: 'notFound', error: 'no' } }]).size, 0);
});

test('a hit in string form holds the evidence of the same hit in object form, and may gain text but not lose it', () => {
  const objects = evidenceOf([{ index: 0, data: { files: [{ path: 'a.ts', matches: [{ line: 149, value: 'function getRouteRegex(' }] }] } }]);
  const strings = evidenceOf([{ index: 0, data: { files: [{ path: 'a.ts', matches: ['149\tfunction getRouteRegex('] }] } }]);
  const longer = evidenceOf([{ index: 0, data: { files: [{ path: 'a.ts', matches: ['149\tfunction getRouteRegex( (end 172)'] }] } }]);
  const shorter = evidenceOf([{ index: 0, data: { files: [{ path: 'a.ts', matches: ['149\tfunction'] }] } }]);
  assert.deepEqual(missingFrom(objects, strings), []);
  assert.deepEqual(missingFrom(strings, objects), []);
  assert.deepEqual(missingFrom(objects, longer), []);
  assert.equal(missingFrom(objects, shorter).length, 1);
  const stated = evidenceOf([{ index: 0, data: { files: [{ path: 'a.ts', matches: [{ line: 149, value: 'f(', role: 'call' }] }] } }]);
  assert.equal(missingFrom(stated, evidenceOf([{ index: 0, data: { files: [{ path: 'a.ts', matches: ['149\tf('] }] } }])).length, 1, 'what a hit states beside its text is evidence');
});

test('shared facts of a reply count for each row', () => {
  const rows = rowsOf({ shared: { data: { commitSha: 'abc' } }, results: [{ index: 0, data: { content: '1\ta' } }, { index: 1, data: { content: '1\tb', commitSha: 'def' } }] });
  const items = evidenceOf(rows);
  assert.ok(items.has('q0 field /commitSha = "abc"') && items.has('q1 field /commitSha = "def"'));
});

test('one build against itself is the same; smaller pages that still reach every line pass the reachable check', () => {
  const one = [page(lines(1, 6), 0)];
  assert.equal(compare(one, one).status, 'same');
  const paged = compare(one, [page(lines(1, 3), 0), page(lines(4, 6), 3)]);
  assert.deepEqual(paged.lost, []);
  assert.equal(paged.cand.pages, 2);
});

test('a candidate that drops one line fails, and the lost line is named', () => {
  const dropped = [1, 2, 3, 5, 6].map(n => `${n}\tline ${n}`).join('\n');
  const result = compare([page(lines(1, 6), 0)], [page(dropped, 0)]);
  assert.equal(result.status, 'lost');
  assert.deepEqual(result.lost, ['q0 line src/a.rs:4\tline 4']);
  assert.deepEqual(result.firstReplyLost, result.lost);
});

test('evidence moved behind a next fails the first-reply check unless the change is declared', () => {
  const base = [page(lines(1, 6), 0)];
  const cand = [page(lines(1, 3), 0), page(lines(4, 6), 3)];
  const moved = compare(base, cand);
  assert.equal(moved.status, 'lost');
  assert.deepEqual(moved.lost, []);
  assert.equal(moved.firstReplyLost.length, 3);
  const declared = compare(base, cand, [{ tool: 'localFetch', scope: 'firstReply', pattern: '^q0 line src/a\\.rs:', reason: 'smaller first page' }]);
  assert.equal(declared.status, 'same');
  assert.equal(declared.declared.length, 3);
  const wrongScope = compare(base, [page(lines(1, 3), 0)], [{ scope: 'firstReply', pattern: '.', reason: 'smaller first page' }]);
  assert.equal(wrongScope.status, 'lost', 'a first-reply declaration does not cover evidence that is unreachable');
});

test('what the base no longer returns on a second run is unstable, not lost', () => {
  let baseRuns = 0;
  const result = compareCall({
    tool: 'localFetch',
    observeOn: side => {
      if (side === 'cand') return observed([page(lines(1, 5), 0)]);
      baseRuns += 1;
      return observed([page(lines(1, baseRuns === 1 ? 6 : 5), 0)]);
    },
  });
  assert.equal(result.status, 'unstable');
  assert.deepEqual(result.lost, []);
  assert.equal(result.unstable.length, 1);
});

test('a candidate whose walk breaks the continuation contract fails; a call that fails on the base compares nothing', () => {
  const loop = [page(lines(1, 3), 0), { ...page(lines(1, 3), 0) }];
  assert.equal(compare([page(lines(1, 3), 0)], loop).status, 'lost');
  assert.ok(compare([page(lines(1, 3), 0)], loop).contract.length > 0);
  const refused = [{ errorCode: 'invalidInput', errorClass: 'correctInput', error: 'bad field' }];
  assert.equal(compare(refused, [page(lines(1, 3), 0)]).status, 'baseFailed');
  assert.equal(compare([page(lines(1, 3), 0)], refused).status, 'lost');
});

test('stored calls are taken once, without cursor calls, with the session home made neutral', () => {
  const { dir, cleanup } = scratch('extract');
  try {
    const manifest = roundIn(dir, { ids: ['Q1', 'Q2'] });
    const call = (name, input, reply = '{"results":[]}') => ({ name: `mcp__octocode__${name}`, input, reply });
    for (const id of manifest.sets.s.ids) {
      const run = runPath(manifest, 1, 's', id, 'tool');
      const home = path.join(dir, 'homes', id);
      storeRun(run, {
        home,
        steps: [
          { calls: [call('ghCloneRepo', { queries: [{ owner: 'o', repo: 'r' }] }, JSON.stringify({ results: [{ index: 0, data: { location: { path: `${home}/clones/o/r` } } }] })), call('localSearch', { queries: [{ repo: 'r', owner: 'o' }] })] },
          { calls: [call('localFetch', { queries: [{ path: `${home}/clones/o/r/a.ts` }] }), call('localFetch', { cursor: 'c1' })] },
          {},
        ],
      });
      storeRun(runPath(manifest, 1, 's', id, 'gh'), { steps: [{ calls: [{ name: 'Bash', input: { command: 'gh api x' }, reply: 'x' }] }, {}] });
    }
    const { calls, cursors } = extractCalls(manifest, ['tool']);
    assert.equal(cursors, manifest.sets.s.ids.length);
    assert.deepEqual(calls.map(one => one.tool), ['ghCloneRepo', 'localSearch', 'localFetch'], 'the same call in two sessions is one call');
    for (const one of calls) assert.equal(one.from.length, manifest.sets.s.ids.length);
    assert.deepEqual(calls.map(one => one.producer), [true, false, false]);
    assert.equal(calls[2].input.queries[0].path, `${HOME}/clones/o/r/a.ts`);
    assert.deepEqual(extractCalls(manifest, ['gh']).calls, [], 'a shell arm has no tool call to replay');
  } finally {
    cleanup();
  }
});
