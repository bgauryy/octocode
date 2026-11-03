import assert from 'node:assert/strict';
import test from 'node:test';
import { createDriver } from './driver.mjs';

const call = { properties: { kind: {}, tool: {}, input: {} } };
const schema = { properties: {
  results: { items: { properties: { data: { properties: { next: { additionalProperties: call } } } } } },
  responsePagination: { properties: { next: call } },
} };
const next = cursor => ({ kind: 'continue', tool: 'read', input: { cursor } });

test('every response page precedes row continuations regardless of JSON property order', async () => {
  for (const surface of ['cli', 'mcp']) for (const responseFirst of [false, true]) {
    const observed = [];
    const driver = createDriver({ validate: () => null, tools: new Set(['read']), schemaOf: () => schema });
    const report = await driver.explore('read', { queries: [{}, {}, {}] }, { surface, call: async (_tool, input) => {
      const key = input.cursor ?? 'seed';
      observed.push(key);
      const index = ['seed', 'display-2', 'display-3'].indexOf(key);
      if (index < 0) return { json: { results: [{ index: Number(key.at(-1)), data: {} }] } };
      const fields = { results: [{ index, data: { next: { nextPage: next(`row-${index}`) } } }] };
      const paging = index < 2 ? { responsePagination: { next: next(`display-${index + 2}`) } } : {};
      return { json: responseFirst ? { ...paging, ...fields } : { ...fields, ...paging } };
    } });
    assert.deepEqual(report.failures, []);
    assert.deepEqual(observed, ['seed', 'display-2', 'display-3', 'row-0', 'row-1', 'row-2']);
  }
});

test('driver failures retain the complete contract diagnostic', async () => {
  const diagnostic = Array.from({ length: 100 }, (_, index) => `field ${index}: invalid 界😀`).join('\n');
  const driver = createDriver({ validate: () => diagnostic, tools: new Set(['read']), schemaOf: () => schema });
  const report = await driver.explore('read', { queries: [{}] }, { surface: 'cli',
    call: async () => ({ json: { results: [{ index: 0, data: {} }] } }) });
  assert.equal(report.failures.find(failure => failure.check === 'contract').detail, `read ${diagnostic}`);
});
