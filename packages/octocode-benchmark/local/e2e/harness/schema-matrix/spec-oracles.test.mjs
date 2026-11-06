import assert from 'node:assert/strict';
import test, { after } from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { structureSearch, isEmptyEntry } from './spec-structure.mjs';
import { lspSearch } from './spec-lsp.mjs';
import { fixtureTexts } from './world-local.mjs';
import { runRow } from './runner.mjs';
import { planTool } from './derive.mjs';
import { coverageReport } from './acceptance.mjs';
import { SPECS } from './specs.mjs';
import { contract } from '../contract.mjs';

const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-matrix-oracle-'));
const plain = path.join(temporary, 'plain');
for (const [name, text] of Object.entries(fixtureTexts())) { const target = path.join(plain, name); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, text); }
const ctx = { world: { plain }, cwd: temporary, home: temporary };
fs.mkdirSync(path.join(temporary, '.octocode'));
fs.writeFileSync(path.join(temporary, '.octocode/mock-lsp.jsonl'), '');
after(() => fs.rmSync(temporary, { recursive: true, force: true }));
const queryCase = query => ({ query: { operation: 'files', ...query }, assignment: { root: 'plain' } });
const canonical = rows => rows.map(value => JSON.stringify(value)).sort();
const physicalEmpty = ([name, type]) => type === 'file' ? fs.statSync(path.join(plain, name)).size === 0 : fs.readdirSync(path.join(plain, name)).length === 0;

test('true and false partition the full fixture by actual emptiness; unset preserves both', () => {
  const all = structureSearch.truth(ctx, queryCase({}));
  const empty = structureSearch.truth(ctx, queryCase({ empty: true }));
  const nonempty = structureSearch.truth(ctx, queryCase({ empty: false }));
  assert.deepEqual(canonical(empty), canonical(all.filter(physicalEmpty)));
  assert.deepEqual(canonical(nonempty), canonical(all.filter(row => !physicalEmpty(row))));
  assert.deepEqual(canonical([...empty, ...nonempty]), canonical(all));
  assert.equal(new Set([...empty, ...nonempty].map(JSON.stringify)).size, all.length);
});

test('metadata controls empty selection, independently of filenames', () => {
  const renamedMeaning = path.join(plain, 'src/gamma.txt'); const original = fs.readFileSync(renamedMeaning);
  try {
    fs.writeFileSync(renamedMeaning, '');
    const selected = structureSearch.truth(ctx, queryCase({ empty: true }));
    const others = structureSearch.truth(ctx, queryCase({ empty: false }));
    assert(selected.some(([name]) => name === 'src/gamma.txt'));
    assert(!others.some(([name]) => name === 'src/gamma.txt'));
  } finally { fs.writeFileSync(renamedMeaning, original); }
});

test('emptiness distinguishes zero bytes, Unicode bytes, empty directories and directories containing empty files', () => {
  const directory = path.join(temporary, 'independent'); fs.mkdirSync(directory);
  const hollow = path.join(directory, 'hollow'); const filled = path.join(directory, 'filled'); fs.mkdirSync(hollow); fs.mkdirSync(filled);
  const empty = path.join(filled, 'zero'); const full = path.join(directory, 'unicode'); fs.writeFileSync(empty, ''); fs.writeFileSync(full, '😀');
  const rows = [{ type: 'file', full: empty, size: fs.statSync(empty).size }, { type: 'file', full, size: fs.statSync(full).size }, { type: 'directory', full: hollow }, { type: 'directory', full: filled }];
  assert.deepEqual(rows.map(row => isEmptyEntry(row)), [true, false, true, false]);
});

const lspTool = contract.tools.find(tool => tool.name === 'lspSearch');
const lspPlan = planTool(lspTool, lspSearch);
for (const row of lspPlan) { lspSearch.normalize?.(row); row.input = { queries: [row.query], ...(row.responseChars && { responseChars: row.responseChars }) }; }
const wantsExecution = query => query.rustContext?.buildScripts === true || query.rustContext?.procMacros === true;
const refusedRows = lspPlan.filter(row => wantsExecution(row.query));
const worker = (code, walkFailure = false) => ({
  surfaces: ['cli', 'mcp'], context: { ...ctx, providerHeaders: [] },
  call: async () => ({ json: { results: [{ index: 0, status: code ? 'error' : 'empty', data: code ? { errorCode: code, error: 'fixture refusal', lsp: { serverAvailable: false } } : { files: [] } }] } }),
  driver: { explore: async (tool, input, options) => { const result = await options.call(tool, input); return { root: result, failures: walkFailure ? [{ check: 'outputSchema', detail: 'injected invalid output', at: 'root' }] : [] }; } },
});

test('default host policy refuses either execution switch; absent/false contexts keep their positive oracle', () => {
  for (const rustContext of [{ buildScripts: true }, { procMacros: true }, { buildScripts: true, procMacros: true }]) assert.equal(lspSearch.refusal?.(ctx, { query: { rustContext } }), 'policy');
  for (const query of [{}, { rustContext: {} }, { rustContext: { buildScripts: false, procMacros: false } }, { rustContext: { features: 'all' } }]) assert.equal(lspSearch.refusal?.(ctx, { query }) ?? null, null);
  assert.equal(lspSearch.base({ operation: 'references', rustContext: { procMacros: true, buildScripts: false } }, {}), null, 'invalid combination remains excluded by the form constraint');
});

test('execution-enabled LSP cases require exact policy refusal on both transports, proving only blocked flags', async () => {
  assert(refusedRows.length > 0);
  const proved = new Map();
  for (const row of refusedRows) {
    const result = await runRow(worker('policy'), lspSearch, row);
    assert.deepEqual(result.failures, [], row.id); assert.equal(result.hasOracle, true);
    assert.deepEqual([...result.passed].sort(), ['cli', 'mcp']);
    assert.deepEqual(result.provedValues, row.values.filter(([field, value]) => ['rustContext.buildScripts', 'rustContext.procMacros'].includes(field) && value === true), 'a policy refusal proves only blocked switches');
    for (const [field, value] of result.provedValues) for (const surface of result.passed) { const key = `${row.tool}\0${field}\0${JSON.stringify(value)}`; const seen = proved.get(key) ?? new Set(); seen.add(surface); proved.set(key, seen); }
  }
  const coverage = coverageReport({ tools: [lspTool], proved, specs: { lspSearch }, surfaces: ['cli', 'mcp'] });
  assert(!coverage.uncovered.some(row => ['rustContext.buildScripts', 'rustContext.procMacros'].includes(row.field) && row.value === true));
});

test('refusal oracle rejects wrong codes, unexpected successful answers, and walk validation failures', async () => {
  const row = refusedRows[0];
  for (const code of ['serverUnavailable', 'invalidInput', null]) { const result = await runRow(worker(code), lspSearch, row); assert.equal(result.passed.size, 0); assert(result.failures.length > 0); }
  const malformed = await runRow(worker('policy', true), lspSearch, row); assert.equal(malformed.passed.size, 0); assert.equal(malformed.failures.length, 2);
});

test('every other LSP finite value retains a normal answer-oracle row', () => {
  const key = ([field, value]) => `${field}:${JSON.stringify(value)}`;
  const allValues = new Set(lspPlan.flatMap(row => row.values.map(key)));
  const supported = new Set(lspPlan.flatMap(row => (wantsExecution(row.query) ? lspSearch.refusalValues(row) : row.values).map(key)));
  assert.deepEqual([...supported].sort(), [...allValues].sort());
  for (const row of refusedRows) for (const [field, value] of lspSearch.refusalValues(row)) {
    assert.equal(value, true); assert(['rustContext.buildScripts', 'rustContext.procMacros'].includes(field));
  }
});

test('all finite schema values retain planned rows', () => {
  const plan = contract.tools.flatMap(tool => planTool(tool, SPECS[tool.name]));
  const proved = new Map();
  for (const row of plan) for (const [field, value] of row.values) {
    proved.set(`${row.tool}\0${field}\0${JSON.stringify(value)}`, new Set(['cli', 'mcp']));
  }
  const coverage = coverageReport({ tools: contract.tools, proved, specs: SPECS, surfaces: ['cli', 'mcp'] });
  assert.deepEqual(coverage.uncovered, []);
  assert.deepEqual(coverage.acceptedOnly, []);
  assert.equal(coverage.summary.cli.proved, coverage.summary.total);
  assert.equal(coverage.summary.mcp.proved, coverage.summary.total);
});
