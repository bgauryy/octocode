import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import {
  prepareExperiment,
  resolveEndpoint,
  runExperiment,
  sendJev,
  summarize,
} from '../src/lib.mjs';

test('prepares several files as resource-major matrix requests without local paths', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'jev-lab-'));
  await writeFile(join(directory, 'one.txt'), 'first');
  await writeFile(join(directory, 'two.txt'), 'second');
  const prepared = await prepareExperiment(
    {
      resources: [
        { id: 'one', path: 'one.txt' },
        { id: 'two', path: 'two.txt' },
      ],
      questions: { useful: { type: 'noul', instructions: 'Is this useful?' } },
    },
    join(directory, 'request.json'),
    {},
  );
  assert.deepEqual(
    prepared.requests.map(request => request.body.state),
    [
      { resource: { id: 'one', content: 'first' } },
      { resource: { id: 'two', content: 'second' } },
    ],
  );
  assert.equal(JSON.stringify(prepared.requests).includes(directory), false);
  assert.equal(prepared.receipt.mode, 'matrix');
  assert.equal(prepared.receipt.resourceCount, 2);
  assert.equal(prepared.receipt.questionCount, 1);
  assert.equal(prepared.receipt.logicalCells, 2);
});

test('combines resources only when explicitly requested', async () => {
  const prepared = await prepareExperiment(
    {
      resourceMode: 'combined',
      resources: [
        { id: 'one', value: 'first' },
        { id: 'two', value: 'second' },
      ],
      questions: { useful: { type: 'noul', instructions: 'Is this useful?' } },
    },
    '/tmp/request.json',
    {},
  );
  assert.deepEqual(prepared.requests, [
    {
      body: {
        model: 'jev-latest',
        state: {
          resources: [
            { id: 'one', content: 'first' },
            { id: 'two', content: 'second' },
          ],
        },
        questions: { useful: { type: 'noul', instructions: 'Is this useful?' } },
      },
    },
  ]);
  assert.equal(prepared.receipt.mode, 'combined');
  assert.equal(prepared.receipt.providerCallsPerPass, 1);
});

test('preserves a raw state value', async () => {
  const state = { facts: ['a', 'b'] };
  const prepared = await prepareExperiment(
    {
      state,
      model: 'jev-test',
      questions: { route: { type: 'choice', instructions: 'Choose', criteria: { a: null } } },
    },
    '/tmp/request.json',
    {},
  );
  assert.equal(prepared.requests[0].body.state, state);
  assert.equal(prepared.requests[0].body.model, 'jev-test');
});

test('keeps the provider JSON unchanged in the sample response', async () => {
  const provider = {
    model: 'jev-test',
    answers: { match: { type: 'noul', noul: 0.75 } },
    usage: { input_tokens: 10, output_tokens: 4 },
  };
  const server = createServer((request, response) => {
    assert.equal(request.headers.authorization, 'Bearer test-key');
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(provider));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  after(() => server.close());
  const address = server.address();
  const sample = await sendJev(
    { model: 'jev-test', state: 'x', questions: {} },
    { key: 'test-key', baseUrl: `http://127.0.0.1:${address.port}` },
  );
  assert.deepEqual(sample.response, provider);
  assert.equal(sample.ok, true);
});

test('summarizes latency and usage without rewriting samples', () => {
  const samples = [
    { ok: true, elapsedMs: 10, response: { usage: { input_tokens: 3, output_tokens: 1 } } },
    { ok: true, elapsedMs: 20, response: { usage: { input_tokens: 4, output_tokens: 2 } } },
    { ok: false, elapsedMs: 5, response: {} },
  ];
  assert.deepEqual(summarize(samples), {
    samples: 3,
    successes: 2,
    failures: 1,
    latencyMs: { min: 10, median: 10, p95: 20, max: 20 },
    usage: {
      inputTokens: 7,
      outputTokens: 3,
      reportedSamples: 2,
      missingSamples: 0,
    },
    resolvedModels: [],
  });
});

test('keeps aggregate usage unknown when a successful sample omits usage', () => {
  const samples = [
    {
      ok: true,
      elapsedMs: 10,
      response: {
        model: 'jev-1.13.0',
        usage: { input_tokens: 3, output_tokens: 1 },
      },
    },
    { ok: true, elapsedMs: 20, response: { model: 'jev-1.13.0' } },
  ];
  assert.deepEqual(summarize(samples).usage, {
    inputTokens: null,
    outputTokens: null,
    reportedSamples: 1,
    missingSamples: 1,
  });
  assert.deepEqual(summarize(samples).resolvedModels, ['jev-1.13.0']);
});

test('preserves mixed native answers and reports requested and resolved models adjacently', async t => {
  const provider = {
    model: 'jev-1.13.0',
    answers: {
      supported: { type: 'noul', noul: 0.81, provider_note: 'future-noul-field' },
      route: {
        type: 'choice',
        choice: 'keep',
        probabilities: { keep: 0.72, revise: 0.28 },
        confidence: 0.44,
        future_choice_metric: 17,
      },
      quality: {
        type: 'score',
        score: 1.6,
        probabilities: { '0': 0.1, '1': 0.2, '2': 0.7 },
        legend: { '0': 'low', '1': 'medium', '2': 'high' },
        confidence: 0.61,
        future_score_detail: { calibration: 'provider-owned' },
      },
    },
    usage: { input_tokens: 31, output_tokens: 9 },
    future_top_level: { trace: 'provider-owned' },
  };
  const server = createServer((_request, response) => {
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(provider));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const address = server.address();
  const prepared = await prepareExperiment(
    {
      state: { source: 'bounded fixture' },
      model: 'jev-latest',
      questions: {
        supported: { type: 'noul', instructions: 'Supported?' },
        route: {
          type: 'choice',
          instructions: 'Route?',
          criteria: { keep: null, revise: null },
        },
        quality: {
          type: 'score',
          instructions: 'Quality?',
          criteria: ['low', 'medium', 'high'],
        },
      },
    },
    '/tmp/request.json',
    {},
  );
  const result = await runExperiment(prepared, {
    key: 'test-key',
    baseUrl: `http://127.0.0.1:${address.port}`,
  });
  assert.equal(result.request.requestedModel, 'jev-latest');
  assert.equal(Object.hasOwn(result.request, 'model'), false);
  assert.deepEqual(result.summary.resolvedModels, ['jev-1.13.0']);
  assert.deepEqual(result.samples[0].response, provider);
});

test('rejects non-root and insecure remote endpoints', () => {
  assert.equal(resolveEndpoint('https://api.typesafe.ai'), 'https://api.typesafe.ai/v1/systemone');
  assert.throws(() => resolveEndpoint('https://api.typesafe.ai/path'));
  assert.throws(() => resolveEndpoint('http://example.com'));
});
