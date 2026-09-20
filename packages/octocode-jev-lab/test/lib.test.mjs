import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { prepareExperiment, resolveEndpoint, sendJev, summarize } from '../src/lib.mjs';

test('prepares several files as one structured state without sending local paths', async () => {
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
  assert.deepEqual(prepared.body.state, {
    resources: [
      { id: 'one', content: 'first' },
      { id: 'two', content: 'second' },
    ],
  });
  assert.equal(JSON.stringify(prepared.body).includes(directory), false);
  assert.equal(prepared.receipt.resourceCount, 2);
  assert.equal(prepared.receipt.questionCount, 1);
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
  assert.equal(prepared.body.state, state);
  assert.equal(prepared.body.model, 'jev-test');
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
    usage: { inputTokens: 7, outputTokens: 3 },
  });
});

test('rejects non-root and insecure remote endpoints', () => {
  assert.equal(resolveEndpoint('https://api.typesafe.ai'), 'https://api.typesafe.ai/v1/systemone');
  assert.throws(() => resolveEndpoint('https://api.typesafe.ai/path'));
  assert.throws(() => resolveEndpoint('http://example.com'));
});
