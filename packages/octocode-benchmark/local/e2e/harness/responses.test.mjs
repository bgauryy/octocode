import assert from 'node:assert/strict';
import test from 'node:test';
import { declarations, joinTextParts, lspLocations, nextHints, rowData, sourceLines } from './responses.mjs';
import { laterPartValue } from './page-context.mjs';

test('string-window parts append at rowPart.text and every value reads exactly once', () => {
  const whole = { index: 0, data: { files: [{ path: 'a', matches: [{ line: 1, text: 'one' }, { line: 2, text: 'abcdefgh' }] }], content: 'xyz' }, status: 'success' };
  const pages = [
    { index: 0, data: { files: [{ path: 'a', matches: [{ line: 1, text: 'one' }, { line: 2, text: 'abc' }] }], content: 'xyz' }, status: 'success', rowPart: { part: 1, of: 3 } },
    { index: 0, data: { files: [{ path: 'a', matches: [{ line: 2, text: 'def' }] }] }, rowPart: { part: 2, of: 3, text: '/data/files/0/matches/1/text', continues: { array: '/data/files/0/matches', index: 1 } } },
    { index: 1, data: { content: 'other' } },
    { index: 0, data: { files: [{ path: 'a', matches: [{ line: 2, text: 'gh' }] }] }, rowPart: { part: 3, of: 3, text: '/data/files/0/matches/1/text', continues: { array: '/data/files/0/matches', index: 1 } } },
  ];
  const joined = joinTextParts(pages);
  assert.equal(joined.length, 2);
  assert.deepEqual(joined[0].data, whole.data);
  assert.deepEqual(joined[1].data, { content: 'other' });
  assert.equal(pages[0].data.files[0].matches[1].text, 'abc', 'the walk itself is not mutated');
  assert.throws(() => joinTextParts([pages[0], { index: 0, data: {}, rowPart: { text: '/data/missing' } }]));
});

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
    next: { nextPage: continued }, related: [related],
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
  const context = { symbolName: 'Worker', kind: 'class', line: 1, sourceDigest: 'digest', ...provenance, members: [member] };
  const full = { ...context, endLine: 3, occurrenceId: 'parent', sourceDigest: 'digest', exported: true };
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

test('per-pointer page context: identity repeats whole, required evidence arrives once, parts join to the row', () => {
  const evidence = { name: 'suite case', suite: 'suite', kind: 'test', framework: 'node', parameterized: false };
  const first = { symbolName: 'Worker', kind: 'class', line: 1, endLine: 9, occurrenceId: 'parent', testIdentity: evidence, members: [{ symbolName: 'run', kind: 'method', line: 2, occurrenceId: 'a' }] };
  const later = (overrides, members = [{ symbolName: 'stop', kind: 'method', line: 5, occurrenceId: 'b' }]) => (
    { symbolName: 'Worker', kind: 'class', line: 1, testIdentity: evidence, ...overrides, members });
  const ids = (symbol, continued, seen) => declarations(outlinePage(symbol, continued), 0, seen).map(row => row.occurrenceId);

  // Joining: the first part's row plus the later part's members equals the single-page row's declarations.
  const whole = { ...first, members: [...first.members, { symbolName: 'stop', kind: 'method', line: 5, occurrenceId: 'b' }] };
  const joined = new Map();
  assert.deepEqual([...ids(first, false, joined), ...ids(later({}), true, joined)], ids(whole, false, new Map()));

  const rejected = (name, overrides) => {
    const seen = new Map();
    ids(first, false, seen);
    assert.equal(ids(later(overrides), true, seen).length, 2, `${name} is evidence, not context`);
  };
  rejected('changed bounded evidence', { testIdentity: { ...evidence, name: 'other' } });
  rejected('a changed identity', { symbolName: 'Other' });
  rejected('a missing identity', { symbolName: undefined });
  rejected('a changed optional identity field', { kind: 'interface' });
  rejected('a changed required-evidence scalar', { line: 4 });
  rejected('first-part-only evidence repeated', { endLine: 9 });
});

test('required evidence on a later part is empty, never a second delivery', () => {
  const symbols = ['data', 'symbols'];
  assert.equal(laterPartValue('astSearch', symbols, [], [{ symbolName: 'a' }]), true, 'a required list arrives as [] on a later part');
  assert.equal(laterPartValue('astSearch', symbols, [{ symbolName: 'a' }], [{ symbolName: 'a' }]), false, 'a list repeated whole is a second delivery');
  assert.equal(laterPartValue('astSearch', [...symbols, '0', 'symbolName'], 'a', 'a'), true, 'identity repeats whole');
  assert.equal(laterPartValue('astSearch', [...symbols, '0', 'symbolName'], '', 'a'), false, 'emptied identity is not identity');
  assert.equal(laterPartValue('astSearch', [...symbols, '0', 'line'], 2, 2), true, 'a required number cannot be empty: whole');
  assert.equal(laterPartValue('astSearch', [...symbols, '0', 'line'], 3, 2), false);
  const path = ['data', 'diagnostics', '0', 'files', '0', 'path'];
  assert.equal(laterPartValue('astSearch', path, '', 'src/a.ts'), true, 'free-text required evidence arrives as ""');
  assert.equal(laterPartValue('astSearch', path, 'src/a.ts', 'src/a.ts'), false, 'free text repeated whole is a second delivery');
});

test('required evidence the output schema forbids empty repeats whole and agrees', () => {
  // clasify sections: `status` is an enum (no empty form), `startLine` a number, `sections` an unbounded list.
  const section = ['data', 'resources', 'r', 'sections', '0'];
  const status = [...section, 'answers', 'q', 'status'];
  assert.equal(laterPartValue('clasify', status, 'answered', 'answered'), true, 'a bounded value has no empty form: whole');
  assert.equal(laterPartValue('clasify', status, '', 'answered'), false, 'an emptied bounded value breaks the schema');
  const start = [...section, 'target', 'lines', 'startLine'];
  assert.equal(laterPartValue('clasify', start, 4, 3), false, 'a repeated bound value must agree');
  const sections = ['data', 'resources', 'r', 'sections'];
  assert.equal(laterPartValue('clasify', sections, [], [{ answers: {} }]), true, 'an unbounded list arrives as []');
  assert.equal(laterPartValue('clasify', sections, [{ answers: {} }], [{ answers: {} }]), false, 'an unbounded list repeated whole is a second delivery');
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

const locations = data => lspLocations({ sc: { results: [{ data }] } });
test('lspLocations gives every location form its path and range, and leaves out call rows', () => {
  const displayRange = { startLine: 3, startCharacter: 5, endLine: 3 };
  const row = { path: 'a.ts', displayRange, content: '3\tx', role: 'call' };
  const { path: _path, ...grouped } = row;
  assert.deepEqual(locations({ files: [{ path: 'a.ts', matches: [grouped] }] }), [row]);
  assert.deepEqual(locations({ matches: [row] }), [row]);
  assert.deepEqual(locations({ files: [{ path: 'a.ts', matches: [{ line: 3, column: 5, value: 'x' }] }] }), [{ path: 'a.ts', displayRange, content: 'x' }]);
  assert.deepEqual(locations({ files: [{ path: 'a.ts', matches: [{ symbolName: 'f', line: 1, sites: [{ line: 2 }] }] }] }), []);
  assert.deepEqual(locations({ files: [{ path: 'a.ts', matchCount: 2, lines: [1, 2] }] }), []);
});
