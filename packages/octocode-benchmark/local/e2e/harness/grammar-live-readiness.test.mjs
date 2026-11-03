import assert from 'node:assert/strict';
import test from 'node:test';
import { createDriver } from './tool-health/driver.mjs';
import { indexingRetry, semanticPagesComplete, settleSemanticQuery } from './grammar-live-readiness.mjs';

const retry = { kind: 'related', tool: 'lspSearch', input: { queries: [{ path: 'build.sbt', operation: 'definition', line: 6 }] } };
const page = data => ({ json: { results: [{ index: 0, data }] } });
const indexing = (related = [retry], partialReasons = ['languageServerIndexing']) => page({ isPartial: true, partialReasons, related });
const ready = page({ files: [{ path: 'build.sbt', matches: [{ displayRange: { startLine: 3, startCharacter: 5 } }] }] });
const continuation = cursor => ({ kind: 'continue', tool: 'lspSearch', input: { cursor } });

test('indexing chooses the emitted retry unchanged after all its response pieces', () => {
  assert.equal(indexingRetry([indexing(), indexing()]), retry);
  assert.equal(semanticPagesComplete([indexing()]), false);
});

test('ordinary empty and complete results never trigger a related reattempt', () => {
  for (const candidate of [page({ category: 'noLocations', related: [retry] }), ready]) {
    assert.equal(indexingRetry([candidate]), null);
    assert.equal(semanticPagesComplete([candidate]), true);
  }
});

test('missing, wrong-target and ambiguous indexing retries leave partial evidence incomplete', () => {
  for (const related of [[], [{ ...retry, tool: 'localFetch' }], [retry, { ...retry, input: { queries: [{ path: 'other.scala' }] } }]]) {
    const candidate = indexing(related);
    assert.equal(indexingRetry([candidate]), null);
    assert.equal(semanticPagesComplete([candidate]), false);
  }
});

test('other partial reasons and errors cannot become successful readiness retries', () => {
  for (const reasons of [[], ['anchorLineError'], ['languageServerIndexing', 'anchorLineError']]) {
    const candidate = indexing([retry], reasons);
    assert.equal(indexingRetry([candidate]), null);
    assert.equal(semanticPagesComplete([candidate]), false);
  }
  const failed = indexing();
  failed.json.results[0].status = 'error';
  assert.equal(indexingRetry([failed]), null);
  assert.equal(semanticPagesComplete([failed]), false);
});

test('repeated indexing attempts retain every receipt and only final semantic evidence', async () => {
  const seed = { queries: [{ path: '/workspace/build.sbt', line: 6 }] };
  const responses = [indexing(), indexing(), ready];
  const calls = [];
  const result = await settleSemanticQuery('lspSearch', seed, async (tool, input) => {
    calls.push({ tool, input });
    return { pages: [responses[calls.length - 1]], failures: [] };
  });
  assert.deepEqual(calls, [{ tool: 'lspSearch', input: seed }, { tool: retry.tool, input: retry.input }, { tool: retry.tool, input: retry.input }]);
  assert.equal(calls[1].input, retry.input);
  assert.equal(calls[2].input, retry.input);
  assert.deepEqual(result.attempts.map(attempt => attempt.pages[0]), responses);
  assert.deepEqual(result.pages, [ready]);
  assert.equal(semanticPagesComplete(result.pages), true);
});

test('a failed attempt is retained and stops without suppressing its diagnostic', async () => {
  const failure = { check: 'contract', detail: 'bad output' };
  const result = await settleSemanticQuery('lspSearch', {}, async () => ({ pages: [indexing()], failures: [failure] }));
  assert.deepEqual(result.failures, [failure]);
  assert.equal(result.attempts.length, 1);
  assert.equal(semanticPagesComplete(result.pages), false);
});

test('required response and tool continuations finish before an indexing reattempt', async () => {
  const callSchema = { properties: { kind: {}, tool: {}, input: {} } };
  const schema = { properties: {
    responsePagination: { properties: { next: callSchema } },
    results: { items: { properties: { data: { properties: {
      next: { properties: { nextPage: callSchema } }, related: { items: callSchema },
    } } } } },
  } };
  const driver = createDriver({ validate: () => null, tools: new Set(['lspSearch']), schemaOf: () => schema });
  const first = indexing();
  first.json.results[0].data.next = { nextPage: continuation('tool') };
  first.json.responsePagination = { next: continuation('response') };
  const events = [];
  const result = await settleSemanticQuery('lspSearch', { seed: true }, async (tool, input) => {
    const pages = [];
    const walked = await driver.explore(tool, input, { surface: 'cli', maxCalls: 1, call: async (_tool, request) => {
      const key = request.seed ? 'seed' : request.cursor || 'retry';
      events.push(key);
      const output = key === 'seed' ? first : key === 'retry' ? ready : indexing();
      pages.push(output);
      return output;
    } });
    return { pages, failures: walked.failures };
  });
  assert.deepEqual(events, ['seed', 'response', 'tool', 'retry']);
  assert.deepEqual(result.failures, []);
  assert.equal(semanticPagesComplete(result.pages), true);
});
