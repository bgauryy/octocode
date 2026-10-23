// Local search/fetch evidence-selection edge cases against a loopback classifier,
// on fresh CLI processes and MCP. Protocol and evidence correctness, not model
// quality. The thirteen-tool selection contract lives in read-tool-selection.mjs;
// this suite keeps only the local edge cases that suite does not exercise.
// Run after native + CLI + MCP builds: node packages/octocode-benchmark/local/e2e/harness/local-ranking.mjs
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { findHint, nextHints, ROOT, RESULTS, startServer, expandShared, hermeticEnv } from './mcp-client.mjs';
import { contract, dataOf, failure, stable, success } from './read-tool-selection/support.mjs';

const fixture = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-selection-')));
const cliPath = path.join(ROOT, 'packages/octocode/out/octocode.js');
const marker = 'SEMANTIC_TARGET_EVIDENCE';
const background = 'background records';
const classifiers = ['Find evidence explaining cancellation of a waiting request'];
const searchDir = path.join(fixture, 'search');
fs.mkdirSync(searchDir);
// Two of five files carry the target so a one-file page still has a selected continuation.
for (let i = 0; i < 5; i++) {
  const lines = Array.from({ length: 6 - i }, (_, j) => `topic ${i}.${j}: ordinary bookkeeping details`);
  if (i === 2 || i === 4) lines.push(`topic: ${marker} A waiting request is cancelled before it starts (${i}).`);
  fs.writeFileSync(path.join(searchDir, `${i}.txt`), `${lines.join('\n')}\n`);
}
const docLines = Array.from({ length: 1200 }, (_, i) => `Entry ${i + 1}: ordinary ${background} ${'without requested evidence '.repeat(2)}`);
docLines[900] = `${marker} Cancelling a waiting request removes it before any work begins.`;
const docPath = path.join(fixture, 'large-document.txt');
fs.writeFileSync(docPath, `${docLines.join('\n')}\n`);

// Score answers follow the core three-level policy: level 2 selects, level 0 excludes. A question whose
// goal names the background records selects them, so a second classifier widens the selection.
let mode = 'score', calls = 0;
const sentStates = [], sentQuestions = [];
const mock = http.createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  calls++;
  const request = JSON.parse(Buffer.concat(chunks).toString());
  sentStates.push(request.state);
  sentQuestions.push(Object.values(request.questions ?? {}));
  if (mode === 'timeout') await new Promise(resolve => setTimeout(resolve, 500));
  if (['429', '500', '401'].includes(mode)) { res.writeHead(Number(mode), { 'content-type': 'application/json' }); res.end(JSON.stringify({ error: 'synthetic provider failure' })); return; }
  if (mode === 'malformed') { res.end('{broken'); return; }
  const state = JSON.stringify(request.state);
  const answers = Object.fromEntries(Object.entries(request.questions ?? {}).map(([id, q]) => {
    if (q.type === 'noul') return [id, { type: 'noul', noul: state.includes(marker) ? 0.99 : 0.01 }];
    assert.equal(q.type, 'score', 'selection uses a relevance score or yes/no question');
    const levels = q.criteria;
    const high = mode === 'tie' || state.includes(marker) || (JSON.stringify(q.instructions).includes(background) && state.includes(background));
    const score = high ? levels.length - 1 : 0;
    return [id, { type: 'score', score, confidence: 1,
      probabilities: Object.fromEntries(levels.map((_, i) => [String(i), Number(i === score)])),
      legend: Object.fromEntries(levels.map((level, i) => [String(i), level])),
    }];
  }));
  res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ model: request.model, answers, usage: { input_tokens: 10, output_tokens: 2 } }));
});
await new Promise(resolve => mock.listen(0, '127.0.0.1', resolve));
const isolated = hermeticEnv(process.env, 'local-ranking');
const env = {
  ...isolated,
  OCTOCODE_CLASSIFICATION_API: 'fixture-key',
  OCTOCODE_CLASSIFICATION_API_HOST: `http://127.0.0.1:${mock.address().port}`,
  OCTOCODE_BETA: 'true', OCTOCODE_STORAGE_MODE: 'memory', OCTOCODE_LSP_PREWARM: 'off',
  DISABLE_TOOLS: '', TOOLS_TO_RUN: '', ALLOWED_PATHS: fixture, WORKSPACE_ROOT: fixture,
};
const checks = [];
fs.mkdirSync(RESULTS, { recursive: true });
let mcp;

async function cliArgs(args, overrides = {}) {
  const started = performance.now();
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [cliPath, ...args], { cwd: fixture, env: { ...env, ...overrides } });
    let stdout = '', stderr = '';
    child.stdout.on('data', value => { stdout += value; });
    child.stderr.on('data', value => { stderr += value; });
    const timer = setTimeout(() => child.kill('SIGKILL'), 25000);
    child.on('error', reject);
    child.on('close', code => {
      clearTimeout(timer);
      try { resolve({ json: expandShared(JSON.parse(stdout)), code, chars: stdout.length, ms: performance.now() - started }); }
      catch { reject(new Error(`CLI ${code}: ${stderr.slice(0, 400)} ${stdout.slice(0, 400)}`)); }
    });
  });
}
const cli = (tool, query, overrides) => cliArgs([tool, JSON.stringify({ queries: [query] }), '--json'], overrides);
const viaMcp = async (tool, query) => { const result = await mcp.call(tool, query); return { json: result.sc, chars: result.text.length, ms: result.ms }; };
async function check(name, run) {
  if (process.argv[2] && !name.includes(process.argv[2])) return;
  mode = 'score';
  const started = performance.now();
  try { const detail = await run(); checks.push({ name, passed: true, ms: Math.round(performance.now() - started), detail }); console.log(`PASS ${name}`); }
  catch (error) { checks.push({ name, passed: false, ms: Math.round(performance.now() - started), error: error.message }); console.log(`FAIL ${name}: ${error.message.slice(0, 250)}`); }
  finally { mode = 'score'; }
}
const sourceRows = row => String(row.content ?? '').split('\n').filter(line => /^\d+\t/.test(line));
const matchRows = row => (row.files ?? []).flatMap(file => (file.matches ?? []).map(match => `${file.path}:${match.line}:${match.value ?? ''}`));
/** Hit lines of a search page: a snippet whose context reaches other hits lists them in `matchedLines`. */
const hitLines = row => (row.files ?? []).flatMap(file => (file.matches ?? []).flatMap(match => (match.matchedLines ?? [match.line]).map(line => `${file.path}:${line}`)));
const rowsOf = (tool, row) => tool === 'localFetch' ? sourceRows(row) : matchRows(row);
const lineNumber = row => Number(row.split('\t')[0]);
const PAGE_KINDS = ['nextPage', 'nextMatchPage', 'continueWindow'];
const continuation = row => Object.entries(row.next ?? {}).find(([kind]) => PAGE_KINDS.includes(kind))?.[1];

/** Every selected or ordinary page reachable from one query; `unfiltered` recovery is not a page. */
async function walk(call, tool, query) {
  const pending = [query], seen = new Set(), rows = [], hits = [], results = [], provider = [];
  while (pending.length) {
    assert.ok(results.length < 160, 'continuation walk terminates');
    const next = pending.shift();
    const id = JSON.stringify(next);
    // By design (local_search/cursor.rs) a file group's last page re-offers its first page's
    // idempotent nextPage; a walk of both axes calls it once. Any other repeated edge is a bug.
    if (seen.has(id) && next.page !== undefined && next.matchPage === undefined) continue;
    if (seen.has(id)) fs.writeFileSync(path.join(RESULTS, 'local-ranking-duplicate-edge.json'), JSON.stringify({ tool, initial: query, repeated: next, pending, rows, results }, null, 2));
    assert.ok(!seen.has(id), `a continuation edge must not repeat: ${id}`);
    seen.add(id);
    const before = calls;
    const result = await call(tool, next);
    const row = success(result);
    provider.push(calls - before);
    results.push(result);
    rows.push(...rowsOf(tool, row));
    if (tool === 'localSearch') hits.push(...hitLines(row));
    for (const [kind, lead] of Object.entries(row.next ?? {})) {
      if (!PAGE_KINDS.includes(kind)) continue;
      assert.equal(lead.tool, tool);
      assert.ok(Array.isArray(lead.query.queries), 'continuation carries the whole envelope');
      pending.push(...lead.query.queries);
    }
  }
  return { rows, hits, results, provider };
}
/** The ordinary read a classified query selects from: with no context field, classified localSearch judges hits with `limits.clasifyContextLines`. */
const ordinary = (query, tool) => {
  const { clasifyContext, selection, ...plain } = query;
  const implied = tool === 'localSearch' && clasifyContext && !['contextLines', 'contextBefore', 'contextAfter'].some(field => field in plain);
  return implied ? { ...plain, contextLines: contract.limits.clasifyContextLines } : plain;
};

/** A selected-view receipt whose partitions reconcile. */
function receipt(row, unit) {
  const value = row.selection;
  assert.ok(value, `classified output carries a selection receipt: ${JSON.stringify(row).slice(0, 300)}`);
  assert.ok(['complete', 'partial', 'unavailable'].includes(value.status), value.status);
  assert.equal(value.scope, 'selectedView');
  assert.equal(value.unit, unit);
  for (const field of ['totalItems', 'selectedItems', 'excludedItems', 'unknownItems'])
    assert.ok(Number.isInteger(value[field]) && value[field] >= 0, `${field} is a nonnegative integer`);
  assert.ok(value.relatedItems === undefined || (Number.isInteger(value.relatedItems) && value.relatedItems > 0), 'relatedItems (decided near-misses, hidden) is omitted or positive');
  assert.equal(value.selectedItems + value.excludedItems + (value.relatedItems ?? 0) + value.unknownItems, value.totalItems, 'selection partitions reconcile');
  return value;
}

/** Selected rows are a source-ordered subsequence of the ordinary rows. */
function assertSubsequence(selected, baseline) {
  let cursor = 0;
  for (const row of selected) {
    cursor = baseline.indexOf(row, cursor);
    assert.notEqual(cursor, -1, `selected evidence keeps its exact ordinary anchor and source order: ${row.slice(0, 120)}`);
    cursor += 1;
  }
}

/** next.unfiltered strips classification and reproduces the ordinary walk without provider work. */
async function assertRecovery(tool, row, baseline, overrides) {
  const lead = row.next?.unfiltered;
  assert.ok(lead, 'classified output exposes next.unfiltered');
  assert.equal(lead.tool, tool);
  const query = lead.query.queries[0];
  assert.equal(query.clasifyContext, undefined, 'recovery removes classification context');
  assert.equal(query.selection, undefined, 'recovery removes the selection plan');
  const before = calls;
  const recovered = await walk((t, q) => cli(t, q, overrides), tool, query);
  assert.equal(calls, before, 'unfiltered recovery performs no provider calls');
  assert.ok(recovered.results.every(result => dataOf(result).selection === undefined), 'recovery is an ordinary projection');
  if (baseline) assert.deepEqual(recovered.rows, baseline.rows, 'unfiltered recovery restores the complete ordinary evidence');
  return recovered;
}

function assertRejected(result, why) {
  assert.equal(result.code, 2, `${why}: ${JSON.stringify(result.json).slice(0, 300)}`);
  assert.equal(result.json.errorCode ?? dataOf(result)?.errorCode, 'invalidInput', why);
}
const staleCode = result => dataOf(result)?.errorCode ?? result.json.errorCode;

const search = { path: searchDir, matchString: 'topic', clasifyContext: { classifiers }, pageSize: 1, matchPageSize: 100 };
// A bounded slice keeps ordinary baselines cheap; the target line 901 is inside it.
const fetch = { path: docPath, ranges: ['851-950'], clasifyContext: { classifiers }, unit: 'lines', length: 10 };
try {
  // Removed: `OCTOCODE_RANKING_ENABLED=false` hid the surface; the gate is gone
  // and read-tool-selection "CLI/MCP selection schema gating: removed ranking gate ignored" owns it.
  for (const [label, overrides, shown] of [
    ['enabled', {}, true],
    ['missing key', { OCTOCODE_CLASSIFICATION_API: '' }, false],
    ['blank key', { OCTOCODE_CLASSIFICATION_API: '   ' }, false],
    ['clasify hidden', { DISABLE_TOOLS: 'clasify' }, true],
    ['beta off', { OCTOCODE_BETA: 'false' }, true],
    ['read-only allowlist', { TOOLS_TO_RUN: 'localSearch,localFetch' }, true],
  ]) {
    await check(`schema: ${label} on CLI query/full views and fresh MCP`, async () => {
      const server = await startServer({ env: { ...env, ...overrides }, cwd: fixture, timeoutMs: 25000 });
      try {
        for (const tool of ['localSearch', 'localFetch']) {
          const mcpSchema = server.tools.find(entry => entry.name === tool)?.inputSchema.properties?.queries?.items;
          assert.ok(mcpSchema, `${tool} is exposed`);
          assert.equal(Boolean(mcpSchema.properties?.clasifyContext), shown);
          assert.ok(!mcpSchema.required?.includes('clasifyContext'), 'clasifyContext stays optional');
          assert.equal(mcpSchema.properties?.selection, undefined, 'MCP keeps the opaque plan out of discovery');
          for (const legacy of ['classifiers', 'ranking']) assert.equal(mcpSchema.properties?.[legacy], undefined, `no legacy ${legacy} input`);
          for (const view of ['query', 'full']) {
            const result = await cliArgs(['schema', tool, '--view', view], overrides);
            assert.equal(result.code, 0);
            const properties = result.json.querySchema?.properties ?? {};
            assert.equal(Boolean(properties.clasifyContext), shown);
            assert.ok(!result.json.querySchema.required?.includes('clasifyContext'));
            const continuationFields = view === 'query' ? result.json.continuationFields : properties;
            assert.equal(Boolean(continuationFields?.selection), shown, `${view} view advertises the selection plan iff selection is available`);
            for (const legacy of ['classifiers', 'ranking']) {
              assert.equal(properties[legacy], undefined, `no legacy ${legacy} input`);
              assert.equal(continuationFields?.[legacy], undefined, `no legacy ${legacy} continuation`);
            }
          }
        }
      } finally { server.close(); }
    });
  }
  // MCP discovery never contacts the provider: a failing provider keeps every
  // advertised surface and surfaces per call as an unavailable selection.
  for (const overrides of [{}, { DISABLE_TOOLS: 'clasify' }]) await check(`schema: a failing provider is not probed at discovery and fails per call (${Object.keys(overrides)[0] ?? 'default'})`, async () => {
    mode = '401';
    const before = calls;
    const server = await startServer({ env: { ...env, ...overrides }, cwd: fixture, timeoutMs: 25000 });
    try {
      assert.equal(calls, before, 'discovery makes no provider call');
      assert.equal(server.tools.some(tool => tool.name === 'clasify'), !overrides.DISABLE_TOOLS);
      for (const name of ['localSearch', 'localFetch']) {
        const schema = server.tools.find(tool => tool.name === name)?.inputSchema.properties.queries.items;
        assert.ok(schema?.properties.clasifyContext, `${name} keeps clasifyContext`);
      }
      const result = await server.call('localSearch', search);
      const row = failure({ json: result.sc, isError: result.isError }, 'classificationProviderError');
      const value = receipt(row, 'chunks');
      // Per-candidate provider errors are unknown decisions (judgment/select.rs): partial, never excluded.
      assert.ok(['partial', 'unavailable'].includes(value.status), value.status);
      assert.equal(value.selectedItems, 0);
      assert.equal(value.excludedItems, 0);
      assert.ok(value.unknownItems > 0 && value.unknownItems === value.totalItems, 'failed judgments are unknown, not excluded');
      assert.deepEqual(matchRows(row), [], 'no unjudged body is returned');
      assert.ok(row.next?.unfiltered, 'failure keeps the explicit unfiltered recovery');
      assert.ok(calls > before, 'the call attempts the provider');
    } finally { server.close(); }
  });
  await check('schema: no usable semantic surface makes no startup provider call', async () => {
    for (const overrides of [
      { TOOLS_TO_RUN: 'ghGetHistoryItem' },
      { OCTOCODE_CLASSIFICATION_API: '' },
      { OCTOCODE_CLASSIFICATION_API: '   ', DISABLE_TOOLS: 'clasify' },
    ]) {
      const before = calls;
      const server = await startServer({ env: { ...env, ...overrides }, cwd: fixture, timeoutMs: 25000 });
      try {
        assert.equal(calls, before, 'no usable semantic surface means no probe');
        assert.ok(!server.tools.some(tool => tool.name === 'clasify'));
        assert.ok(!JSON.stringify(server.tools.map(tool => tool.inputSchema)).includes('clasifyContext'), `no tool advertises clasifyContext: ${JSON.stringify(overrides)}`);
      } finally { server.close(); }
    }
  });
  for (const overrides of [{ DISABLE_TOOLS: 'clasify' }, { OCTOCODE_BETA: 'false' }]) await check(`hidden clasify (${Object.keys(overrides)[0]}): clasifyContext works, public calls and handoffs stay disabled`, async () => {
    const server = await startServer({ env: { ...env, ...overrides }, cwd: fixture, timeoutMs: 25000 });
    try {
      assert.ok(!server.tools.some(tool => tool.name === 'clasify'));
      const publicQuery = { state: { evidence: 'waiting request' }, questions: { q: { type: 'noul', instructions: 'Does the evidence mention a waiting request?' } } };
      const beforePublic = calls;
      const cliRejected = await cli('clasify', publicQuery, overrides);
      assert.notEqual(cliRejected.code, 0, 'CLI rejects hidden public clasify');
      assert.equal(cliRejected.json.errorCode, overrides.DISABLE_TOOLS ? 'toolUnavailable' : 'missingConfiguration', JSON.stringify(cliRejected.json));
      const mcpRejected = await server.raw('clasify', { queries: [publicQuery] });
      assert.ok(mcpRejected.isError || mcpRejected.sc?.errorCode, 'MCP rejects hidden public clasify');
      assert.equal(calls, beforePublic, 'rejected public calls do not send evidence');
      // Retrieval emits no clasify handoff on any surface (RFC: Scout/handoff flows removed).
      const plain = { ...ordinary(search, 'localSearch'), matchString: 'ordinary bookkeeping', pageSize: 5, extras: ['hints'], mainGoal: classifiers[0] };
      for (const result of [await cli('localSearch', plain), await cli('localSearch', plain, overrides), { json: (await server.call('localSearch', plain)).sc }]) {
        const row = success(result);
        assert.equal(findHint(row, 'hints.clasify'), undefined);
        assert.equal(row.next?.clasify, undefined);
        assert.ok(!JSON.stringify(result.json).includes('"tool":"clasify"'), 'retrieval returns no clasify lead');
      }
      const beforeSelect = calls;
      const cliSelected = success(await cli('localSearch', search, overrides));
      const mcpSelected = success({ json: (await server.call('localSearch', search)).sc });
      for (const row of [cliSelected, mcpSelected]) {
        const value = receipt(row, 'chunks');
        assert.equal(value.selectedItems, 2);
        assert.equal(value.excludedItems, 3);
        assert.ok(matchRows(row).some(line => line.includes(marker)), 'selected target evidence is returned');
        assert.ok(matchRows(row).every(line => !line.includes('topic 0.')), 'excluded evidence stays hidden');
      }
      assert.deepEqual(cliSelected.selection, mcpSelected.selection, 'CLI and MCP agree');
      assert.ok(calls > beforeSelect, 'clasifyContext invokes the provider with public clasify hidden');
      return { tools: server.tools.length, discoveryChars: JSON.stringify(server.tools).length, instructionChars: server.init.instructions?.length ?? 0 };
    } finally { server.close(); }
  });
  for (const [label, query, tool] of [['search', search, 'localSearch'], ['fetch', fetch, 'localFetch']]) {
    await check(`${label}: missing provider gates reject clasifyContext with no provider calls`, async () => {
      for (const overrides of [{ OCTOCODE_CLASSIFICATION_API: '' }, { OCTOCODE_CLASSIFICATION_API: '   ' }, { OCTOCODE_CLASSIFICATION_API: undefined }, { DISABLE_TOOLS: 'clasify', OCTOCODE_CLASSIFICATION_API: undefined }]) {
        const before = calls;
        assertRejected(await cli(tool, query, overrides), JSON.stringify(overrides));
        assert.equal(calls, before);
      }
      const before = calls;
      assert.equal(success(await cli(tool, ordinary(query, tool))).selection, undefined, 'ordinary reads carry no receipt');
      assert.equal(calls, before);
    });
    await check(`${label}: selected evidence only, source order, fresh-process replay and lossless recovery`, async () => {
      const baseline = await walk(cli, tool, ordinary(query, tool));
      const before = calls;
      const selected = await walk(cli, tool, query);
      assert.ok(selected.provider[0] > 0, 'the first page judges the candidates');
      assert.ok(selected.results.length > 1, 'the selected view spans several pages');
      assert.ok(selected.provider.slice(1).every(n => n === 0), 'a fresh CLI continuation must not classify again');
      const first = dataOf(selected.results[0]);
      const lead = continuation(first);
      assert.ok(!path.isAbsolute(lead.query.queries[0].path), 'response normalizes an absolute source path to its workspace-relative continuation');
      assert.ok(lead.query.queries[0].selection, 'the continuation carries the saved plan');
      assert.deepEqual(lead.query.queries[0].clasifyContext, query.clasifyContext, 'the continuation keeps clasifyContext');
      const value = receipt(first, 'chunks');
      assert.ok(value.selectedItems > 0 && value.excludedItems > 0, JSON.stringify(value));
      for (const result of selected.results) assert.deepEqual(dataOf(result).selection, value, 'every page reports the same plan');
      assert.ok(selected.rows.some(row => row.includes(marker)), 'selected pages contain the target evidence');
      if (tool === 'localSearch') assert.ok(selected.rows.every(row => /(^|\/)(2|4)\.txt:/.test(row)), `non-target files are excluded whole: ${JSON.stringify(selected.rows)}`);
      else assert.ok(selected.rows.length < baseline.rows.length, 'excluded windows are withheld');
      assert.equal(new Set(selected.rows).size, selected.rows.length, 'no duplicated source anchors');
      assertSubsequence(selected.rows, baseline.rows);
      fs.writeFileSync(path.join(RESULTS, `local-ranking-${label}-walk.json`), JSON.stringify({ baseline, selected }, null, 2));
      await assertRecovery(tool, first, baseline);
      return { pages: selected.results.length, providerCalls: calls - before, evidenceRows: selected.rows.length, baselineRows: baseline.rows.length };
    });
    await check(`${label}: refused provider calls expose no unjudged body; a provider that cannot answer now withholds nothing`, async () => {
      const baseline = await walk(cli, tool, ordinary(query, tool));
      const attempts = [];
      // A selection past its constant deadline is covered by
      // runtime_clasify::a_selection_past_its_deadline_withholds_bodies_and_recovers.
      // Refused calls (credential, quota, malformed answer) fail the row and offer next.unfiltered.
      for (const failureMode of ['401', 'malformed']) {
        mode = failureMode;
        const startedCalls = calls;
        const row = failure(await cli(tool, query), failureMode === 'malformed'
          ? 'invalidClassificationResponse' : 'classificationProviderError');
        const value = receipt(row, 'chunks');
        attempts.push({ failure: failureMode, providerCalls: calls - startedCalls, status: value.status, reason: value.reason });
        assert.ok(calls > startedCalls, `${failureMode} attempted the provider`);
        assert.ok(['partial', 'unavailable'].includes(value.status), failureMode);
        assert.equal(value.selectedItems, 0, failureMode);
        assert.equal(value.excludedItems, 0, `${failureMode}: a failed judgment never excludes`);
        assert.equal(value.unknownItems, value.totalItems, failureMode);
        assert.deepEqual(rowsOf(tool, row), [], `${failureMode}: no raw fallback body`);
        assert.equal(continuation(row)?.query.queries[0].selection, undefined, `${failureMode}: no plan-bound continuation of an empty view`);
        mode = 'score';
        await assertRecovery(tool, row, baseline);
      }
      // A provider that cannot answer now (rate limited, failing with 5xx) is transient
      // (judgment/transport.rs is_transient): evidence the reader already has is never withheld, so every page is
      // the ordinary view under an `unavailable` receipt and a warning naming the code. Later pages replay the saved
      // plan without another provider call, and there is nothing to recover.
      for (const [failureMode, code] of [['429', 'classificationRateLimited'], ['500', 'classificationProviderUnavailable']]) {
        mode = failureMode;
        const startedCalls = calls;
        const transient = await walk(cli, tool, query);
        mode = 'score';
        attempts.push({ failure: failureMode, providerCalls: calls - startedCalls, pages: transient.results.length });
        assert.ok(calls > startedCalls, `${failureMode} attempted the provider`);
        assert.ok(transient.provider.slice(1).every(count => count === 0), `${failureMode}: later pages replay without provider calls`);
        assert.deepEqual(transient.rows, baseline.rows, `${failureMode}: the complete ordinary evidence is shown, once and in order`);
        for (const result of transient.results) {
          const row = dataOf(result);
          const value = receipt(row, 'chunks');
          assert.equal(value.status, 'unavailable', failureMode);
          assert.equal(value.reason, code, failureMode);
          assert.equal(value.selectedItems, 0, failureMode);
          assert.equal(value.excludedItems, 0, `${failureMode}: a failed judgment never excludes`);
          assert.equal(value.unknownItems, value.totalItems, failureMode);
          // The first page warns; later pages replay the plan and carry its receipt reason.
          if (result === transient.results[0]) assert.ok((row.warnings ?? []).some(warning => warning.includes(code) && warning.includes('shown unfiltered')), `${failureMode} warns that the view is unfiltered: ${JSON.stringify(row.warnings)}`);
          assert.equal(row.next?.unfiltered, undefined, `${failureMode}: the view is already complete, so there is no unfiltered recovery`);
        }
      }
      return attempts;
    });
    await check(`${label}: malformed plan, changed classifiers or changed source view reject replay; output paging does not`, async () => {
      const row = success(await cli(tool, query));
      const lead = continuation(row);
      assert.ok(lead?.query.queries[0].selection, 'a plan-bound continuation exists');
      const saved = lead.query.queries[0];
      const before = calls;
      const sourceView = tool === 'localFetch' ? { ranges: ['851-951'] } : { matchString: 'topic:' };
      for (const change of [{ selection: 'bad-plan' }, { clasifyContext: { classifiers: ['A different classifier'] } }, { mainGoal: 'A changed goal' }, sourceView]) {
        const result = await cli(tool, { ...saved, ...change });
        assert.equal(staleCode(result), 'staleSnapshot', JSON.stringify(change));
        assert.ok(dataOf(result)?.next?.restart, 'stale replay offers restart');
      }
      if (tool === 'localFetch') {
        // The plan binds the assessed view, not the response pager (selection.rs binding).
        const repaged = success(await cli(tool, { ...saved, length: 20 }));
        assert.deepEqual(repaged.selection, row.selection, 'a changed output window replays the same plan');
        assert.ok(sourceRows(repaged).length > 0);
      }
      assert.equal(calls, before, 'rejected or repaged replays make no provider calls');
    });
  }
  await check('content-free result views and minified fetch reject clasifyContext before provider work; fullContent still selects', async () => {
    // countMatches, unit:bytes and minify:standard are owned by read-tool-selection "incompatible views fail before provider work".
    for (const [tool, query] of [['localSearch', { ...search, resultView: 'files' }], ['localSearch', { ...search, resultView: 'filesWithout' }], ['localSearch', { ...search, resultView: 'countLines' }], ['localFetch', { path: docPath, clasifyContext: { classifiers }, minify: 'symbols' }]]) {
      const before = calls;
      const result = await cli(tool, query);
      assertRejected(result, JSON.stringify(query));
      const recovery = findHint(result.json, 'repair')?.query?.queries?.[0] ??
        dataOf(result)?.next?.unfiltered?.query?.queries?.[0];
      assert.ok(recovery, 'rejection supplies an executable repair');
      assert.equal(calls, before, 'validation does not call the provider');
      success(await cli(tool, recovery));
    }
    const before = calls;
    const row = success(await cli('localFetch', { path: path.join(searchDir, '4.txt'), clasifyContext: { classifiers }, fullContent: true }));
    const value = receipt(row, 'chunks');
    assert.equal(value.selectedItems, 1);
    assert.ok(row.content.includes(marker));
    assert.ok(calls > before, 'fullContent is a raw line view and is judged');
  });
  await check('empty selections disclose no candidates without provider calls', async () => {
    for (const [tool, query] of [['localSearch', { ...search, matchString: 'NO_MATCH_EXISTS' }], ['localFetch', { ...fetch, matchString: 'NO_MATCH_EXISTS' }]]) {
      const before = calls;
      const row = success(await cli(tool, query));
      const value = receipt(row, 'chunks');
      assert.equal(value.status, 'complete', tool);
      assert.equal(value.totalItems, 0, tool);
      assert.equal(calls, before, tool);
    }
  });
  // The candidate budget is a runtime constant (64): 66 files overflow it by two.
  await check('candidate budget judges exactly the admitted prefix and keeps the tail unknown', async () => {
    const budgetDir = path.join(fixture, 'budget');
    fs.mkdirSync(budgetDir, { recursive: true });
    for (let i = 0; i < 66; i++) {
      const tail = i === 65 ? ` ${marker} A waiting request is cancelled before it starts.` : '';
      fs.writeFileSync(path.join(budgetDir, `f${String(i).padStart(2, '0')}.txt`), `topic ${i}: ordinary bookkeeping details${tail}\n`);
    }
    const query = { ...search, path: budgetDir, pageSize: 70, sort: 'path' };
    const baseline = await walk(cli, 'localSearch', ordinary(query, 'localSearch'));
    const before = calls;
    const limited = await walk(cli, 'localSearch', query);
    const value = receipt(dataOf(limited.results[0]), 'chunks');
    assert.equal(calls - before, 64, 'one provider request per admitted candidate');
    assert.equal(value.status, 'partial');
    assert.equal(value.reason, 'candidateBudget');
    assert.equal(value.selectedItems + value.excludedItems, 64);
    assert.equal(value.unknownItems, 2);
    assert.ok(limited.rows.every(row => !row.includes('f65.txt')), 'the unjudged tail is not returned');
    await assertRecovery('localSearch', dataOf(limited.results[0]), baseline);
  });
  // The character budget (131,072) bounds outgoing evidence in
  // selection::tests::candidate_budget_counts_serialized_characters_in_source_order and
  // local_fetch::selection::tests::exhausted_state_budget_withholds_bodies_and_unfiltered_recovers_every_line.
  await check('an all-selected view preserves original order across both pagination axes', async () => {
    mode = 'tie';
    const query = { ...search, pageSize: 2, matchPageSize: 2 };
    const baseline = await walk(cli, 'localSearch', ordinary(query, 'localSearch'));
    const selected = await walk(cli, 'localSearch', query);
    const expectedCount = fs.readdirSync(searchDir).reduce((sum, file) => sum + fs.readFileSync(path.join(searchDir, file), 'utf8').split('\n').filter(line => line.includes('topic')).length, 0);
    assert.equal(baseline.hits.length, expectedCount, 'every fixture match is reached');
    assert.equal(new Set(baseline.hits).size, expectedCount, 'each original match occurs once');
    assert.equal(new Set(baseline.rows).size, baseline.rows.length, 'each snippet occurs once');
    assert.deepEqual(selected.rows, baseline.rows);
    const value = receipt(dataOf(selected.results[0]), 'chunks');
    assert.equal(value.status, 'complete');
    assert.equal(value.selectedItems, value.totalItems);
    assert.ok(selected.provider.slice(1).every(n => n === 0), 'match and file pages replay the plan');
    fs.writeFileSync(path.join(RESULTS, 'local-ranking-tied-walk.json'), JSON.stringify({ expectedCount, baseline, selected }, null, 2));
  });
  await check('fetch defaults and multiple classifiers survive continuation replay', async () => {
    const both = { classifiers: [...classifiers, `Find the ordinary ${background}`] };
    const query = { path: docPath, clasifyContext: both };
    const before = calls;
    const selected = await walk(cli, 'localFetch', query);
    const firstCalls = selected.provider[0];
    assert.ok(firstCalls > 0);
    assert.equal(calls - before, firstCalls, 'only the first page judges');
    assert.ok(sentQuestions.slice(-firstCalls).every(questions => questions.length === 2), 'each candidate is asked every classifier');
    for (const result of selected.results.slice(0, -1)) {
      const saved = continuation(dataOf(result)).query.queries[0];
      assert.deepEqual(saved.clasifyContext, both, 'continuations keep every classifier');
      assert.ok(saved.selection);
    }
    const value = receipt(dataOf(selected.results[0]), 'chunks');
    assert.equal(value.status, 'complete');
    assert.equal(value.selectedItems, value.totalItems, 'any direct classifier selects a window');
    const baseline = await walk(cli, 'localFetch', ordinary(query, 'localFetch'));
    assert.deepEqual(selected.rows, baseline.rows);
    assert.equal(new Set(selected.rows).size, 1200);
  });
  await check('missing plans and changed classification settings reject replay', async () => {
    for (const [tool, query] of [['localSearch', search], ['localFetch', fetch]]) {
      const row = success(await cli(tool, query));
      const { selection: omitted, ...withoutPlan } = continuation(row).query.queries[0];
      assert.ok(omitted);
      const before = calls;
      assert.equal(staleCode(await cli(tool, withoutPlan)), tool === 'localFetch' ? 'invalidInput' : 'staleSnapshot', `${tool} without plan`);
      // Changed selection limits (runtime constants) re-bind the plan:
      // selection::tests::changed_selection_limits_invalidate_the_plan_binding.
      assert.equal(calls, before, 'stale replays make no provider calls');
    }
  });
  await check('Unicode, CRLF, missing final newline and limited fetch preserve exact anchors', async () => {
    const edgePath = path.join(fixture, 'unicode.txt');
    const lines = Array.from({ length: 160 }, (_, i) => `Row ${i}: שלום 世界 🐙 ${'background '.repeat(8)}`);
    lines[135] = `${marker} בקשה מבוטלת before it starts.`;
    fs.writeFileSync(edgePath, lines.join('\r\n'));
    const query = { path: edgePath, clasifyContext: { classifiers }, unit: 'lines', length: 17 };
    const baseline = await walk(cli, 'localFetch', ordinary(query, 'localFetch'));
    assert.equal(new Set(baseline.rows).size, 160);
    const selected = await walk(cli, 'localFetch', query);
    assertSubsequence(selected.rows, baseline.rows);
    assert.ok(selected.rows.some(row => lineNumber(row) === 136 && row.includes(marker)), 'the target keeps its source line');
    assert.ok(selected.rows.length < 160);
    await assertRecovery('localFetch', dataOf(selected.results[0]), baseline);
    // An over-budget tail on such a file stays hidden and recoverable in
    // local_fetch::selection::tests::exhausted_state_budget_withholds_bodies_and_unfiltered_recovers_every_line.
  });
  await check('streaming files reject clasifyContext and recover through the unfiltered read', async () => {
    const streamPath = path.join(fixture, 'stream.txt');
    fs.writeFileSync(streamPath, 'Large streaming source evidence.\n'.repeat(350000));
    const before = calls;
    const rejected = await cli('localFetch', { path: streamPath, clasifyContext: { classifiers }, unit: 'lines', length: 10 });
    assertRejected(rejected, 'streamed source');
    const lead = dataOf(rejected).next?.unfiltered;
    assert.ok(lead, 'rejection names the unfiltered read');
    const row = success(await cli('localFetch', lead.query.queries[0]));
    assert.equal(row.selection, undefined);
    const next = success(await cli('localFetch', row.next.continueWindow.query.queries[0]));
    assert.equal(calls, before);
    // Recovery drops the output pager (length), so the streamed default window applies.
    assert.equal(lineNumber(sourceRows(row)[0]), 1);
    assert.equal(lineNumber(sourceRows(next)[0]), lineNumber(sourceRows(row).at(-1)) + 1, 'the ordinary continuation resumes after the recovered window');
  });
  await check('disjoint fetch ranges retain source anchors after selection', async () => {
    const query = { path: docPath, clasifyContext: { classifiers }, ranges: ['5-35', '885-925'], unit: 'lines', length: 31 };
    const baseline = await walk(cli, 'localFetch', ordinary(query, 'localFetch'));
    const selected = await walk(cli, 'localFetch', query);
    assertSubsequence(selected.rows, baseline.rows);
    assert.ok(selected.rows.some(row => lineNumber(row) === 901 && row.includes(marker)));
    assert.ok(selected.rows.every(row => lineNumber(row) >= 885 && lineNumber(row) <= 925), 'the unrelated span is excluded');
    const value = receipt(dataOf(selected.results[0]), 'chunks');
    assert.ok(value.excludedItems > 0);
  });
  await check('matchString hit windows are judged separately and keep exact anchors', async () => {
    const query = { path: docPath, matchString: ['Entry 120: ', marker], contextLines: 2, clasifyContext: { classifiers }, unit: 'lines', length: 4 };
    const baseline = await walk(cli, 'localFetch', ordinary(query, 'localFetch'));
    assert.ok(baseline.rows.some(row => lineNumber(row) === 120) && baseline.rows.some(row => lineNumber(row) === 901), 'both hit windows are ordinary evidence');
    const selected = await walk(cli, 'localFetch', query);
    assertSubsequence(selected.rows, baseline.rows);
    assert.ok(selected.rows.some(row => lineNumber(row) === 901 && row.includes(marker)));
    assert.ok(selected.rows.every(row => lineNumber(row) >= 899 && lineNumber(row) <= 903), 'the unrelated hit window is excluded');
    const value = receipt(dataOf(selected.results[0]), 'chunks');
    assert.equal(value.selectedItems, 1);
    assert.equal(value.excludedItems, 1);
    await assertRecovery('localFetch', dataOf(selected.results[0]), baseline);
  });
  await check('provider receives sanitized search and fetch evidence', async () => {
    const secretDir = path.join(fixture, 'secrets');
    fs.mkdirSync(secretDir);
    const secretPath = path.join(secretDir, 'example.txt');
    const password = 'hunter2secretpw';
    const body = 'MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
    fs.writeFileSync(secretPath, `topic postgres://admin:${password}@db.example.com/app\n-----BEGIN RSA PRIVATE KEY-----\n${`${body}\n`.repeat(100)}-----END RSA PRIVATE KEY-----\n`);
    fs.writeFileSync(path.join(secretDir, 'public.txt'), 'topic public documentation.\n');
    const before = sentStates.length;
    receipt(success(await cli('localSearch', { path: secretDir, matchString: 'topic', clasifyContext: { classifiers } })), 'chunks');
    receipt(success(await cli('localFetch', { path: secretPath, clasifyContext: { classifiers }, unit: 'lines', length: 10 })), 'chunks');
    const outgoing = JSON.stringify(sentStates.slice(before));
    assert.ok(sentStates.length > before);
    assert.ok(!outgoing.includes(password));
    assert.ok(!outgoing.includes('interiorKeyBody'));
  });
  await check('provider paths use workspace-relative identities for search and fetch', async () => {
    for (const [tool, query] of [['localSearch', search], ['localFetch', { ...fetch, ranges: undefined }]]) {
      const before = sentStates.length;
      const value = receipt(success(await cli(tool, query)), 'chunks');
      const states = sentStates.slice(before);
      assert.ok(states.length > 0, `${tool} sends candidate states`);
      assert.equal(states.length, value.totalItems - value.unknownItems, `${tool} sends one state per admitted candidate`);
      for (const state of states) {
        assert.equal(typeof state.path, 'string', `${tool} names its candidate path`);
        assert.equal(path.isAbsolute(state.path), false, `${tool} sent an absolute provider path`);
        assert.ok(!JSON.stringify(state).includes(fixture), 'local workspace prefix stays private');
      }
    }
  });
  await check('clasify source judges one captured selection with no selection pass and a pinned read lead', async () => {
    const before = calls;
    const start = sentQuestions.length;
    const result = await cli('clasify', {
      source: { tool: 'localFetch', query: { path: docPath, ranges: ['880-920'] } },
      questions: { answer: { type: 'noul', instructions: 'Does this evidence explain cancellation of a waiting request?' } },
    });
    assert.equal(result.code, 0, JSON.stringify(result.json));
    const row = result.json.queries[0];
    assert.equal(row.answers.answer.type, 'noul');
    assert.equal(row.answers.answer.noul, 0.99, 'the captured selection includes the target line');
    assert.equal(calls - before, 1, 'one provider request for one selected state');
    assert.deepEqual(sentQuestions.slice(start).flat().map(question => question.type), ['noul'], 'no selection question is sent');
    assert.ok(!JSON.stringify(row).includes('Cancelling a waiting request'), 'no source body is returned');
    const read = findHint(row, 'read');
    assert.equal(read.tool, 'localFetch');
    const exact = read.query.queries[0];
    assert.deepEqual(exact.ranges, ['880-920']);
    assert.equal(exact.minify, undefined);
    assert.ok(exact.snapshot, 'the lead pins the captured snapshot');
    assert.equal(exact.clasifyContext, undefined, 'source verification does not add a selection stage');
    assert.equal(exact.selection, undefined, 'source verification carries no selection plan');
    const lines = sourceRows(success(await cli(read.tool, exact)));
    assert.equal(lines.length, 41);
    assert.ok(lines.some(line => line.includes(marker)));
    assert.equal(calls - before, 1, 'verification makes no provider call');
  });
  await check('nested clasifyContext in a clasify source and legacy inputs are rejected before provider access', async () => {
    const before = calls;
    const nested = await cli('clasify', {
      source: { tool: 'localFetch', query: { path: docPath, ranges: ['1-10'], clasifyContext: { classifiers } } },
      questions: { answer: { type: 'noul', instructions: 'Is cancellation explained?' } },
    });
    assert.equal(nested.json.errorCode, 'invalidInput', JSON.stringify(nested.json));
    const legacy = await cli('clasify', {
      resources: [{ id: 'document', tool: 'localFetch', query: { path: docPath } }],
      questions: [{ id: 'answer', type: 'yesno', ask: 'Is cancellation explained?' }],
    });
    assert.equal(legacy.code, 2);
    assert.equal(legacy.json.errorCode, 'invalidInput', JSON.stringify(legacy.json));
    // A missing required state is reported first. `resources` is a live field of operation:"sections", so beside a
    // valid state it is rejected as a section-only field naming the operation that accepts it (validate/union.rs
    // section_fields_without_operation_name_the_sections_literal), not as an unknown legacy field.
    const named = await cli('clasify', {
      state: { evidence: 'waiting request' },
      resources: [{ id: 'document', tool: 'localFetch', query: { path: docPath } }],
      questions: { answer: { type: 'noul', instructions: 'Is cancellation explained?' } },
    });
    assert.equal(named.json.errorCode, 'invalidInput', JSON.stringify(named.json));
    assert.match(named.json.error, /Remove 'resources' from queries\[0\]: it applies only with operation:"sections"/, JSON.stringify(named.json));
    for (const tool of ['localSearch', 'localFetch']) {
      const plain = ordinary(tool === 'localSearch' ? search : fetch, tool);
      for (const field of [{ goals: classifiers }, { classifiers }, { ranking: 'legacy-plan' }]) {
        const result = await cli(tool, { ...plain, ...field });
        assertRejected(result, `${tool} ${Object.keys(field)[0]}`);
        assert.match(JSON.stringify(result.json), new RegExp(Object.keys(field)[0]), 'the rejected legacy field is named');
      }
    }
    assert.equal(calls, before, 'rejected inputs send no evidence');
  });
  await check('selected fetch pages carry no verification reread hint and no hint inherits the plan or context', async () => {
    const selected = await walk(cli, 'localFetch', { ...fetch, extras: ['hints'] });
    receipt(dataOf(selected.results[0]), 'chunks');
    for (const result of selected.results) {
      const row = dataOf(result);
      assert.equal(findHint(row, 'hints.readBlock'), undefined);
      for (const lead of nextHints(row).filter(call => call.path.includes('.hints.'))) {
        const name = lead.key;
        for (const query of lead.query?.queries ?? []) {
          assert.equal(query.selection, undefined, `hints.${name} inherits no selection plan`);
          assert.equal(query.clasifyContext, undefined, `hints.${name} inherits no classification context`);
        }
      }
    }
  });
  await check('oversized indivisible line is judged whole or left unknown, never clipped', async () => {
    const giantPath = path.join(fixture, 'giant.txt');
    const giant = `${marker} ${'x'.repeat(9000)}`;
    fs.writeFileSync(giantPath, `${giant}\n`);
    const query = { path: giantPath, clasifyContext: { classifiers }, unit: 'lines', length: 1 };
    const baseline = success(await cli('localFetch', ordinary(query, 'localFetch')));
    const before = sentStates.length;
    const selected = success(await cli('localFetch', query));
    assert.equal(receipt(selected, 'chunks').selectedItems, 1);
    assert.equal(sentStates.length - before, 1);
    assert.ok(JSON.stringify(sentStates.at(-1)).includes(giant), 'the provider receives the whole line');
    assert.deepEqual(sourceRows(selected), sourceRows(baseline));
    assert.ok(selected.content.includes('x'.repeat(9000)));
    // A line over the constant 131,072-character budget is never sent clipped.
    const hugePath = path.join(fixture, 'huge.txt');
    const huge = `${marker} ${'x'.repeat(140_000)}`;
    fs.writeFileSync(hugePath, `${huge}\n`);
    const startCalls = calls;
    const unknown = success(await cli('localFetch', { ...query, path: hugePath }));
    const value = receipt(unknown, 'chunks');
    assert.equal(calls, startCalls, 'an over-budget window is not sent clipped');
    assert.equal(value.unknownItems, 1);
    assert.deepEqual(sourceRows(unknown), []);
    const recovered = await assertRecovery('localFetch', unknown, undefined);
    const text = recovered.results.map(result => dataOf(result).content ?? '').join('');
    assert.ok(text.includes('x'.repeat(140_000)), 'recovery returns the whole line');
  });
  await check('source mutation rejects a selected continuation', async () => {
    const row = success(await cli('localFetch', fetch));
    try {
      fs.appendFileSync(docPath, 'A new source line.\n');
      const before = calls;
      const result = await cli('localFetch', continuation(row).query.queries[0]);
      assert.equal(staleCode(result), 'staleSnapshot');
      assert.ok(dataOf(result)?.next?.restart);
      assert.equal(calls, before);
    } finally { fs.writeFileSync(docPath, `${docLines.join('\n')}\n`); }
  });
  await check('same bytes at a different source path reject a selection plan', async () => {
    const copyPath = path.join(fixture, 'source with spaces 🐙.txt');
    fs.copyFileSync(docPath, copyPath);
    const row = success(await cli('localFetch', fetch));
    const continued = continuation(row).query.queries[0];
    const before = calls;
    const result = await cli('localFetch', { ...continued, path: copyPath });
    assert.equal(staleCode(result), 'staleSnapshot');
    assert.equal(calls, before);
  });
  await check('selection reports provider usage through existing opt-in statistics', async () => {
    const before = calls;
    const row = success(await cli('localSearch', search, { OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_ENABLE_STATS: 'true' }));
    assert.ok(receipt(row, 'chunks').selectedItems > 0);
    const usage = JSON.parse(fs.readFileSync(path.join(isolated.OCTOCODE_HOME, 'stats.json'), 'utf8')).stats.clasify;
    assert.equal(usage.calls, calls - before);
    assert.equal(usage.input_tokens, 10 * usage.calls);
    assert.equal(usage.output_tokens, 2 * usage.calls);
    assert.equal(usage.unknown_usage_calls, 0);
  });
  await check('MCP and CLI select the same evidence and MCP replays without classification', async () => {
    mcp = await startServer({ env, cwd: fixture, timeoutMs: 25000 });
    for (const [tool, query] of [['localSearch', search], ['localFetch', fetch]]) {
      const throughCli = await walk(cli, tool, query);
      const throughMcp = await walk(viaMcp, tool, query);
      assert.ok(throughMcp.results.length > 1, 'MCP follows a plan-bound continuation');
      assert.ok(throughMcp.provider[0] > 0);
      assert.ok(throughMcp.provider.slice(1).every(n => n === 0), 'MCP continuations replay without classification');
      assert.deepEqual(throughMcp.rows, throughCli.rows);
      assert.equal(stable(dataOf(throughMcp.results[0]).selection), stable(dataOf(throughCli.results[0]).selection));
    }
  });
} finally {
  mcp?.close();
  mock.closeAllConnections();
  await new Promise(resolve => mock.close(resolve));
  fs.rmSync(fixture, { recursive: true, force: true });
  fs.rmSync(isolated.OCTOCODE_HOME, { recursive: true, force: true });
}
fs.mkdirSync(RESULTS, { recursive: true });
fs.writeFileSync(path.join(RESULTS, 'local-ranking.json'), `${JSON.stringify({ checks, providerCalls: calls, failed: checks.filter(c => !c.passed).length }, null, 2)}\n`);
console.log(`${checks.filter(c => c.passed).length}/${checks.length} local selection checks passed`);
if (checks.some(c => !c.passed)) process.exitCode = 1;
