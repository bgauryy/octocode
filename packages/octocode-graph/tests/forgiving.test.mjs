// Loosely typed queries are read as one exact query, and the answer says how; several readings give candidates.
import assert from 'node:assert/strict';
import test from 'node:test';
import { editDistance, nearest, readQuery } from '../src/runtime/query-forgiving.ts';
import { computeMetrics } from '../src/runtime/metrics.ts';
import { formatResult, runQuery } from '../src/runtime/query.ts';

const OPS = ['search', 'about', 'critical', 'files', 'impact', 'deps', 'dependents', 'path', 'cycle', 'order', 'trace', 'community', 'callers', 'callees', 'top'];
const read = (text) => readQuery(text.split(' '), OPS);

test('op typos, synonyms and questions read as the one op they name', () => {
  for (const [text, as] of [['serach x', 'search x'], ['calers x', 'callers x'], ['dependencies a.ts', 'deps a.ts'], ['cycles a.ts', 'cycle a.ts'], ['who calls x', 'callers x'], ['who imports a.ts', 'dependents a.ts']])
    assert.equal(read(text).tokens.join(' '), as, text);
  assert.match(read('cycles a.ts').notes[0], /"cycles" read as cycle/);
  // An exact op is never rewritten.
  assert.deepEqual(read('deps a.ts').notes, []);
});

test('filler words drop, keywords and free text stay', () => {
  assert.equal(read('callers of x').tokens.join(' '), 'callers x');
  assert.equal(read('path from a.ts to b.ts').tokens.join(' '), 'path a.ts b.ts');
  assert.equal(read('top in 3').tokens.join(' '), 'top inweight 3');
  assert.equal(read('top pr').tokens.join(' '), 'top pagerank');
  assert.equal(read('search run to completion').tokens.join(' '), 'search run to completion');
  // A filler word that is the last argument is the argument itself.
  assert.equal(read('callers to').tokens.join(' '), 'callers to');
  assert.equal(read('critical function').tokens.join(' '), 'critical functions');
});

test('an op close to two ops is not guessed: both come back as queries', () => {
  const r = readQuery(['calles', 'x'], OPS);
  assert.deepEqual(r.candidates?.toSorted(), ['callees x', 'callers x']);
});

test('edit distance counts a transposition once and short words get no tolerance', () => {
  assert.equal(editDistance('runqeury', 'runquery'), 1);
  assert.deepEqual(nearest('abc', ['abd']).best, []);
  assert.deepEqual(nearest('runQeury', ['runQuery', 'runQueries', 'rerun']).best, ['runQuery']);
});

test('a misspelled name with one near match answers, and says how it was read', () => {
  const file = (id) => ({ id, label: id, chunk: 'a' });
  const D = { files: [file('a/lib.ts'), file('a/main.ts')], imports: [{ source: 'a/main.ts', target: 'a/lib.ts', line: 1, lines: [1], confidence: 'high', test: false }], symbols: [{ id: 'a/lib.ts#runQuery', file: 'a/lib.ts', name: 'runQuery', kind: 'function', line: 3 }], calls: [{ source: 'a/main.ts#main', target: 'a/lib.ts#runQuery', line: 2, lines: [2], count: 1, confidence: 'high', via: 'graph', test: false }] };
  computeMetrics(D);
  const r = runQuery(D, 'callers of runQeury');
  assert.equal(r.ok, true, r.error);
  assert.deepEqual(r.readAs, { query: 'callers a/lib.ts#runQuery', notes: ['dropped "of"', 'no name is "runQeury": read as a/lib.ts#runQuery'] });
  assert.match(formatResult(r), /^read as: callers a\/lib\.ts#runQuery/);
  assert.equal(runQuery(D, 'dependents a/libb.ts').rows[0].id, 'a/main.ts');
  assert.equal(runQuery(D, 'about zzzz').ok, false);
});
