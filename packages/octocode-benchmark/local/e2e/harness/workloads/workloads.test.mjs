import assert from 'node:assert/strict';
import test from 'node:test';
import { frequentIdentifier } from './cases.mjs';
import { parseResourceReport } from './server.mjs';
import { emitted } from './workflows.mjs';

test('the search term is the most frequent long identifier, ties by name', () => {
  assert.equal(frequentIdentifier('fn short() { longerName(); longerName(); anotherOne(); anotherOne(); }'), 'anotherOne');
  assert.equal(frequentIdentifier('a b c'), undefined);
});

test('a chain follows only typed calls of the requested kind and tool', () => {
  const response = { results: [{ data: { related: [{ kind: 'related', tool: 'localFetch', input: { queries: [1] } }, { kind: 'related', tool: 'lspSearch', input: { queries: [2] } }], next: { kind: 'continue', tool: 'localFetch', input: { cursor: 'c' } } } }] };
  assert.deepEqual(emitted(response, 'related', 'localFetch'), [{ queries: [1] }]);
  assert.deepEqual(emitted(response, 'continue', 'localFetch'), [{ cursor: 'c' }]);
  assert.deepEqual(emitted(response, 'related', 'astSearch'), []);
});

test('the resource report yields the peak in KiB', () => {
  const report = process.platform === 'darwin'
    ? '        0.87 real\n            37830656  maximum resident set size\n            18301576  peak memory footprint\n'
    : '\tMaximum resident set size (kbytes): 36944\n';
  const { peakRssKb, peakFootprintKb } = parseResourceReport(report);
  assert.equal(peakRssKb, 36944);
  assert.equal(peakFootprintKb, process.platform === 'darwin' ? 17873 : null);
});
