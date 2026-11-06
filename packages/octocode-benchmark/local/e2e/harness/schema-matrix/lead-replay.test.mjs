import assert from 'node:assert/strict';
import test from 'node:test';
import { replayLeads } from './lead-replay.mjs';

function worker(failures, root = {}) {
  return {
    pending: [{ surface: 'cli', from: 'ghGetHistoryItem', tool: 'ghGetFileContent', input: { queries: [{ path: 'fixture.ts' }] } }],
    driver: { explore: async () => ({ failures, root: { json: root } }) },
  };
}
const deliberateFailure = { check: 'lead.execution', detail: 'fixture deliberately returns server failure' };

test('a negative scenario applies its existing failure policy to its own related walk', async () => {
  assert.deepEqual(await replayLeads(worker([deliberateFailure]), null, { allow: ['lead.execution'] }), []);
});

test('ordinary rows still fail the same related execution error', async () => {
  const failures = await replayLeads(worker([deliberateFailure]), null);
  assert.equal(failures.length, 1);
  assert.equal(failures[0].check, 'lead.lead.execution');
});

test('allowing intentional execution failures never hides schema or rejected-input failures', async () => {
  const schemaFailure = { check: 'schema', detail: 'missing required evidence field' };
  const failures = await replayLeads(worker([deliberateFailure, schemaFailure], { errorCode: 'invalidInput' }), null, { allow: ['lead.execution'] });
  assert.deepEqual(failures.map(failure => failure.check), ['lead.schema', 'lead.rejected']);
});
