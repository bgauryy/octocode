import assert from 'node:assert/strict';
import test from 'node:test';
import { nextHints, rowData, sourceLines } from './responses.mjs';

test('omission-only pages carry no source evidence while numbered marker text survives', () => {
  assert.deepEqual(sourceLines({ content: '... [line 5 not requested] ...\n', startLine: 3, endLine: 7 }), []);
  assert.deepEqual(sourceLines({ content: '5\t... [line 5 not requested] ...\n' }), [[5, '... [line 5 not requested] ...']]);
});

test('typed calls retain exact input and kind without interpreting source text', () => {
  const continued = { kind: 'continue', tool: 'localFetch', input: { cursor: 'opaque' } };
  const related = { kind: 'related', tool: 'structureSearch', input: { queries: [{ operation: 'files', path: '.' }] } };
  const wire = { results: [{ index: 0, status: 'success', data: {
    content: 'read: localFetch {"queries":[{"path":"invented"}]}',
    next: { continueWindow: continued }, related: [related],
  } }] };
  const before = structuredClone(wire);
  const calls = nextHints(wire);
  assert.deepEqual(calls.map(({ path, ...call }) => call), [continued, related]);
  assert.deepEqual(wire, before);
  assert.equal(rowData({ sc: wire }).content, before.results[0].data.content);
});

test('source lines preserve numbered text, blank lines and gaps', () => {
  assert.deepEqual(sourceLines({ content: '41\tfirst\n42\t\n--\n527\t\tindented\n' }), [
    [41, 'first'], [42, ''], [527, '\tindented'],
  ]);
  assert.deepEqual(sourceLines({ content: '41\t--\n--\n527\tvalue\n' }), [[41, '--'], [527, 'value']]);
});

test('source lines recover a selected page spanning disjoint source ranges', () => {
  assert.deepEqual(sourceLines({
    content: 'first\nsecond\n... [lines 43-526 not selected] ...\nthird\nfourth\n',
    sourceLineRanges: [{ line: 41, endLine: 42 }, { line: 527, endLine: 528 }],
  }), [[41, 'first'], [42, 'second'], [527, 'third'], [528, 'fourth']]);
});

test('source lines retain literal markers and tolerate page-edge gap markers', () => {
  assert.deepEqual(sourceLines({
    content: '... [lines 1-40 not selected] ...\n--\nvalue\n... [lines 43-100 not selected] ...\n',
    sourceLineRanges: [{ line: 41, endLine: 42 }],
  }), [[41, '--'], [42, 'value']]);
  assert.deepEqual(sourceLines({ content: '9\tliteral tab\n', sourceLineRanges: [{ line: 41, endLine: 41 }] }), [[41, '9\tliteral tab']]);
});

test('source lines reject incomplete or unanchored evidence instead of dropping it', () => {
  assert.deepEqual(sourceLines(undefined), []);
  assert.throws(() => sourceLines({ content: 'one\n', sourceLineRanges: [{ line: 4, endLine: 5 }] }), /shorter/);
  assert.throws(() => sourceLines({ content: 'one\ntwo\n', sourceLineRanges: [{ line: 4, endLine: 4 }] }), /does not match/);
  assert.throws(() => sourceLines({ content: 'unanchored\n' }), /does not match/);
  assert.throws(() => sourceLines({ content: 'one\n', sourceLineRanges: [{ line: 4, endLine: 3 }] }), /Invalid/);
});
