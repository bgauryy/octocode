import test from 'node:test';
import assert from 'node:assert/strict';
import { equal, orderedEqual, scenario } from './advanced-local/helpers.mjs';

test('value equality ignores object property order and preserves nested order and multiplicity', () => {
  const checks = [];
  equal(checks, 'keys', [{ startByte: 3, endByte: 7 }], [{ endByte: 7, startByte: 3 }]);
  equal(checks, 'top-level', [{ id: 2 }, { id: 1 }], [{ id: 1 }, { id: 2 }]);
  equal(checks, 'scalar', [{ id: 2 }], [{ id: 1 }]);
  equal(checks, 'nested-order', [{ items: [2, 1] }], [{ items: [1, 2] }]);
  equal(checks, 'duplicate', [{ id: 1 }, { id: 1 }], [{ id: 1 }]);
  equal(checks, 'missing', [], [{ id: 1 }]);
  orderedEqual(checks, 'ordered-keys', [{ a: 1, b: 2 }], [{ b: 2, a: 1 }]);
  orderedEqual(checks, 'ordered-rows', [{ id: 2 }, { id: 1 }], [{ id: 1 }, { id: 2 }]);
  assert.deepEqual(checks.map(([label, passed]) => [label, passed]), [
    ['keys', true], ['top-level', true], ['scalar', false], ['nested-order', false],
    ['duplicate', false], ['missing', false], ['ordered-keys', true], ['ordered-rows', false],
  ]);
});

test('scenario forwards a source-derived page bound while retaining completion failures', async () => {
  const calls = [];
  const ctx = { explore: async (...args) => {
    calls.push(args);
    return { root: { json: { results: [{ index: 0, data: {} }] } }, responses: [], failures: calls.length === 3 ? [{ check: 'page.cap' }] : [] };
  } };
  const checks = [];
  const options = { label: 'tree', query: { operation: 'syntaxTree' }, checks };
  await scenario(ctx, 'astSearch', 'cli', options);
  await scenario(ctx, 'astSearch', 'cli', { ...options, maxPages: 1002 });
  await scenario(ctx, 'astSearch', 'cli', { ...options, maxPages: 2 });
  assert.deepEqual(calls.map(call => call[3]), [{}, { maxPages: 1002 }, { maxPages: 2 }]);
  assert.deepEqual(checks.map(([, passed]) => passed), [true, true, false]);
});
