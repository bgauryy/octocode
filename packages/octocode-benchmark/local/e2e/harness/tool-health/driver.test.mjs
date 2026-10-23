// node --test packages/octocode-benchmark/local/e2e/harness/tool-health/driver.test.mjs — the
// generic e2e driver's checks against scripted responses (no binary needed).
import test from 'node:test';
import assert from 'node:assert/strict';
import { createDriver, factSentences, leadsIn, statusClass, writes } from './driver.mjs';
import { createQueryDefaults } from '../query-defaults.mjs';

const TOOLS = new Set(['t', 'u', 'clasify', 'ghCloneRepo', 'astSearch']);
const lead = (tool, row) => ({ tool, query: { queries: [row] } });
const ok = data => ({ results: [{ index: 0, data }] });
const empty = data => ({ results: [{ index: 0, data, status: 'empty' }] });

test('string hints preserve suggested inputs and stay separate from required pages', () => {
  const query = { queries: [{ path: 'a file.ts', ranges: ['4-9'], snapshot: 'abc' }] };
  const value = ok({
    next: { nextPage: lead('t', { page: 2 }) },
    hints: [`read: u ${JSON.stringify(query)}`, `repair: Approximate: t ${JSON.stringify(query)}`, 'Use a narrower path.', 'read: u {broken}'],
    content: `read: u ${JSON.stringify(query)}`,
  });
  const calls = leadsIn(value, TOOLS);
  assert.equal(calls.length, 3);
  assert.equal(calls[0].container, 'next');
  assert.deepEqual(calls[1].query, query);
  assert.equal(calls[1].container, 'hints');
  assert.equal(calls[2].confidence, 'low');
});

/** A scripted surface: `script(tool, row)` → payload; records every call. */
function surface(script) {
  const calls = [];
  const call = async (tool, input) => {
    calls.push({ tool, input });
    const json = script(tool, input.queries[0]);
    return { json, raw: json, sc: json, text: JSON.stringify(json), isError: statusClass(json) === 'invalid', ms: 1 };
  };
  return { call, calls };
}

function driver({ validate = () => null, staticText = '', defaultsOf } = {}) {
  return createDriver({ validate, staticText, tools: TOOLS, cliOnly: new Set(['ghCloneRepo',]), defaultsOf });
}
const rows = () => data => data.items ?? [];

/** Three pages of two rows; `bad` alters page 2 or 3. */
const paged = bad => (tool, row) => {
  const page = row.page ?? 1;
  const items = bad === 'dupRow' && page === 2 ? ['r2', 'r3'] : [`r${page * 2 - 1}`, `r${page * 2}`];
  const nextRow = bad === 'cycle' && page === 2 ? { q: 'x' } : { q: 'x', page: page + 1 };
  return ok({ items, ...(page < 3 && { next: { nextPage: lead('t', nextRow) } }) });
};

test('a page chain walks to its end with every row once', async () => {
  const s = surface(paged());
  const r = await driver().explore('t', { q: 'x' }, { surface: 'cli', call: s.call, rows, root: '/' });
  assert.deepEqual(r.failures, []);
  assert.equal(r.chains.length, 1);
  assert.deepEqual({ pages: r.chains[0].pages, rows: r.chains[0].rows, ended: r.chains[0].ended }, { pages: 3, rows: ['r1', 'r2', 'r3', 'r4', 'r5', 'r6'], ended: true });
});

test('a page may materialize an equivalent schema default without changing scope', async () => {
  const script = (_tool, row) => row.page
    ? ok({ items: ['second'] })
    : ok({ items: ['first'], next: { nextPage: lead('t', { q: 'x', sort: 'path', apply: false, page: 2 }) } });
  const r = await driver({ defaultsOf: () => ({ sort: 'path', apply: false }) })
    .explore('t', { q: 'x' }, { surface: 'cli', call: surface(script).call, rows, root: '/' });
  assert.deepEqual(r.failures, []);
  assert.deepEqual(r.chains[0].rows, ['first', 'second']);
});

test('effective defaults never excuse changed or dropped supplied selection controls', async () => {
  for (const [seed, next] of [
    [{ q: 'x', sort: 'modified' }, { q: 'x', sort: 'path', page: 2 }],
    [{ q: 'x', includeHidden: true }, { q: 'x', page: 2 }],
    [{ q: 'x', defaultExcludes: false }, { q: 'x', page: 2 }],
  ]) {
    const script = (_tool, row) => row.page
      ? ok({ items: ['second'] })
      : ok({ items: ['first'], next: { nextPage: lead('t', next) } });
    const r = await driver({ defaultsOf: () => ({ sort: 'path', includeHidden: false, defaultExcludes: true }) })
      .explore('t', seed, { surface: 'cli', call: surface(script).call, rows, root: '/' });
    assert.ok(r.failures.some(f => f.check === 'lead.keeps-scope'), JSON.stringify({ seed, next, failures: r.failures }));
  }
});

test('whole-file pages preserve the effective exact view and reject changed non-default views', async () => {
  for (const [seed, minify, accepted] of [
    [{ q: 'x', fullContent: true }, undefined, true],
    [{ q: 'x' }, undefined, true],
    [{ q: 'x', fullContent: true, minify: 'standard' }, 'standard', true],
    [{ q: 'x', fullContent: true, minify: 'standard' }, undefined, false],
    [{ q: 'x', fullContent: true, minify: 'symbols' }, 'symbols', true],
    [{ q: 'x', fullContent: true, minify: 'symbols' }, undefined, false],
    [{ q: 'x', fullContent: true }, 'standard', false],
  ]) {
    const script = (_tool, row) => row.offset
      ? ok({ items: ['second'] })
      : ok({ items: ['first'], next: { continueWindow: lead('t', { q: 'x', unit: 'lines', offset: 1, ...(minify === undefined ? {} : { minify }) }) } });
    const r = await driver({ defaultsOf: () => ({}) })
      .explore('t', seed, { surface: 'cli', call: surface(script).call, rows, root: '/' });
    assert.equal(r.failures.some(f => f.check === 'lead.keeps-scope'), !accepted, JSON.stringify({ seed, failures: r.failures }));
    assert.deepEqual(r.chains[0].rows, ['first', 'second']);
  }
});

test('query defaults resolve references only in the matching operation variant', () => {
  const schema = {
    $defs: { Flag: { type: 'boolean', default: false }, Bound: { type: 'integer', default: 7 } },
    properties: { queries: { items: { anyOf: [
      { type: 'object', required: ['path', 'operation'], properties: { path: { type: 'string' }, operation: { const: 'files' }, includeHidden: { $ref: '#/$defs/Flag' }, maxFiles: { $ref: '#/$defs/Bound' }, sort: { type: 'string', default: 'path' } } },
      { type: 'object', required: ['path', 'operation'], properties: { path: { type: 'string' }, operation: { const: 'tree' }, maxDepth: { type: 'integer', default: 3 } } },
    ] } } },
  };
  const defaultsOf = createQueryDefaults([{ name: 't', inputSchema: schema }]);
  assert.deepEqual(defaultsOf('t', { path: '.', operation: 'files' }), { includeHidden: false, maxFiles: 7, sort: 'path' });
  assert.deepEqual(defaultsOf('t', { path: '.', operation: 'tree' }), { maxDepth: 3 });
  assert.deepEqual(defaultsOf('t', { path: '.', operation: 'unknown' }), {});
  assert.deepEqual(defaultsOf('unregistered', {}), {});
});

test('ambiguous schema branches do not invent a conflicting default', () => {
  const schema = { properties: { queries: { items: { anyOf: [
    { type: 'object', properties: { enabled: { type: 'boolean', default: true }, pageSize: { type: 'integer', default: 10 } } },
    { type: 'object', properties: { enabled: { type: 'boolean', default: false }, pageSize: { type: 'integer', default: 10 } } },
  ] } } } };
  const defaultsOf = createQueryDefaults([{ name: 't', inputSchema: schema }]);
  assert.deepEqual(defaultsOf('t', {}), { pageSize: 10 });
});

test('viewSyntax permits AST syntax inspection and rejects other AST operations', async () => {
  for (const operation of ['syntaxTree', 'symbols', 'match']) {
    const script = (_tool, row) => row.operation
      ? ok({ items: ['node'] })
      : empty({ hints: { viewSyntax: lead('astSearch', { path: '/source.ts', operation }) } });
    const r = await driver().explore('astSearch', { path: '/source.ts' }, { surface: 'cli', call: surface(script).call, root: '/' });
    assert.equal(r.failures.some(f => f.check === 'lead.key-tool'), operation !== 'syntaxTree', JSON.stringify(r.failures));
  }
});

test('a page repeating a row fails exactly-once', async () => {
  const r = await driver().explore('t', { q: 'x' }, { surface: 'cli', call: surface(paged('dupRow')).call, rows, root: '/' });
  assert.deepEqual(r.failures.map(f => f.check), ['page.exactlyOnce']);
  assert.match(r.failures[0].detail, /page 2 repeats r2/);
});

test('a page lead back to an earlier page is a cycle', async () => {
  const r = await driver().explore('t', { q: 'x' }, { surface: 'cli', call: surface(paged('cycle')).call, rows, root: '/' });
  assert.ok(r.failures.some(f => f.check === 'lead.repeat' && /cycle/.test(f.detail)), JSON.stringify(r.failures));
  assert.ok(r.failures.some(f => f.check === 'page.ended'), 'the chain never reached its end');
});

test('a chain past maxPages fails the cap', async () => {
  const r = await driver().explore('t', { q: 'x' }, { surface: 'cli', call: surface(paged()).call, rows, maxPages: 2, root: '/' });
  assert.deepEqual(r.failures.map(f => f.check), ['page.cap']);
});

test('two leads of one response with the same query fail; converging branches run once', async () => {
  const script = (tool, row) => {
    if (tool === 'u') return ok({ items: [row.n], hints: { lookT: lead('t', { q: 'shared' }) } });
    if (row.q === 'seed') return ok({ hints: { lookA: lead('u', { n: 1 }), lookB: lead('u', { n: 2 }), again: lead('u', { n: 1 }) } });
    return ok({ items: ['s'] });
  };
  const s = surface(script);
  const r = await driver().explore('t', { q: 'seed' }, { surface: 'cli', call: s.call, root: '/' });
  assert.deepEqual(r.failures.map(f => f.check), ['lead.repeat']);
  assert.match(r.failures[0].detail, /hints\.again .*repeats hints\.lookA of the same response/);
  assert.equal(r.converged, 1, 'u n:1 and u n:2 both lead to t shared');
  assert.equal(s.calls.filter(c => c.input.queries[0].q === 'shared').length, 1);
});

test('a rejected lead and a contract-invalid response fail', async () => {
  const script = (tool, row) => (row.bad ? { error: 'bad field', errorCode: 'invalidInput' } : ok({ items: [1], hints: { retryNarrow: lead('t', { bad: true }) } }));
  const validate = (tool, json) => (json.results?.[0]?.data?.items ? 'items must be strings' : null);
  const r = await driver({ validate }).explore('t', { q: 'x' }, { surface: 'cli', call: surface(script).call, root: '/' });
  assert.deepEqual(r.failures.map(f => f.check).sort(), ['contract', 'lead.accepted']);
});

test('writing leads, and CLI-only tools on MCP, are skipped with a reason', async () => {
  const script = tool => tool === 't' ? ok({ hints: { cloneRepo: lead('ghCloneRepo', { owner: 'o' }), read: lead('u', { n: 1 }) } }) : ok({ items: ['p'] });
  const cli = surface(script);
  const r = await driver().explore('t', { q: 'x' }, { surface: 'cli', call: cli.call, root: '/' });
  assert.deepEqual(r.skipped.map(x => x.reason.split(' (')[0]), ['writes the clone cache']);
  assert.deepEqual(cli.calls.map(c => c.tool), ['t', 'u']);
  const mcp = surface(script);
  const m = await driver().explore('t', { q: 'x' }, { surface: 'mcp', call: mcp.call, root: '/' });
  assert.deepEqual(m.skipped.map(x => x.reason), ['ghCloneRepo is CLI-only (not on MCP)']);
  assert.deepEqual(mcp.calls.map(c => c.tool), ['t', 'u']);
  assert.equal(writes(lead('u', { n: 1 })), null);
});

test('leadsIn reads {tool, query} leads, self-continuations and the envelope page', () => {
  const sc = { queries: [{ next: { clasify: { queries: [{ carry: {} }] } }, hints: { read: lead('u', { n: 1 }), text: ['tip'] } }], responsePagination: { next: lead('clasify', { x: 1 }) } };
  assert.deepEqual(leadsIn(sc, TOOLS).map(l => `${l.container}.${l.key}->${l.tool}`), ['next.clasify->clasify', 'hints.read->u', 'next.responsePagination.next->clasify']);
});

test('facts: description text and notes repeated across every call shape fail', () => {
  const d = driver({ staticText: 'Find literal matches. Hits do not establish symbol identity across files.' });
  const fact = (shape, sentences) => ({ tool: 't', shape, sentences });
  const failures = d.factsFailures([{ facts: [
    fact('a|', ['Hits do not establish symbol identity across files.', 'Always the same static reminder text here.']),
    fact('b|', ['Always the same static reminder text here.', 'No matches in 3 files searched today.']),
    fact('a|', ['Always the same static reminder text here.']),
  ] }]);
  assert.deepEqual(failures.map(f => f.check), ['facts.static', 'facts.repeated']);
  assert.deepEqual(factSentences(empty({ warnings: ['short', 'A long warning sentence about this call. Second sentence is long enough too.'], hints: ['No matches in 3 files searched.'] })),
    ['A long warning sentence about this call.', 'Second sentence is long enough too.', 'No matches in 3 files searched.']);
});

test('graph parity compares edges of calls both surfaces ran', async () => {
  const mk = key => (tool, row) => (row.q === 'x' ? ok({ items: ['a'], hints: { [key]: lead('u', { n: 1 }) } }) : ok({ items: ['b'] }));
  const d = driver();
  const a = await d.explore('t', { q: 'x' }, { surface: 'cli', call: surface(mk('lookA')).call, root: '/' });
  const b = await d.explore('t', { q: 'x' }, { surface: 'mcp', call: surface(mk('lookA')).call, root: '/' });
  const c = await d.explore('t', { q: 'x' }, { surface: 'mcp', call: surface(mk('lookB')).call, root: '/' });
  assert.equal(d.graphParity(a, b), null);
  assert.match(d.graphParity(a, c), /CLI-only 1 .*hints\.lookA.*MCP-only 1/);
});
