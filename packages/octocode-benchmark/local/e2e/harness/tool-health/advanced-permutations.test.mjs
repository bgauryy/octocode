import assert from 'node:assert/strict';
import test from 'node:test';
import { contract } from '../contract.mjs';
import { finiteInputDomains } from './input-coverage.mjs';
import { searchExpectation, searchPermutations } from './advanced-permutations.mjs';

test('search matrix covers every compatible combination of the declared axes', () => {
  const domains = finiteInputDomains(contract.tools.find(tool => tool.name === 'localSearch').querySchema);
  const values = name => domains.find(domain => domain.path.join('.') === name).values;
  const matrix = searchPermutations();
  const views = values('view').length - 1 + values('unique').length;
  assert.equal(matrix.length, views * values('regexEngine').length * values('caseMode').length * values('wholeWord').length);
  assert.equal(new Set(matrix.map(query => JSON.stringify(query))).size, matrix.length);
  assert.ok(matrix.every(query => (query.view === 'matchOnly') === Object.hasOwn(query, 'unique')));
});

test('source oracle preserves case, multiplicity, whole words and view selection', () => {
  const source = [['one.txt', 'alpha alpha ALPHA alphabet\nother\n'], ['two.txt', 'empty\n']];
  const query = { view: 'matchOnly', unique: 'count', wholeWord: true, caseMode: 'insensitive' };
  assert.deepEqual(searchExpectation(source, query), [['one.txt', 'alpha', 2], ['one.txt', 'ALPHA', 1]]);
  assert.deepEqual(searchExpectation(source, { ...query, wholeWord: false }), [['one.txt', 'alpha', 3], ['one.txt', 'ALPHA', 1]]);
  assert.deepEqual(searchExpectation(source, { ...query, caseMode: 'sensitive' }), [['one.txt', 'alpha', 2]]);
  assert.deepEqual(searchExpectation(source, { ...query, view: 'filesWithout' }), [['two.txt']]);
  assert.deepEqual(searchExpectation(source, { ...query, view: 'countLines' }), [['one.txt', 1]]);
  assert.deepEqual(searchExpectation(source, { ...query, view: 'countMatches' }), [['one.txt', 3]]);
});
