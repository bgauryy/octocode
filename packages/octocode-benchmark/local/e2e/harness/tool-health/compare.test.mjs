import test from 'node:test';
import assert from 'node:assert/strict';
import { mapDeclared } from './calls.mjs';
import { firstDiff, parityView, transportProblem } from './compare.mjs';

const response = (cursor, line) => ({ results: [{ index: 0, data: {
  path: 'src/a.ts', content: `${line}\tconst answer = 42;`,
  next: { more: { kind: 'continue', tool: 'localFetch', input: { cursor } } },
} }] });

test('surface comparison preserves source evidence while accepting independent issued handles', () => {
  assert.equal(firstDiff(parityView(response('cli', 4), 'localFetch'), parityView(response('mcp', 4), 'localFetch')), null);
  assert.match(firstDiff(parityView(response('cli', 4), 'localFetch'), parityView(response('mcp', 5), 'localFetch')), /content/);
});

test('metadata normalization never rewrites arbitrary literal evidence', () => {
  const literal = { expiresAt: 'source truth', hints: ['source text'], kind: 'continue', tool: 'localFetch', input: { cursor: 'source code' } };
  const json = { content: literal, expiresAt: 'runtime deadline' };
  const schema = { properties: { content: {}, expiresAt: { type: 'string' } } };
  const normalized = mapDeclared(json, schema, (value, path) => path.at(-1) === 'expiresAt' ? '<deadline>' : value);
  assert.deepEqual(normalized.content, literal);
  assert.equal(normalized.expiresAt, '<deadline>');
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
