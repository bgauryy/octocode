import assert from 'node:assert/strict';
import test from 'node:test';
import { runRow } from './runner.mjs';

function fixture({ headers = [], problem = 'classification row reached no provider' } = {}) {
  const text = '😀complete evidence\n'.repeat(500);
  const response = { json: { results: [{ index: 0, data: { content: text } }] }, stdout: text, stderr: '', wire: { result: { content: [{ type: 'text', text }] } } };
  const worker = {
    surfaces: ['cli', 'mcp'], context: { providerHeaders: [] },
    call: async () => { worker.context.providerHeaders.push(...headers); return response; },
    driver: { explore: async (tool, input, options) => ({ root: await options.call(tool, input), failures: [] }) },
  };
  const spec = { truth: () => ['selected'], observed: () => ['selected'], check: ctx => { ctx.providerHeaders.splice(0); return problem; } };
  const testCase = { id: 'localSearch#receipt', tool: 'localSearch', input: { queries: [{ matchString: ['needle'] }] }, values: [] };
  return { worker, spec, testCase, response };
}

test('failed rows retain full responses and provider receipts before an oracle consumes them', async () => {
  const f = fixture({ headers: [{ 'anthropic-beta': 'long-context', authorization: 'credential', 'Proxy-Authorization': 'credential', cookie: 'credential', 'set-cookie': ['credential'], 'x-api-key': 'credential' }] });
  const result = await runRow(f.worker, f.spec, f.testCase);
  assert.equal(result.failures.length, 2);
  assert.equal(result.passed.size, 0);
  for (const failure of result.failures) {
    assert.equal(failure.check, 'order');
    assert.deepEqual(failure.observation.pages, [f.response]);
    assert.deepEqual(failure.observation.providerHeaders, [{ 'anthropic-beta': 'long-context' }]);
    assert.equal(JSON.stringify(failure).includes('credential'), false);
  }
  assert.deepEqual(f.worker.context.providerHeaders, []);
});

test('an empty provider receipt is retained as evidence of the same failing verdict', async () => {
  const f = fixture();
  const result = await runRow(f.worker, f.spec, f.testCase);
  assert.equal(result.failures.length, 2);
  for (const failure of result.failures) assert.deepEqual(failure.observation.providerHeaders, []);
});

test('passing rows keep their original verdict and return no diagnostic payload', async () => {
  const f = fixture({ problem: null });
  const result = await runRow(f.worker, f.spec, f.testCase);
  assert.deepEqual(result.failures, []);
  assert.deepEqual([...result.passed], ['cli', 'mcp']);
  assert.equal(Object.hasOwn(result, 'observation'), false);
});
