import assert from 'node:assert/strict';
import test from 'node:test';
import { contractValidators } from '../contract.mjs';

test('caller validation accepts exact public envelopes and rejects private engine progress', () => {
  const { validateInput } = contractValidators();
  const queries = [{ operation: 'search', ecosystem: 'npm', keywords: ['proof'] }];
  assert.equal(validateInput('artifactSearch', { queries }), null);
  assert.equal(validateInput('artifactSearch', { cursor: 'issued-opaque-handle' }), null);
  for (const state of [{ page: 2 }, { registryCursor: 'private-provider-position' }]) {
    assert.notEqual(validateInput('artifactSearch', { queries: [{ ...queries[0], ...state }] }), null);
  }
  assert.notEqual(validateInput('artifactSearch', { queries, cursor: 'issued-opaque-handle' }), null);
  assert.notEqual(validateInput('artifactSearch', { queries, responseChars: 50_001 }), null);
});

test('caller validation retains nested authored constraints and strict unknown-field rejection', () => {
  const { validateInput } = contractValidators();
  const query = { path: 'source.ts', matchString: ['needle'], contextLines: { before: 0, after: 2 } };
  assert.equal(validateInput('localFetch', { queries: [query] }), null);
  for (const change of [{ contextLines: { before: -1, after: 2 } }, { matchString: 'needle' }, { minify: true }]) {
    assert.notEqual(validateInput('localFetch', { queries: [{ ...query, ...change }] }), null);
  }
});

test('caller validation uses the selected published capability without admitting engine state', () => {
  const query = { path: 'source.ts', classification: { predicates: ['Contains a timeout default?'] } };
  const enabled = contractValidators();
  const disabled = contractValidators({ selection: false });
  assert.equal(enabled.validateInput('localFetch', { queries: [query] }), null);
  assert.notEqual(disabled.validateInput('localFetch', { queries: [query] }), null);
  assert.notEqual(enabled.validateInput('localFetch', { queries: [{ ...query, readProgress: { unit: 'lines', offset: 10 } }] }), null);
});
