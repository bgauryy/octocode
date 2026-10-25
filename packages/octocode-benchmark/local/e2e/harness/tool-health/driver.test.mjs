import test from 'node:test';
import assert from 'node:assert/strict';
import { createDriver, leadsIn, statusClass } from './driver.mjs';
import { contractValidators } from '../contract.mjs';

const callSchema = { properties: { kind: {}, tool: {}, input: {} } };
const schema = { properties: {
  results: { items: { properties: { data: { properties: {
    next: { additionalProperties: callSchema }, related: { items: callSchema },
    resources: { additionalProperties: { properties: { read: callSchema } } }, content: {},
  } } } } }, responsePagination: { properties: { next: callSchema } },
} };
const continuation = (cursor, tool = 't') => ({ kind: 'continue', tool, input: { cursor } });
const related = (tool, input) => ({ kind: 'related', tool, input });
const ok = data => ({ results: [{ index: 0, data }] });
const driver = (options = {}) => createDriver({
  validate: () => null, tools: new Set(['t', 'u', 'ghCloneRepo']), cliOnly: new Set(['ghCloneRepo']),
  schemaOf: () => schema, ...options,
});
function surface(script) {
  const calls = [];
  return { calls, call: async (tool, input) => {
    calls.push({ tool, input });
    const json = script(tool, input);
    return { json, raw: json, sc: json, text: JSON.stringify(json), ms: 1 };
  } };
}
const rows = (_tool, key) => key === 'remaining' ? data => data.items ?? [] : undefined;
const indexedItems = (index, items, next) => ({ index, data: { items, ...(next && { next: { remaining: continuation(next) } }) } });

// Same shape at an arbitrary evidence position must never become executable.
test('only schema-declared typed calls execute, including named resource reads', async () => {
  const input = { queries: [{ path: 'a file.ts', ranges: [{ startLine: 4, endLine: 9 }] }], responseChars: 500 };
  const result = ok({
    next: { remaining: continuation('opaque-page') }, related: [related('u', input)],
    resources: { file: { read: continuation('opaque-source', 'u') } },
    hints: [`read: u ${JSON.stringify(input)}`], content: { next: { fake: continuation('never') } },
  });
  const calls = leadsIn(result, schema);
  assert.deepEqual(calls.map(c => [c.kind, c.tool, c.input]), [
    ['continue', 't', { cursor: 'opaque-page' }], ['related', 'u', input], ['continue', 'u', { cursor: 'opaque-source' }],
  ]);
  const s = surface((_tool, request) => request.queries?.[0]?.seed ? result : ok({ items: ['evidence'] }));
  const report = await driver().explore('t', { queries: [{ seed: true }] }, { surface: 'cli', call: s.call });
  assert.deepEqual(report.failures, []);
  assert.deepEqual(s.calls.map(c => c.input), [{ queries: [{ seed: true }] }, { cursor: 'opaque-page' }, { cursor: 'opaque-source' }, input]);
});

test('opaque continuation walk reconstructs every row once without modifying any request', async () => {
  const s = surface((_tool, input) => {
    if (input.cursor === 'second') return ok({ items: ['b'], next: { remaining: continuation('third') } });
    if (input.cursor === 'third') return ok({ items: ['c'] });
    return ok({ items: ['a'], next: { remaining: continuation('second') } });
  });
  const input = { queries: [{ path: 'src', filters: { include: ['*.ts'] } }], responseChars: 600 };
  const report = await driver().explore('t', input, { surface: 'cli', call: s.call, rows });
  assert.deepEqual(report.failures, []);
  assert.deepEqual(s.calls.map(c => c.input), [input, { cursor: 'second' }, { cursor: 'third' }]);
  assert.deepEqual(report.chains.map(c => ({ rows: c.rows, ended: c.ended, pages: c.pages })), [{ rows: ['a', 'b', 'c'], ended: true, pages: 3 }]);
});

test('query continuations retain their origin when resumed rows move and fan out', async () => {
  const s = surface((_tool, input) => ({ results: input.cursor === 'tail'
    ? [indexedItems(2, ['b']), indexedItems(2, ['c'], 'last')]
    : input.cursor === 'last' ? [indexedItems(2, ['d'])]
      : [indexedItems(0, ['unrelated']), indexedItems(1, []), indexedItems(2, ['a'], 'tail')] }));
  const report = await driver().explore('t', { queries: [{}, {}, {}] }, { surface: 'cli', call: s.call, rows });
  assert.deepEqual(report.failures, []);
  assert.deepEqual(report.chains.map(chain => chain.rows), [['a', 'b', 'c', 'd']]);
  assert.equal(report.chains[0].ended, true);
});

test('display pages reconstruct all batch rows, including captured indexed errors', async () => {
  const s = surface((_tool, input) => input.cursor ? { results: [
    { index: 1, status: 'error', data: { errorCode: 'invalidInput', error: 'invalid sibling' } },
    { index: 2, data: { items: ['b'] } },
  ] } : { results: [{ index: 0, data: { items: ['a'] } }], responsePagination: { next: continuation('rest') } });
  const report = await driver().explore('t', { queries: [{}, {}, {}] }, {
    surface: 'cli', call: s.call, rows: (_tool, key) => key === 'responsePagination.next' ? data => data.items ?? [] : undefined,
  });
  assert.deepEqual(report.failures, []);
  assert.deepEqual(report.chains[0].rows, ['a', 'b']);
});

test('transport validation joins display pages and isolates unrelated tool outcomes', async () => {
  const groups = [];
  const s = surface((tool, input) => tool === 'u' ? ok({ items: ['related'] }) : input.cursor
    ? ok({ items: ['tail'] }) : { ...ok({ items: ['first'], related: [related('u', { queries: [{}] })] }), responsePagination: { next: continuation('rest') } });
  const report = await driver({ validateTransport: (channel, responses) => {
    groups.push({ surface: channel, rows: responses.flatMap(response => response.json.results.flatMap(row => row.data.items)) });
    return responses.length === 2 ? 'deliberate full-batch failure' : null;
  } }).explore('t', { queries: [{}] }, { surface: 'mcp', call: s.call });
  assert.deepEqual(groups, [{ surface: 'mcp', rows: ['first', 'tail'] }, { surface: 'mcp', rows: ['related'] }]);
  assert.deepEqual(report.failures.map(failure => failure.check), ['transport']);
});

test('identical related calls from sibling rows converge while duplicate required work fails', async () => {
  for (const kind of ['related', 'continue']) {
    const lead = kind === 'related' ? related('u', { queries: [{}] }) : continuation('read', 'u');
    const s = surface((tool) => tool === 't' ? { results: [0, 1].map(index => ({ index, data:
      kind === 'related' ? { related: [lead] } : { next: { remaining: lead } },
    })) } : ok({ items: ['evidence'] }));
    const report = await driver().explore('t', { queries: [{}, {}] }, { surface: 'cli', call: s.call });
    assert.equal(s.calls.filter(call => call.tool === 'u').length, 1);
    if (kind === 'related') { assert.deepEqual(report.failures, []); assert.equal(report.converged, 1); }
    else assert.ok(report.failures.some(failure => failure.check === 'lead.repeat'));
  }
});

test('continuations have priority over optional work and caps disclose unfinished evidence', async () => {
  const s = surface((_tool, input) => ok({ items: [input.cursor ?? 'first'], next: { remaining: continuation(`${input.cursor ?? 'start'}+`) }, related: [related('u', { queries: [{ explore: true }] })] }));
  const report = await driver().explore('t', { queries: [{}] }, { surface: 'cli', call: s.call, maxPages: 2, maxCalls: 1, rows });
  assert.deepEqual(s.calls.map(c => c.tool), ['t', 't']);
  assert.ok(report.failures.some(f => f.check === 'page.cap'));
  assert.ok(report.budgetCut > 0);
});

test('duplicate evidence and repeated cursor handles fail the walk', async () => {
  for (const mode of ['rows', 'cursor']) {
    const s = surface((_tool, input) => input.cursor ? ok({ items: ['a'], ...(mode === 'cursor' && { next: { remaining: continuation('again') } }) }) : ok({ items: ['a'], next: { remaining: continuation('again') } }));
    const report = await driver().explore('t', { queries: [{}] }, { surface: 'cli', call: s.call, rows });
    assert.ok(report.failures.some(f => f.check === 'page.exactlyOnce'));
    assert.equal(report.failures.some(f => f.check === 'lead.repeat'), mode === 'cursor');
  }
});

test('display fragments reuse a completed captured read without executing it twice', async () => {
  const read = continuation('pinned-source', 'u');
  const s = surface((tool, input) => {
    if (tool === 'u') return ok({ content: '1\timmutable source' });
    const result = ok({ resources: { file: { read } } });
    return input.cursor ? result : { ...result, responsePagination: { next: continuation('fragment-2') } };
  });
  const report = await driver().explore('t', { queries: [{}] }, { surface: 'cli', call: s.call });
  assert.deepEqual(report.failures, []);
  assert.equal(s.calls.filter(call => call.tool === 'u').length, 1);
  assert.equal(report.converged, 1);
});

test('a repeated branch with unfinished source pages is still an error', async () => {
  const read = continuation('pinned-source', 'u');
  const s = surface((tool, input) => {
    if (tool === 'u') return input.cursor === 'pinned-source' ? ok({ next: { remaining: continuation('source-tail', 'u') } }) : ok({ content: 'tail' });
    const result = ok({ resources: { file: { read } } });
    return input.cursor ? result : { ...result, responsePagination: { next: continuation('fragment-2') } };
  });
  const report = await driver().explore('t', { queries: [{}] }, { surface: 'cli', call: s.call });
  assert.ok(report.failures.some(failure => failure.check === 'lead.repeat' && failure.detail.includes('unfinished work')));
});

test('a continuation pointing to an ancestor remains a cycle', async () => {
  const s = surface((_tool, input) => ok({ next: { remaining: continuation(input.cursor === 'a' ? 'b' : 'a') } }));
  const report = await driver().explore('t', { cursor: 'a' }, { surface: 'cli', call: s.call });
  assert.equal(s.calls.length, 2);
  assert.ok(report.failures.some(failure => failure.check === 'lead.repeat' && failure.detail.includes('ancestor')));
});

test('every required continuation fails on a typed execution error without a row mapper', async () => {
  for (const response of [
    { errorCode: 'responseSnapshotExpired', error: 'Captured read expired' },
    { results: [{ index: 0, status: 'error', data: { errorCode: 'staleSnapshot', error: 'Source changed' } }] },
    { errorCode: 'invalidInput', error: 'Issued cursor rejected' },
  ]) {
    const s = surface((_tool, input) => input.cursor ? response : ok({ resources: { file: { read: continuation('issued', 'u') } } }));
    const report = await driver().explore('t', { queries: [{}] }, { surface: 'cli', call: s.call });
    assert.equal(s.calls.length, 2);
    assert.equal(report.failures.length, 1);
    assert.equal(report.failures[0].check, response.errorCode === 'invalidInput' ? 'lead.accepted' : 'lead.execution');
  }
});

test('top-level errors must satisfy the published output schema', async () => {
  const { validateOutput } = contractValidators();
  for (const malformed of [true, false]) {
    const response = malformed ? { errorCode: 'notFound', bogus: true } : { errorCode: 'notFound', error: 'Source is missing' };
    const s = surface(() => response);
    const report = await driver({ validate: validateOutput }).explore('localFetch', { queries: [{ path: 'missing' }] }, { surface: 'cli', call: s.call });
    assert.equal(report.failures.filter(failure => failure.check === 'contract').length, Number(malformed));
  }
});

test('invalid indexed replies and invalid output contracts are reported', async () => {
  const s = surface((_tool, input) => input.cursor ? { results: [{ index: 0, status: 'error', data: { errorCode: 'invalidInput', error: 'bad' } }] } : ok({ next: { remaining: continuation('invalid') } }));
  const report = await driver({ validate: (_tool, json) => json.results?.[0]?.data?.next ? 'invalid output' : null })
    .explore('t', { queries: [{}] }, { surface: 'cli', call: s.call });
  assert.ok(report.failures.some(f => f.check === 'contract'));
  assert.ok(report.failures.some(f => f.check === 'lead.accepted'));
  assert.equal(statusClass(s.calls.length && { results: [{ index: 0, status: 'error', data: { errorCode: 'invalidInput' } }] }), 'invalid');
});

test('related convergence executes once; unavailable and writing calls are disclosed', async () => {
  const s = surface((_tool, input) => input.queries?.[0]?.seed ? ok({ related: [related('u', { queries: [{}] }), related('ghCloneRepo', { queries: [{}] })] }) : ok({ related: [related('t', { queries: [{ seed: true }] })] }));
  const report = await driver().explore('t', { queries: [{ seed: true }] }, { surface: 'mcp', call: s.call });
  assert.deepEqual(s.calls.map(c => c.tool), ['t', 'u']);
  assert.deepEqual(report.failures, []);
  assert.equal(report.converged, 1);
  assert.match(report.skipped[0].reason, /CLI-only/);
});

test('graph parity compares observed continuation routes, never token bytes', async () => {
  const d = driver();
  const explore = async (token, target = 't') => d.explore('t', { queries: [{}] }, { surface: 'cli', call: surface((_tool, input) => input.cursor ? ok({ items: ['end'] }) : ok({ next: { remaining: continuation(token, target) } })).call });
  assert.equal(d.graphParity(await explore('cli-token'), await explore('mcp-token')), null);
  assert.match(d.graphParity(await explore('cli-token'), await explore('mcp-token', 'u')), /graph differs/);
});

test('declared calls resolve references, unions, and record entries without inspecting literal evidence', async () => {
  const { contract } = await import('../contract.mjs');
  const local = contract.tools.find(tool => tool.name === 'localSearch').outputSchema;
  const captured = contract.tools.find(tool => tool.name === 'clasify').outputSchema;
  const next = continuation('cursor-A', 'localFetch');
  const exploration = related('localFetch', { queries: [{ path: 'src/a.ts' }] });
  assert.deepEqual(leadsIn(ok({ next: { expand: next }, related: [exploration] }), local).map(call => call.input), [next.input, exploration.input]);
  assert.deepEqual(leadsIn(ok({ resources: { source: { read: next } } }), captured).map(call => call.input), [next.input]);
});
