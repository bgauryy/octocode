import assert from 'node:assert/strict';
import test from 'node:test';
import { declarations, nextHints, rowData, sourceLines } from './responses.mjs';

const outlinePage = (symbol, continued) => ({ sc: { results: [{ data: { files: [{ path: 'source.ts', symbols: [symbol] }] }, ...(continued && { rowPart: { part: 2, of: 2, continues: { array: '/data/files', index: 0 } } }) }] } });

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

test('outline continuation retains member evidence and omits only the repeated ancestor context', () => {
  const provenance = { sourceRange: { startByte: 0, endByte: 50 }, selectionByteRange: { startByte: 6, endByte: 12 }, parseAssessment: { status: 'parsed', scope: 'declaration' } };
  const member = { symbolName: 'run', kind: 'method', line: 2, occurrenceId: 'member', sourceDigest: 'digest', ...provenance };
  const context = { symbolName: 'Worker', kind: 'class', line: 1, endLine: 3, sourceDigest: 'digest', ...provenance, members: [member] };
  const full = { ...context, occurrenceId: 'parent', sourceDigest: 'digest', exported: true };
  const seen = new Map();
  assert.deepEqual(declarations(outlinePage(full, false), 0, seen).map(row => row.occurrenceId), ['parent', 'member']);
  assert.deepEqual(declarations(outlinePage(context, true), 0, seen).map(row => row.occurrenceId), ['member']);
  assert.equal(declarations(outlinePage(context, true), 0, seen)[0].parent, 'Worker');
  assert.equal(declarations(outlinePage(context, true), 0, seen)[0].path, 'source.ts');
  assert.deepEqual(declarations(outlinePage(full, true), 0, seen).map(row => row.occurrenceId), ['parent', 'member'], 'actual repeated evidence remains visible');
  assert.equal(declarations(outlinePage(context, false), 0, seen).length, 2, 'an unmarked parent is never silently omitted');
  assert.equal(declarations(outlinePage(context, true)).length, 2, 'orphan context cannot replace missing original evidence');
  assert.equal(declarations(outlinePage({ ...context, sourceDigest: 'changed' }, true), 0, seen).length, 2, 'changed repeated identity remains evidence');
  assert.equal(declarations(outlinePage({ ...context, sourceRange: { startByte: 1, endByte: 50 } }, true), 0, seen).length, 2, 'changed nested context remains evidence');
  assert.equal(declarations(outlinePage({ ...context, newMetadata: { flag: true } }, true), 0, seen).length, 2, 'new nested metadata remains evidence');
  assert.deepEqual(declarations(outlinePage({ ...context, exported: true }, true), 0, seen).map(row => row.occurrenceId), ['member'], 'unchanged declared exported qualifier remains context');
  assert.equal(declarations(outlinePage({ ...context, docStartLine: 1 }, true), 0, seen).length, 2, 'non-context evidence is retained');
});

test('unchanged declared evidence collections never disappear as ancestor context', () => {
  const seen = new Map();
  const parent = { symbolName: 'Worker', kind: 'class', line: 1, sourceDigest: 'digest', exportedAs: ['alias'] };
  const first = declarations(outlinePage({ ...parent, occurrenceId: 'parent', members: [{ symbolName: 'first', kind: 'method', line: 2, occurrenceId: 'first' }] }, false), 0, seen);
  const next = declarations(outlinePage({ ...parent, members: [{ symbolName: 'second', kind: 'method', line: 3, occurrenceId: 'second' }] }, true), 0, seen);
  assert.deepEqual(first.flatMap(row => row.exportedAs ?? []), ['alias']);
  assert.deepEqual(next.flatMap(row => row.exportedAs ?? []), ['alias']);
  assert.deepEqual(next.map(row => row.occurrenceId), [undefined, 'second'], 'repeated alias evidence remains visible for the whole/page comparison');
});
