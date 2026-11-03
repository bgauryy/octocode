import assert from 'node:assert/strict';
import test from 'node:test';
import { createInputCoverage, finiteInputDomains, primaryInputChecks } from './input-coverage.mjs';
import { contract } from '../contract.mjs';

const querySchema = {
  anyOf: [{ properties: { operation: { const: 'read' }, view: { $ref: '#/$defs/view' } } },
    { properties: { operation: { const: 'search' }, kinds: { items: { enum: ['file', 'directory'] } } } }],
  properties: { hidden: { type: 'boolean' }, questions: { additionalProperties: { properties: { type: { enum: ['noul', 'category'] } } } } },
  $defs: { view: { enum: ['content', 'symbols'] } },
};
const tools = [{ name: 'tool', querySchema }];
const row = (index, data = {}) => ({ index, data });

test('finite domains resolve references, union constants, booleans, arrays and maps', () => {
  assert.deepEqual(finiteInputDomains(querySchema), [
    { path: ['hidden'], values: [false, true] },
    { path: ['kinds', '[]'], values: ['file', 'directory'] },
    { path: ['operation'], values: ['read', 'search'] },
    { path: ['questions', '*', 'type'], values: ['noul', 'category'] },
    { path: ['view'], values: ['content', 'symbols'] },
  ]);
});

test('attempted invalid inputs and another transport cannot satisfy accepted coverage', () => {
  const coverage = createInputCoverage(tools);
  coverage.observe('tool', 'cli', { queries: [{ operation: 'read', view: 'symbols' }, { operation: 'search', hidden: false }] },
    { isError: true, json: { results: [row(0), { ...row(1, { errorCode: 'invalidInput' }), status: 'error' }] } });
  const domain = coverage.report().tools[0].domains.find(candidate => candidate.path[0] === 'operation');
  assert.deepEqual(domain.surfaces.cli, { requested: ['read', 'search'], accepted: ['read'], missing: ['search'] });
  assert.deepEqual(domain.surfaces.mcp.accepted, []);
});

test('response pages cover later batch indexes without guessing tool-level cursor inputs', () => {
  const coverage = createInputCoverage(tools);
  coverage.observe('tool', 'mcp', { queries: [{ operation: 'read' }, { operation: 'search', kinds: ['file', 'directory'] }] },
    { json: { results: [row(0)], responsePagination: { next: { kind: 'continue', tool: 'tool', input: { cursor: 'display' } } } } });
  coverage.observe('tool', 'cli', { cursor: 'display' }, { json: { results: [row(1)] } });
  coverage.observe('tool', 'mcp', { cursor: 'unrelated' }, { json: { results: [row(1)] } });
  assert.deepEqual(coverage.report().tools[0].domains.find(domain => domain.path[0] === 'operation').surfaces.mcp.missing, ['search']);
  coverage.observe('tool', 'mcp', { cursor: 'display' }, { json: { results: [row(1)] } });
  const domains = coverage.report().tools[0].domains;
  assert.deepEqual(domains.find(domain => domain.path[0] === 'operation').surfaces.mcp.missing, []);
  assert.deepEqual(domains.find(domain => domain.path[0] === 'kinds').surfaces.mcp.accepted, ['file', 'directory']);
});

test('the current generated contract exposes every tool and referenced input values', () => {
  const report = createInputCoverage(contract.tools).report();
  assert.deepEqual(report.tools.map(tool => tool.tool), contract.tools.map(tool => tool.name));
  const local = report.tools.find(tool => tool.tool === 'localFetch');
  assert.deepEqual(local.domains.find(domain => domain.path.join('.') === 'regexEngine').values, ['literal', 'rust', 'pcre2']);
  assert.deepEqual(local.domains.find(domain => domain.path.join('.') === 'caseMode').values, ['smart', 'sensitive', 'insensitive']);
  assert.equal(report.summary.cli.accepted, 0);
  assert.equal(report.summary.cli.total, report.summary.cli.missing);
});

test('primary variant gate fails missing operations/views only for selected tools and surfaces', () => {
  const coverage = createInputCoverage([...tools, { name: 'unselected', querySchema }]);
  coverage.observe('tool', 'cli', { queries: [{ operation: 'read', view: 'symbols' }] }, { json: { results: [row(0)] } });
  const initial = primaryInputChecks(coverage.report(), ['tool']);
  assert.equal(initial.length, 4);
  assert.ok(initial.every(check => check.tool === 'tool' && check.ok === false));
  assert.ok(initial.find(check => check.check === 'input.coverage.operation.cli').detail.includes('search'));
  for (const surface of ['cli', 'mcp']) {
    coverage.observe('tool', surface, { queries: [{ operation: 'read', view: 'symbols' }, { operation: 'search', view: 'content' }] },
      { json: { results: [row(0), row(1)] } });
  }
  assert.ok(primaryInputChecks(coverage.report(), ['tool']).every(check => check.ok));
  assert.ok(coverage.report().tools[0].domains.find(domain => domain.path[0] === 'hidden').surfaces.cli.missing.length);
  assert.ok(primaryInputChecks(coverage.report(), ['unselected']).every(check => !check.ok));
  assert.deepEqual(primaryInputChecks(coverage.report(), []), []);
});
