import assert from 'node:assert/strict';
import test from 'node:test';
import { historyFileRows } from './history-evidence.mjs';
import { createDriver } from './driver.mjs';
import { contract } from '../contract.mjs';

const tool = 'ghGetHistoryItem';
const schema = contract.tools.find(entry => entry.name === tool).outputSchema;
const response = (patch, offset, next, extra = {}) => ({ results: [{ index: 0, data: { pullRequests: [{
  number: 4, owner: 'owner', repo: 'repo', headSha: 'head', baseSha: 'base',
  files: [{ path: 'huge.ts', stat: 'A +2 -0', patch, ...(offset !== undefined && { patchPagination: { offset } }), ...extra }],
  ...(next && { next: { nextPage: { kind: 'continue', tool, input: { cursor: 'tail' } } } }),
}] } }] });

test('history pagination distinguishes repeated file context from duplicated numbered evidence', async () => {
  const first = '@@ -0,0 +1,2 @@\n1\t+first 界\n', tail = '2\t+second 😀\n';
  const whole = response(first + tail);
  const expected = historyFileRows(whole.results[0].data, whole);
  for (const mode of ['complete', 'duplicate', 'missing', 'changedMetadata', 'unmarkedContext']) {
    const driver = createDriver({ validate: () => null, tools: new Set([tool]), schemaOf: () => schema });
    const report = await driver.explore(tool, { queries: [{}] }, { surface: 'cli', rows: () => historyFileRows, call: async (_tool, input) => ({
      json: input.cursor ? response(mode === 'duplicate' ? '1\t+first 界\n' + tail : mode === 'missing' ? '' : tail,
        mode === 'unmarkedContext' ? undefined : Array.from(first).length, false, mode === 'changedMetadata' ? { stat: 'M +2 -1' } : {})
        : response(first, 0, true),
    }) });
    const actual = report.chains[0].rows;
    if (mode === 'complete') { assert.deepEqual(report.failures, []); assert.deepEqual(actual, expected); }
    else assert.ok(report.failures.length || JSON.stringify(actual) !== JSON.stringify(expected), `${mode} must fail evidence reconstruction`);
    if (mode === 'duplicate' || mode === 'unmarkedContext') assert.ok(report.failures.some(failure => failure.check === 'page.exactlyOnce'));
  }
});

test('inventory duplicates and same-patch references retain their own identities', () => {
  const data = { pullRequests: [{ number: 3, files: ['A +1 -0 first.ts', 'A +1 -0 first.ts', { path: 'second.ts', samePatchAs: 'first.ts' }] }] };
  assert.deepEqual(historyFileRows(data), ['3:first.ts:added', '3:first.ts:added', 'shared-patch:3:second.ts:first.ts']);
});
