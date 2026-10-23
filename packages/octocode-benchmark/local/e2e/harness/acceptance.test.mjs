// node --test harness/acceptance.test.mjs — one red and one green fixture per acceptance check.
import test from 'node:test';
import assert from 'node:assert/strict';
import { MCP_SURFACE_BUDGETS, measureMcpSurface } from '@octocodeai/config/mcp';
import { checkChainFit, checkRowContract, descriptionLint, packedCoordinates, packedStrings, silentOmissions, surfaceBudget, zeroBasedTexts, zeroCoordinates } from './acceptance.mjs';

const inputs = { localFetch: new Set(['path', 'matchString', 'startLine', 'endLine']), lspSearch: new Set(['path', 'symbolName', 'line', 'operation']) };
const published = t => inputs[t] ?? null;
const opts = { inputNames: published, published, toolNames: Object.keys(inputs) };

test('public string hints preserve call validation and prose recovery', () => {
  const bad = { results: [{ data: { hints: ['read: localFetch {"queries":[{"path":"a","position":3}]}'] } }] };
  assert.ok(checkChainFit(bad, 'lspSearch', opts).some(reason => reason.includes('`position`')));
  const prose = { results: [{ status: 'empty', data: { hints: ['Broaden matchString.'] } }] };
  assert.deepEqual(checkRowContract(prose, 'empty'), []);
  const unknown = { results: [{ data: { hints: ['Try inventedKnob:true'] } }] };
  assert.ok(checkChainFit(unknown, 'localFetch', opts).some(reason => reason.includes('inventedKnob')));
});

test('required next recovery satisfies an error row without optional hints', () => {
  const error = { results: [{ status: 'error', data: { error: 'Invalid range', errorCode: 'invalidInput', next: { repair: { tool: 'localFetch', query: { queries: [{ path: 'a', startLine: 1 }] } } } } }] };
  assert.deepEqual(checkRowContract(error, 'error'), []);
});

test('C1 packed coordinates: lsp/ast packed rows fail, "<line>\\t<value>" passes', () => {
  assert.ok(packedCoordinates('433:20 in function x 430-475'));
  assert.ok(packedCoordinates('227-428 function x +'));
  assert.ok(packedCoordinates('fn handle@430 12-20'));
  assert.equal(packedCoordinates('452\tconst x = 1;'), 0);
  assert.equal(packedCoordinates('src/a.ts'), 0);
  assert.equal(packedCoordinates('v1.2-3'), 0);
  assert.equal(packedCoordinates('2026-10-04'), 0);
  assert.equal(packedCoordinates('2026-05-30T23:55:19Z'), 0);
});

test('C1 packed coordinates: outline rows are declaration objects; a packed string fails under any key', async t => {
  const outline = { results: [{ data: { symbols: [{ symbolName: 'a', kind: 'function', line: 3, endLine: 5, docStartLine: 2 }, { symbolName: 'B', kind: 'impl', line: 7, endLine: 20, members: [{ symbolName: 'c', line: 8, endLine: 10 }] }] } }] };
  assert.deepEqual(packedStrings(outline), []);
  for (const [name, data] of [
    ['locations', { locations: ['c (8-10, doc 7)'] }],
    ['symbols', { symbols: ['227-428 function x +'] }],
    ['symbols outline string', { symbols: ['a (3-5, function, doc 2)'] }],
    ['members', { symbols: [{ symbolName: 'B', line: 7, members: ['c (8-10, doc 7)'] }] }],
  ]) await t.test(name, () => assert.equal(packedStrings({ results: [{ data }] }).length, 1));
});

test('C1 chain fit: unknown target field and packed row fail; clean row passes', () => {
  const bad = { results: [{ status: 'ok', data: { locations: ['433:20 in function x 430-475'], hints: ['read: localFetch '+JSON.stringify({ queries: [{ path: 'a', position: 3 }] })] } }] };
  const reasons = checkChainFit(bad, 'lspSearch', opts);
  assert.ok(reasons.some(r => r.includes('`position`')), reasons.join('\n'));
  assert.ok(reasons.some(r => r.startsWith('packed')), reasons.join('\n'));
  const flat = { results: [{ data: { next: { repair: { tool: 'localFetch', query: { path: 'a' } } } } }] };
  assert.ok(checkChainFit(flat, 'lspSearch', opts).some(r => r.includes('envelope')));
  const good = { results: [{ status: 'ok', data: { locations: [{ path: 'a', line: 433, value: 'x' }], hints: ['read: localFetch '+JSON.stringify({ queries: [{ path: 'a', startLine: 1 }] })] } }] };
  assert.deepEqual(checkChainFit(good, 'lspSearch', opts), []);
});

test('C1 hint text lint: unpublished knob without a lead fails; with a lead passes', () => {
  const bad = { results: [{ data: { hints: ['try caseMode:"insensitive"'] } }] };
  assert.ok(checkChainFit(bad, 'localFetch', opts).some(r => r.includes('caseMode')));
  const good = { results: [{ data: { hints: ['try caseMode:"insensitive"', 'retry: localFetch '+JSON.stringify({ queries: [{ path: 'a', caseMode: 'insensitive' }] })] } }] };
  const optsWithCase = { ...opts, inputNames: t => (t === 'localFetch' ? new Set([...inputs.localFetch, 'caseMode']) : published(t)) };
  assert.deepEqual(checkChainFit(good, 'localFetch', optsWithCase), []);
  const named = { results: [{ data: { hints: ['Verify with lspSearch operation:"definition"'] } }] };
  assert.deepEqual(checkChainFit(named, 'localFetch', opts), []);
});

test('C2 row contract: error row without errorCode fails; full error/empty rows pass', () => {
  const bad = { results: [{ status: 'error', data: { error: 'boom' } }] };
  const reasons = checkRowContract(bad, 'error');
  assert.ok(reasons.some(r => r.includes('errorCode')));
  assert.ok(reasons.some(r => r.includes('recovery')));
  assert.ok(checkRowContract({ results: [{ status: 'error', data: { error: 'x', errorCode: 'transport', retryable: false, hints: ['retry'] } }] }, 'error').some(r => r.includes('retryable:false')));
  assert.deepEqual(checkRowContract({ results: [{ status: 'error', data: { error: 'x', errorCode: 'pathNotFound', hints: ['check the path'] } }] }, 'error'), []);
  assert.ok(checkRowContract({ results: [{ status: 'empty', data: {} }] }, 'empty').length);
  assert.deepEqual(checkRowContract({ results: [{ status: 'empty', data: { hints: ['shorter term'] } }] }, 'empty'), []);
  assert.ok(checkRowContract(undefined, 'error', { isError: true, text: 'Input validation error' }).length);
});

test('C3 one base: 0-based text and column 0 fail; 1-based passes', () => {
  assert.equal(zeroBasedTexts({ lspSearch: { inputSchema: { properties: { position: { description: '0-based UTF-16 position' } } } } }).length, 1);
  assert.deepEqual(zeroBasedTexts({ lspSearch: { description: '1-based line' } }), []);
  // Allow-listed by decision: offsets.
  assert.deepEqual(zeroBasedTexts({ ghCloneRepo: { inputSchema: { properties: { responseOffset: { description: 'a zero-based page index' } } } } }), []);
  assert.equal(zeroBasedTexts({ astSearch: { inputSchema: { properties: { page: { description: 'zero-based page' } } } } }).length, 1);
  // A clasify score is the index of its criterion, not a source coordinate.
  assert.deepEqual(zeroBasedTexts({ clasify: { inputSchema: { properties: { questions: { description: 'score: zero-based levels' } } } } }), []);
  assert.equal(zeroBasedTexts({ clasify: { inputSchema: { properties: { sections: { description: 'zero-based lines' } } } } }).length, 1);
  assert.deepEqual(zeroCoordinates({ results: [{ data: { matches: [{ line: 3, column: 0 }] } }] }), ['.results[0].data.matches[0].column=0']);
  assert.deepEqual(zeroCoordinates({ results: [{ data: { matches: [{ line: 3, column: 1 }] } }] }), []);
});

test('C3 distinguishes exact-range grammar from result coordinates', () => {
  const published = description => ({ astSearch: { inputSchema: { properties: { rule: { description } } } } });
  assert.deepEqual(zeroBasedTexts(published('range: exact 0-based scalar bounds; stopBy: neighbor/end/rule.')), []);
  assert.equal(zeroBasedTexts(published('range: exact 0-based scalar bounds; results use 0-based lines.')).length, 1);
  assert.equal(zeroBasedTexts(published('Returned matches have 0-based columns.')).length, 1);
});

test('C4 no silent omission: unlistedNested without next fails; with next or terminalLimit passes', () => {
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { unlistedNested: 53 } } }] }), ['results[0].data.stats.unlistedNested=53']);
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { unlistedNested: 53 }, next: { nested: { tool: 'lspSearch', query: { queries: [{}] } } } } }] }), []);
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { skippedFiles: 2, terminalLimit: true } } }] }), []);
  assert.deepEqual(silentOmissions({ results: [{ data: { stats: { skippedFiles: 0 } } }] }), []);
});

test('C5 description lint: every field a Next: note attributes to a tool is one it takes or the source returns', async t => {
  const fields = tool => (tool === 'localFetch' ? new Set(['matchString', 'line']) : new Set());
  for (const [name, description, reasons] of [
    ['neighbor with its field passes', 'Find text. Next: hit → localFetch matchString', []],
    ['terminal note naming no tool passes', 'Read lines. Next: cite lines by number', []],
    ['routing by purpose alone passes', 'Find text. Next: content → localFetch', []],
    ['missing note fails', 'Find text with localFetch matchString.', ['no Next: note']],
    ['a retired <workflow> tag is no note', 'Find text. <workflow>hit → localFetch matchString</workflow>', ['no Next: note']],
    // Core's workflow grammar names what the source returns to carry over.
    ['neighbor with a carried output field passes', 'Find text. Next: hit commitSha → localFetch', []],
    ['a field neither side has fails', 'Find text. Next: hit inventedKnob → localFetch line', ['inventedKnob: neither localFetch takes it nor localSearch returns it']],
    ['a known field word the neighbor lacks fails', 'Find text. Next: hit → localFetch ref', ['ref: neither localFetch takes it nor localSearch returns it']],
    ['a segment naming no tool is prose', 'Find text. Next: cite inventedKnob; hit → localFetch', []],
  ]) await t.test(name, () => assert.deepEqual(descriptionLint('localSearch', description, ['localSearch', 'localFetch'], fields, new Set(['commitSha'])), reasons));
});

const surfaceTools = profile => [
  ...Object.keys(MCP_SURFACE_BUDGETS.tools).map(name => ({
    name,
    ...(profile !== 'default' && name in MCP_SURFACE_BUDGETS.selectionAdditions ? { selection: 'fixture' } : {}),
  })),
  ...(profile === 'keyedBeta' ? [{ name: 'clasify' }] : []),
];

test('C5 checks all three core-owned surface totals, including array punctuation', async t => {
  const baseline = measureMcpSurface(surfaceTools('default'), '');
  for (const [profile, budget] of Object.entries(MCP_SURFACE_BUDGETS.totals)) {
    await t.test(profile, () => {
      const tools = surfaceTools(profile);
      assert.deepEqual(surfaceBudget(measureMcpSurface(tools, ''), { profile, baseline }).reasons, []);
      const toolsBytes = measureMcpSurface(tools, '').toolsList;
      for (const excess of [0, 1]) {
        const measured = measureMcpSurface(tools, 'x'.repeat(budget - toolsBytes + excess));
        const checked = surfaceBudget(measured, { profile, baseline });
        assert.equal(checked.total, budget + excess);
        assert.equal(checked.reasons.some(reason => reason.startsWith('surface ')), excess === 1);
      }
    });
  }
});

test('C5 default tool and instruction quotas reject one byte over', () => {
  const tools = surfaceTools('default');
  const target = tools.find(tool => tool.name === 'localSearch');
  target.description = '';
  const remaining = MCP_SURFACE_BUDGETS.tools.localSearch - measureMcpSurface([target], '').perTool.localSearch;
  target.description = 'x'.repeat(remaining);
  assert.deepEqual(surfaceBudget(measureMcpSurface(tools, '')).reasons, []);
  target.description += 'x';
  assert.ok(surfaceBudget(measureMcpSurface(tools, '')).reasons.some(reason => reason.startsWith('localSearch ')));
  for (const excess of [0, 1]) {
    const measured = measureMcpSurface(surfaceTools('default'), 'x'.repeat(MCP_SURFACE_BUDGETS.instructions + excess));
    assert.equal(surfaceBudget(measured).reasons.some(reason => reason.startsWith('instructions ')), excess === 1);
  }
});

test('C5 checks keyed reader growth even when clasify is absent', () => {
  const baseline = measureMcpSurface(surfaceTools('default'), '');
  const tools = surfaceTools('keyed');
  tools.find(tool => tool.name === 'localSearch').selection = 'x'.repeat(MCP_SURFACE_BUDGETS.selectionAdditions.localSearch);
  assert.ok(!tools.some(tool => tool.name === 'clasify'));
  assert.ok(surfaceBudget(measureMcpSurface(tools, ''), { profile: 'keyed', baseline }).reasons.some(reason => reason.startsWith('localSearch adds ')));
  const beta = surfaceTools('keyedBeta');
  // The published entry holds evaluate/models (`clasify`) plus section research (`sectionsAddition`).
  const clasify = beta.find(tool => tool.name === 'clasify');
  const quota = MCP_SURFACE_BUDGETS.clasify + MCP_SURFACE_BUDGETS.sectionsAddition;
  clasify.description = '';
  clasify.description = 'x'.repeat(quota - measureMcpSurface([clasify], '').perTool.clasify);
  assert.deepEqual(surfaceBudget(measureMcpSurface(beta, ''), { profile: 'keyedBeta', baseline }).reasons.filter(reason => reason.startsWith('clasify ')), []);
  clasify.description += 'x';
  assert.ok(surfaceBudget(measureMcpSurface(beta, ''), { profile: 'keyedBeta', baseline }).reasons.some(reason => reason.startsWith('clasify ')));
});

test('C5 missing tools, keyed selection fields, or a baseline cannot silently pass', () => {
  const baseline = measureMcpSurface(surfaceTools('default'), '');
  const missing = measureMcpSurface(surfaceTools('default').filter(tool => tool.name !== 'lspSearch'), '');
  assert.ok(surfaceBudget(missing).reasons.includes('missing tool lspSearch'));
  assert.ok(surfaceBudget(baseline, { profile: 'keyed', baseline }).reasons.some(reason => reason.includes('missing its keyed selection surface')));
  assert.ok(surfaceBudget(measureMcpSurface(surfaceTools('keyed'), ''), { profile: 'keyed' }).reasons.includes('keyed surface requires the default baseline'));
});
