import test from 'node:test';
import assert from 'node:assert/strict';
import { mapDeclared } from './calls.mjs';
import { firstDiff, parityView, transportProblem, transportPagesProblem } from './compare.mjs';

const response = (cursor, line) => ({ results: [{ index: 0, data: {
  path: 'src/a.ts', content: `${line}\tconst answer = 42;`,
  next: { more: { kind: 'continue', tool: 'localFetch', input: { cursor } } },
} }] });
const graphBuildReceipt = (extra = {}) => ({ results: [{ index: 0, data: {
  id: 'snapshot', reused: true, counts: { nodes: 8, edges: 7 }, ...extra,
} }] });
const timingEvidence = totalMs => ({ results: [{ index: 0, data: { content: JSON.stringify({ totalMs }) } }] });
const cloneReceipt = (clonedAt, commitSha = 'a'.repeat(40)) => ({ results: [{ index: 0, data: {
  location: { kind: 'repo', localPath: '/checkout', commitSha, resolvedRef: commitSha, clonedAt },
} }] });
const cloneTimingEvidence = clonedAt => ({ results: [{ index: 0, data: { content: JSON.stringify({ clonedAt }) } }] });

test('surface comparison preserves source evidence while accepting independent issued handles', () => {
  assert.equal(firstDiff(parityView(response('cli', 4), 'localFetch'), parityView(response('mcp', 4), 'localFetch')), null);
  assert.match(firstDiff(parityView(response('cli', 4), 'localFetch'), parityView(response('mcp', 5), 'localFetch')), /content/);
});

test('graph build parity compares snapshot evidence and reuse, and elapsed text stays evidence', () => {
  const reference = parityView(graphBuildReceipt(), 'graphBuild');
  assert.equal(firstDiff(reference, parityView(graphBuildReceipt(), 'graphBuild')), null);
  for (const changed of [{ id: 'other' }, { reused: false }, { counts: { nodes: 9, edges: 7 } }]) {
    assert.ok(firstDiff(reference, parityView(graphBuildReceipt(changed), 'graphBuild')));
  }
  assert.ok(firstDiff(parityView(timingEvidence(9), 'localFetch'), parityView(timingEvidence(5), 'localFetch')));
});

test('metadata normalization never rewrites arbitrary literal evidence', () => {
  const literal = { expiresAt: 'source truth', hints: ['source text'], kind: 'continue', tool: 'localFetch', input: { cursor: 'source code' } };
  const json = { content: literal, expiresAt: 'runtime deadline' };
  const schema = { properties: { content: {}, expiresAt: { type: 'string' } } };
  const normalized = mapDeclared(json, schema, (value, path) => path.at(-1) === 'expiresAt' ? '<deadline>' : value);
  assert.deepEqual(normalized.content, literal);
  assert.equal(normalized.expiresAt, '<deadline>');
});

test('clone parity accepts separate completion times but preserves checkout identity and source text', () => {
  const first = parityView(cloneReceipt('2026-10-30T11:00:00Z'), 'ghCloneRepo');
  assert.equal(firstDiff(first, parityView(cloneReceipt('2026-10-30T11:00:01Z'), 'ghCloneRepo')), null);
  assert.ok(firstDiff(first, parityView(cloneReceipt('2026-10-30T11:00:01Z', 'b'.repeat(40)), 'ghCloneRepo')));
  assert.ok(firstDiff(parityView(cloneTimingEvidence('a'), 'localFetch'), parityView(cloneTimingEvidence('b'), 'localFetch')));
});

test('advanced calls cannot hide a failure behind a valid payload or a successful transport flag', () => {
  const healthy = { results: [{ index: 0, data: { content: '1\tactual evidence' } }] };
  const failed = { error: 'Missing source', errorCode: 'notFound' };
  assert.equal(transportProblem('cli', { json: healthy, status: 0 }), null);
  assert.ok(transportProblem('cli', { json: healthy, status: 5 }));
  assert.equal(transportProblem('cli', { json: failed, status: 3 }), null);
  assert.ok(transportProblem('cli', { json: failed, status: 0 }));
  assert.equal(transportProblem('mcp', { json: failed, isError: true }), null);
  assert.ok(transportProblem('mcp', { json: failed, isError: false }));
  const mixed = { results: [...healthy.results, { index: 1, status: 'error', data: { error: 'Bad row', errorCode: 'invalidInput' } }] };
  assert.equal(transportProblem('cli', { json: mixed, status: 2 }), null);
  assert.equal(transportProblem('mcp', { json: mixed, isError: false }), null);
  assert.ok(transportProblem('mcp', { json: mixed, isError: true }));
});

test('transport flags describe the whole captured batch across error-only and success-only pages', () => {
  const errors = { results: [{ index: 1, status: 'error', data: { errorCode: 'invalidInput', error: 'bad sibling' } }], responsePagination: { page: 1, hasMore: true } };
  const success = { results: [{ index: 0, data: { content: '1\tcomplete evidence' } }], responsePagination: { page: 2, hasMore: false } };
  const pages = [{ json: errors, isError: false, status: 2 }, { json: success, isError: false, status: 0 }];
  for (const surface of ['cli', 'mcp']) {
    for (const page of pages) assert.equal(transportProblem(surface, page), null);
    assert.equal(transportPagesProblem(surface, pages), null);
    assert.ok(transportPagesProblem(surface, pages.map(page => ({ ...page, isError: true, status: 0 }))));
  }
  assert.ok(transportPagesProblem('mcp', [{ json: errors, isError: false }]));
  assert.equal(transportPagesProblem('mcp', [{ json: errors, isError: true }]), null);
  const empty = { ...success, results: [{ index: 0, status: 'empty', data: {} }] };
  assert.equal(transportPagesProblem('cli', [pages[0], { json: empty, status: 1 }]), null);
});
