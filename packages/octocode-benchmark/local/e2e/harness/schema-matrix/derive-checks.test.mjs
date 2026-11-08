import assert from 'node:assert/strict';
import test from 'node:test';
import { planRefusals, planTool } from './derive.mjs';
import { ghSearchCode, ghSearchHistory } from './spec-github.mjs';
import { contract } from '../contract.mjs';

const both = value => value?.a !== undefined && value?.b !== undefined;
const pair = { type: 'object', properties: { a: { type: 'boolean' }, b: { type: 'boolean' } }, 'x-checks': [{ id: 'pair', assert: { not: {} }, when: { type: 'object', required: ['a', 'b'] } }] };

test('a check under a map value keeps its rows out of the plan, as the runtime validates map values', () => {
  const querySchema = {
    type: 'object',
    properties: {
      kind: { enum: ['x'] },
      byName: { type: 'object', additionalProperties: { $ref: '#/$defs/pair' } },
      byPattern: { type: 'object', patternProperties: { '^k': { $ref: '#/$defs/pair' } } },
    },
    required: ['kind'],
    $defs: { pair },
  };
  const admitted = planTool({ name: 'synthetic', querySchema }, { tool: 'synthetic', base: () => ({}) }).map(row => row.query);
  assert(admitted.some(query => query.byName?.k?.a !== undefined), 'map values are planned');
  assert.deepEqual(admitted.filter(query => both(query.byName?.k) || both(query.byPattern?.k)), [], 'no admitted row breaks a map value\'s check');
});

test('each check the axes can break plans one minimal row that expects the refusal', () => {
  const tool = contract.tools.find(entry => entry.name === 'ghSearchHistory');
  const refusals = planRefusals(tool, ghSearchHistory);
  for (const [check, broken] of [
    ['history.linked-exclusive', filters => filters.linked !== undefined && filters.notLinked !== undefined],
    ['history.pull-request-state', filters => filters.state === 'open' && filters.isMerged === true],
  ]) {
    const rows = refusals.filter(row => row.check === check);
    assert(rows.length > 0, `${String(check)} has a refusal row`);
    for (const row of rows) {
      assert.equal(row.refusal, 'invalidInput');
      assert(broken(row.query.filters ?? {}), `${String(check)} row sets the forbidden pair: ${JSON.stringify(row.query)}`);
      assert.equal(Object.keys(row.query.filters).length, 2, `${String(check)} row sets nothing else in filters`);
      assert.deepEqual([row.values, row.swaps], [[], []], 'a refusal proves no field value and no field effect');
    }
  }
  assert.deepEqual(planTool(tool, ghSearchHistory).filter(row => row.query.filters?.linked !== undefined && row.query.filters?.notLinked !== undefined), [], 'the admitted plan stays clean');
});

test('a check is judged with the defaults the runtime fills: classification on ghSearchCode needs content hits, which matchIn defaults to', () => {
  const tool = contract.tools.find(entry => entry.name === 'ghSearchCode');
  assert(planTool(tool, ghSearchCode).some(row => row.query.classification && row.query.matchIn === undefined), 'classification with the default matchIn is admitted');
  const refusals = planRefusals(tool, ghSearchCode).filter(row => row.check === 'gh-search-code.classification');
  assert.equal(refusals.length, 1);
  assert.deepEqual(refusals[0].query.matchIn, ['path'], 'the refusal row is the one matchIn without content');
});
