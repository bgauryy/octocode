import assert from 'node:assert/strict';
import test from 'node:test';
import { scenario } from './advanced-local/helpers.mjs';

test('an empty evidence expectation cannot hide an execution failure', async () => {
  for (const [root, expected] of [
    [{ errorCode: 'notFound', error: 'Missing source' }, false],
    [{ results: [{ index: 0, status: 'error', data: { errorCode: 'providerError', error: 'Provider failed' } }] }, false],
    [{ results: [{ index: 0, status: 'empty', data: {} }] }, true],
  ]) {
    const checks = [];
    const ctx = { explore: async () => ({ failures: [], responses: [{ tool: 'structureSearch', result: { json: root } }], root: { json: root } }) };
    await scenario(ctx, 'structureSearch', 'cli', { label: 'filtered', query: { operation: 'files', path: '.' }, checks });
    assert.equal(checks[0][1], expected);
    if (!expected) assert.match(checks[0][2], /"errors":\[\{"errorCode":"(?:notFound|providerError)","error":"(?:Missing source|Provider failed)"\}\]/);
  }
});
