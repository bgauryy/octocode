import assert from 'node:assert/strict';
import test from 'node:test';
import { createDriver } from './tool-health/driver.mjs';
import {
  indexingRetry,
  semanticPagesComplete,
  settleSemanticQuery,
  stillIndexing,
} from './grammar-live-readiness.mjs';

const retry = {
  kind: 'related',
  tool: 'lspSearch',
  input: { queries: [{ path: 'build.sbt', operation: 'definition', line: 6 }] },
};
const page = data => ({ json: { results: [{ index: 0, data }] } });
const indexing = (
  related = [retry],
  partialReasons = ['languageServerIndexing']
) => page({ isPartial: true, partialReasons, related });
const ready = page({
  files: [
    {
      path: 'build.sbt',
      matches: [{ displayRange: { startLine: 3, startCharacter: 5 } }],
    },
  ],
});
const continuation = cursor => ({
  kind: 'continue',
  tool: 'lspSearch',
  input: { cursor },
});

test('indexing chooses the emitted retry unchanged after all its response pieces', () => {
  assert.equal(indexingRetry([indexing(), indexing()]), retry);
  assert.equal(semanticPagesComplete([indexing()]), false);
});

test('ordinary empty and complete results never trigger a related reattempt', () => {
  for (const candidate of [
    page({ category: 'noLocations', related: [retry] }),
    ready,
  ]) {
    assert.equal(indexingRetry([candidate]), null);
    assert.equal(semanticPagesComplete([candidate]), true);
  }
});

test('missing, wrong-target and ambiguous indexing retries leave partial evidence incomplete', () => {
  for (const related of [
    [],
    [{ ...retry, tool: 'localFetch' }],
    [retry, { ...retry, input: { queries: [{ path: 'other.scala' }] } }],
  ]) {
    const candidate = indexing(related);
    assert.equal(indexingRetry([candidate]), null);
    assert.equal(semanticPagesComplete([candidate]), false);
  }
});

test('other partial reasons and errors cannot become successful readiness retries', () => {
  for (const reasons of [
    [],
    ['anchorLineError'],
    ['languageServerIndexing', 'anchorLineError'],
  ]) {
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
  const result = await settleSemanticQuery(
    'lspSearch',
    seed,
    async (tool, input) => {
      calls.push({ tool, input });
      return { pages: [responses[calls.length - 1]], failures: [] };
    }
  );
  assert.deepEqual(calls, [
    { tool: 'lspSearch', input: seed },
    { tool: retry.tool, input: retry.input },
    { tool: retry.tool, input: retry.input },
  ]);
  assert.equal(calls[1].input, retry.input);
  assert.equal(calls[2].input, retry.input);
  assert.deepEqual(
    result.attempts.map(attempt => attempt.pages[0]),
    responses
  );
  assert.deepEqual(result.pages, [ready]);
  assert.equal(semanticPagesComplete(result.pages), true);
});

test('a failed attempt is retained and stops without suppressing its diagnostic', async () => {
  const failure = { check: 'contract', detail: 'bad output' };
  const result = await settleSemanticQuery('lspSearch', {}, async () => ({
    pages: [indexing()],
    failures: [failure],
  }));
  assert.deepEqual(result.failures, [failure]);
  assert.equal(result.attempts.length, 1);
  assert.equal(semanticPagesComplete(result.pages), false);
});

test('required response and tool continuations finish before an indexing reattempt', async () => {
  const callSchema = { properties: { kind: {}, tool: {}, input: {} } };
  const schema = {
    properties: {
      responsePagination: { properties: { next: callSchema } },
      results: {
        items: {
          properties: {
            data: {
              properties: {
                next: { properties: { nextPage: callSchema } },
                related: { items: callSchema },
              },
            },
          },
        },
      },
    },
  };
  const driver = createDriver({
    validate: () => null,
    tools: new Set(['lspSearch']),
    schemaOf: () => schema,
  });
  const first = indexing();
  first.json.results[0].data.next = { nextPage: continuation('tool') };
  first.json.responsePagination = { next: continuation('response') };
  const events = [];
  const result = await settleSemanticQuery(
    'lspSearch',
    { seed: true },
    async (tool, input) => {
      const pages = [];
      const walked = await driver.explore(tool, input, {
        surface: 'cli',
        maxCalls: 1,
        call: async (_tool, request) => {
          const key = request.seed ? 'seed' : request.cursor || 'retry';
          events.push(key);
          const output =
            key === 'seed' ? first : key === 'retry' ? ready : indexing();
          pages.push(output);
          return output;
        },
      });
      return { pages, failures: walked.failures };
    }
  );
  assert.deepEqual(events, ['seed', 'response', 'tool', 'retry']);
  assert.deepEqual(result.failures, []);
  assert.equal(semanticPagesComplete(result.pages), true);
});

test('only a row whose sole reason is indexing counts as still indexing', () => {
  assert.equal(stillIndexing([indexing([])]), true);
  assert.equal(stillIndexing([ready, indexing([])]), true);
  for (const candidate of [
    ready,
    indexing([], []),
    indexing([], ['noCompileDatabase']),
    indexing([], ['serverUnanswered']),
    indexing([], ['languageServerIndexing', 'serverUnanswered']),
  ]) {
    assert.equal(stillIndexing([candidate]), false, JSON.stringify(candidate));
  }
  const failed = indexing([]);
  failed.json.results[0].status = 'error';
  assert.equal(stillIndexing([failed]), false);
  assert.equal(stillIndexing([]), false);
});

const answering = responses => {
  const calls = [];
  return {
    calls,
    explore: async (tool, input) => {
      calls.push({ tool, input });
      return { pages: [responses[calls.length - 1]], failures: [] };
    },
  };
};

test('a row with no related call asks the same question again only when the caller opts in', async () => {
  const seed = { queries: [{ path: 'main.c', line: 5 }] };
  const opted = answering([indexing([]), indexing([]), ready]);
  const settled = await settleSemanticQuery('lspSearch', seed, opted.explore, {
    reaskUntil: Infinity,
  });
  assert.equal(opted.calls.length, 3);
  assert.ok(opted.calls.every(call => call.input === seed));
  assert.deepEqual(settled.pages, [ready]);
  const plain = answering([indexing([]), ready]);
  const unchanged = await settleSemanticQuery('lspSearch', seed, plain.explore);
  assert.equal(plain.calls.length, 1);
  assert.equal(semanticPagesComplete(unchanged.pages), false);
});

test('another partial reason ends the wait and the limit leaves the row incomplete', async () => {
  let asked = 0;
  const unanswered = await settleSemanticQuery(
    'lspSearch',
    {},
    async () => {
      asked += 1;
      return { pages: [indexing([], ['serverUnanswered'])], failures: [] };
    },
    { reaskUntil: Infinity }
  );
  assert.equal(asked, 1);
  assert.equal(semanticPagesComplete(unanswered.pages), false);
  let clock = 0;
  const expired = await settleSemanticQuery(
    'lspSearch',
    {},
    async () => ({ pages: [indexing([])], failures: [] }),
    { reaskUntil: 2, now: () => clock++ }
  );
  assert.equal(expired.attempts.length, 3);
  assert.equal(semanticPagesComplete(expired.pages), false);
});

test('the caller deadline also bounds emitted indexing retries without hiding partial evidence', async () => {
  let asked = 0;
  const result = await settleSemanticQuery('lspSearch', {}, async () => {
    asked += 1;
    return { pages: [indexing(asked === 1 ? [retry] : [])], failures: [] };
  }, { reaskUntil: 0, now: () => 0 });
  assert.equal(asked, 1, 'an emitted related call cannot bypass the caller deadline');
  assert.equal(result.attempts.length, 1);
  assert.equal(semanticPagesComplete(result.pages), false);
});
