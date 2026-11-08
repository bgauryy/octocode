import assert from 'node:assert/strict';
import test from 'node:test';
import { planTool } from './derive.mjs';
import { ghGetHistoryItem, ghSearchHistory } from './spec-github.mjs';
import { contract } from '../contract.mjs';

const toolOf = name => contract.tools.find(tool => tool.name === name);
const numbers = query => ghSearchHistory.truth({}, { query: { owner: 'octo', repo: 'hist', operation: 'pullRequest', ...query } }).map(([number]) => number);

test('a cross-field check nested below the form root keeps its rows out of the plan', () => {
  const plan = planTool(toolOf('ghSearchHistory'), ghSearchHistory);
  const filters = plan.map(row => row.query.filters ?? {});
  assert(filters.some(f => f.linked !== undefined), 'linked rows are planned');
  assert(filters.some(f => f.notLinked !== undefined), 'notLinked rows are planned');
  assert.deepEqual(filters.filter(f => f.linked !== undefined && f.notLinked !== undefined), [], 'history.linked-exclusive');
  assert.deepEqual(filters.filter(f => f.state === 'open' && f.isMerged === true), [], 'history.pull-request-state');
});

test('nested checks are judged on their own value: under properties, union members and array items', () => {
  const check = { id: 'pair', assert: { not: {} }, when: { type: 'object', required: ['a', 'b'] } };
  const pair = { type: 'object', properties: { a: { type: 'boolean' }, b: { type: 'boolean' } }, 'x-checks': [check] };
  const querySchema = {
    type: 'object',
    properties: {
      kind: { enum: ['x'] },
      direct: pair,
      chosen: { anyOf: [{ $ref: '#/$defs/pair' }, { type: 'string' }] },
      list: { type: 'array', items: { $ref: '#/$defs/pair' } },
    },
    required: ['kind'],
    $defs: { pair },
  };
  const spec = { tool: 'synthetic', base: () => ({}) };
  const queries = planTool({ name: 'synthetic', querySchema }, spec).map(row => row.query);
  const both = value => value?.a !== undefined && value?.b !== undefined;
  assert(queries.some(query => query.direct?.a !== undefined), 'direct values are planned');
  assert.deepEqual(queries.filter(query => both(query.direct) || both(query.chosen) || (query.list ?? []).some(both)), []);
});

test('history filters: a scalar selects its value, its not<Field> excludes it', () => {
  assert.deepEqual(numbers({ filters: { checkStatus: 'success' } }), [12, 14]);
  assert.deepEqual(numbers({ filters: { notCheckStatus: ['pending'] } }), [12, 13, 14]);
  assert.deepEqual(numbers({ filters: { checkStatus: 'success', notCheckStatus: ['success'] } }), []);
  assert.deepEqual(numbers({ filters: { review: 'changesRequested' } }), [13]);
  assert.deepEqual(numbers({ filters: { notReview: ['approved', 'required'] } }), [11, 13]);
  assert.deepEqual(numbers({ filters: { linked: 'issue' } }), [11, 13]);
  assert.deepEqual(numbers({ filters: { notLinked: 'issue' } }), [12, 14]);
  assert.deepEqual(ghSearchHistory.truth({}, { query: { owner: 'octo', repo: 'hist', operation: 'issue', filters: { notLinked: 'pullRequest' } } }), [[22]]);
});

test('ghGetHistoryItem classification needs discussion in the read: comments, reviews or review comments', () => {
  const silent = query => ghGetHistoryItem.check({ providerHeaders: [] }, { query: { owner: 'octo', repo: 'demo', classification: { predicates: ['p'] }, ...query } }, []);
  for (const query of [
    { operation: 'pullRequest', number: 7, sections: ['files'] },
    { operation: 'pullRequest', number: 7, sections: ['body', 'commits'] },
    { operation: 'issue', number: 2 },
    { operation: 'issue', number: 2, sections: ['body'] },
  ]) assert.equal(silent(query), null, JSON.stringify(query));
  for (const query of [
    { operation: 'pullRequest', number: 7, sections: ['reviews'] },
    { operation: 'issue', number: 2, sections: ['comments'] },
  ]) assert.equal(silent(query), 'classification row reached no provider', JSON.stringify(query));
});
