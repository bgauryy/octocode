// node --test harness/acceptance.test.mjs — one red and one green fixture per B1 check.
import test from 'node:test';
import assert from 'node:assert/strict';
import { checkChainFit, checkRowContract, descriptionLint, packedCoordinates, silentOmissions, surfaceBudget, zeroBasedTexts, zeroCoordinates } from './acceptance.mjs';

const inputs = { localFetch: new Set(['path', 'matchString', 'startLine', 'endLine']), lspSearch: new Set(['path', 'symbolName', 'lineHint', 'operation']) };
const published = t => inputs[t] ?? null;
const opts = { inputNames: published, published, toolNames: Object.keys(inputs) };

test('C1 packed coordinates: lsp/ast packed rows fail, "<line>\\t<value>" passes', () => {
  assert.ok(packedCoordinates('433:20 in function x 430-475'));
  assert.ok(packedCoordinates('227-428 function x +'));
  assert.ok(packedCoordinates('fn handle@430 12-20'));
  assert.equal(packedCoordinates('452\tconst x = 1;'), 0);
  assert.equal(packedCoordinates('src/a.ts'), 0);
  assert.equal(packedCoordinates('v1.2-3'), 0);
  assert.equal(packedCoordinates('2026-10-04'), 0);
  assert.equal(packedCoordinates('2026-05-30T23:55:19Z'), 0);
});

test('C1 chain fit: unknown target field and packed row fail; clean row passes', () => {
  const bad = { results: [{ status: 'ok', data: { locations: ['433:20 in function x 430-475'], hints: { read: { tool: 'localFetch', query: { queries: [{ path: 'a', position: 3 }] } } } } }] };
  const reasons = checkChainFit(bad, 'lspSearch', opts);
  assert.ok(reasons.some(r => r.includes('`position`')), reasons.join('\n'));
  assert.ok(reasons.some(r => r.startsWith('packed')), reasons.join('\n'));
  const flat = { results: [{ data: { hints: { read: { tool: 'localFetch', query: { path: 'a' } } } } }] };
  assert.ok(checkChainFit(flat, 'lspSearch', opts).some(r => r.includes('envelope')));
  const good = { results: [{ status: 'ok', data: { locations: [{ path: 'a', line: 433, value: 'x' }], hints: { read: { tool: 'localFetch', query: { queries: [{ path: 'a', startLine: 1 }] } } } } }] };
  assert.deepEqual(checkChainFit(good, 'lspSearch', opts), []);
});

test('C1 hint text lint (X12): unpublished knob without a lead fails; with a lead passes', () => {
  const bad = { results: [{ data: { hints: { text: ['try caseMode:"insensitive"'] } } }] };
  assert.ok(checkChainFit(bad, 'localFetch', opts).some(r => r.includes('caseMode')));
  const good = { results: [{ data: { hints: { text: ['try caseMode:"insensitive"'], retry: { tool: 'localFetch', query: { queries: [{ path: 'a', caseMode: 'insensitive' }] } } } } }] };
  const optsWithCase = { ...opts, inputNames: t => (t === 'localFetch' ? new Set([...inputs.localFetch, 'caseMode']) : published(t)) };
  assert.deepEqual(checkChainFit(good, 'localFetch', optsWithCase), []);
  const named = { results: [{ data: { hints: { text: ['Verify with lspSearch operation:"definition"'] } } }] };
  assert.deepEqual(checkChainFit(named, 'localFetch', opts), []);
});

test('C2 row contract: error row without errorCode fails; full error/empty rows pass', () => {
  const bad = { results: [{ status: 'error', data: { error: 'boom' } }] };
  const reasons = checkRowContract(bad, 'error');
  assert.ok(reasons.some(r => r.includes('errorCode')));
  assert.ok(reasons.some(r => r.includes('recovery')));
  assert.ok(checkRowContract({ results: [{ status: 'error', data: { error: 'x', errorCode: 'transport', retryable: false, hints: { text: ['retry'] } } }] }, 'error').some(r => r.includes('retryable:false')));
  assert.deepEqual(checkRowContract({ results: [{ status: 'error', data: { error: 'x', errorCode: 'pathNotFound', hints: { text: ['check the path'] } } }] }, 'error'), []);
  assert.ok(checkRowContract({ results: [{ status: 'empty', data: {} }] }, 'empty').length);
  assert.deepEqual(checkRowContract({ results: [{ status: 'empty', data: { hints: { text: ['shorter term'] } } }] }, 'empty'), []);
  assert.ok(checkRowContract(undefined, 'error', { isError: true, text: 'Input validation error' }).length);
});

test('C3 one base: 0-based text and column 0 fail; 1-based passes', () => {
  assert.equal(zeroBasedTexts({ lspSearch: { inputSchema: { properties: { position: { description: '0-based UTF-16 position' } } } } }).length, 1);
  assert.deepEqual(zeroBasedTexts({ lspSearch: { description: '1-based line' } }), []);
  assert.deepEqual(zeroCoordinates({ results: [{ data: { matches: [{ line: 3, column: 0 }] } }] }), ['.results[0].data.matches[0].column=0']);
  assert.deepEqual(zeroCoordinates({ results: [{ data: { matches: [{ line: 3, column: 1 }] } }] }), []);
});

test('C4 no silent omission: unlistedNested without next fails; with next or terminalLimit passes', () => {
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { unlistedNested: 53 } } }] }), ['results[0].data.stats.unlistedNested=53']);
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { unlistedNested: 53 }, next: { nested: { tool: 'lspSearch', query: { queries: [{}] } } } } }] }), []);
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { skippedFiles: 2, terminalLimit: true } } }] }), []);
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { skippedFiles: 0 } } }] }), []);
});

test('C5 description lint: neighbor with a field passes; none fails; budget', () => {
  const fields = t => (t === 'localFetch' ? new Set(['matchString']) : new Set());
  assert.deepEqual(descriptionLint('localSearch', 'Read hits with localFetch (matchString).', ['localSearch', 'localFetch'], fields), []);
  assert.deepEqual(descriptionLint('localSearch', 'Find text.', ['localSearch', 'localFetch'], fields), ['names no next tool']);
  assert.equal(descriptionLint('localSearch', 'Then localFetch.', ['localSearch', 'localFetch'], fields).length, 1);
  assert.equal(surfaceBudget({ a: 14_000 }, 1_057).reasons.length, 1);
  assert.deepEqual(surfaceBudget({ a: 13_000 }, 1_000).reasons, []);
  assert.equal(surfaceBudget({ a: 500 }, 0, { quotas: { a: 400 } }).reasons.length, 1);
});
