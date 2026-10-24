/** Built CLI and fresh MCP context selection against a hermetic loopback scorer. */
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { hermeticEnv } from '../../../../test-utils/hermetic-env.ts';
import { startSelectionScorer } from './selection-scorer.mjs';

const root = process.cwd();
const cliPath = path.join(root, 'packages/octocode/out/octocode.js');
const mcpPath = path.join(root, 'packages/octocode-mcp/dist/index.js');
const fixture = await fs.mkdtemp(
  path.join(os.tmpdir(), 'octocode-selection-execution-')
);
const selectedMarker = 'SELECTED_SELECTION_EVIDENCE';
const unknownMarker = 'UNKNOWN_SELECTION_EVIDENCE';
const excludedMarker = 'EXCLUDED_SELECTION_EVIDENCE';
const predicates = [
  'Does this directly state that a queued request is cancelled before work starts?',
];
const searchPath = path.join(fixture, 'search');
const fetchPath = path.join(fixture, 'document.txt');
await fs.mkdir(searchPath);
await fs.writeFile(
  path.join(searchPath, '0-excluded.txt'),
  `topic: ${excludedMarker} bookkeeping only\n`
);
await fs.writeFile(
  path.join(searchPath, '1-unknown.txt'),
  `topic: ${unknownMarker} relationship unclear\n`
);
await fs.writeFile(
  path.join(searchPath, '2-selected.txt'),
  `topic: ${selectedMarker} A queued request is cancelled before work starts.\n`
);
const lines = Array.from(
  { length: 1200 },
  (_, index) => `line ${index + 1}: ordinary context`
);
for (let index = 0; index < 400; index++) {
  lines[index] =
    `${selectedMarker} Request ${index + 1} is cancelled before work starts.`;
}
lines[599] = `${unknownMarker} The relationship to cancellation is unclear.`;
lines[999] = `${excludedMarker} Routine bookkeeping does not cancel work.`;
await fs.writeFile(fetchPath, `${lines.join('\n')}\n`);

let scorer;
try {
  scorer = await startSelectionScorer({
    selected: selectedMarker,
    unknown: unknownMarker,
  });
} catch (error) {
  await fs.rm(fixture, { recursive: true, force: true });
  throw error;
}
const { requests } = scorer;

const isolated = hermeticEnv(process.env, 'selection-execution');
const baseEnv = {
  ...isolated,
  HOME: isolated.OCTOCODE_HOME,
  OCTOCODE_CLASSIFICATION_API: 'fixture-key',
  OCTOCODE_CLASSIFICATION_API_HOST: `http://127.0.0.1:${scorer.port}`,
  OCTOCODE_STORAGE_MODE: 'persistent',
  OCTOCODE_LSP_PREWARM: 'off',
  DISABLE_TOOLS: '',
  TOOLS_TO_RUN: '',
  WORKSPACE_ROOT: fixture,
  ALLOWED_PATHS: fixture,
};

async function cli(tool, input, { env = baseEnv, status = 0 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [cliPath, tool, JSON.stringify(input), '--json'],
      {
        cwd: fixture,
        env,
      }
    );
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', chunk => {
      stdout += chunk;
    });
    child.stderr.on('data', chunk => {
      stderr += chunk;
    });
    const timer = setTimeout(() => child.kill('SIGKILL'), 25000);
    child.on('error', reject);
    child.on('close', code => {
      clearTimeout(timer);
      try {
        assert.equal(code, status, `${tool}: ${stderr.slice(0, 400)}`);
        resolve(JSON.parse(stdout));
      } catch (error) {
        reject(new Error(`${error.message}; stdout: ${stdout.slice(0, 400)}`));
      }
    });
  });
}

async function mcp(env = baseEnv) {
  const client = new Client({ name: 'selection-execution', version: '1' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [mcpPath],
    cwd: fixture,
    env,
    stderr: 'pipe',
  });
  try {
    await client.connect(transport);
    return client;
  } catch (error) {
    await client.close().catch(() => {});
    await transport.close().catch(() => {});
    throw error;
  }
}

// A refused provider credential fails the row: the CLI exits 5 (execution
// error, docs/OCTOCODE_CLI.md "Exit codes") and MCP marks the call isError.
const cliEnv = { ...baseEnv, OCTOCODE_CLASSIFICATION_API: 'fixture-cli-key' };
const callCli = (tool, input) => cli(tool, input, { env: cliEnv });
const refusedCli = (tool, input) =>
  cli(tool, input, { env: cliEnv, status: 5 });
const mcpCall = isError => client => async (tool, input) => {
  const response = await client.callTool({
    name: tool,
    arguments: input,
  });
  assert.equal(response.isError, isError, JSON.stringify(response));
  return response.structuredContent;
};
const callMcp = mcpCall(false);
const refusedMcp = mcpCall(true);
function data(result) {
  assert.ok(result?.results?.[0], JSON.stringify(result));
  assert.notEqual(result.results[0].status, 'error', JSON.stringify(result));
  return result.results[0].data;
}
function hitText(match) {
  const value = String(match.value ?? '');
  const numbered = value
    .split('\n')
    .find(line => line.startsWith(`${match.line}\t`));
  return numbered === undefined
    ? value
    : numbered.slice(numbered.indexOf('\t') + 1);
}
function evidence(tool, row, { hits = false } = {}) {
  return tool === 'localFetch'
    ? String(row.content ?? '')
        .split('\n')
        .filter(line => /^\d+\t/.test(line))
    : (row.files ?? []).flatMap(file =>
        (file.matches ?? []).map(
          match =>
            `${file.path}:${match.line}:${hits ? hitText(match) : (match.value ?? '')}`
        )
      );
}
function receipt(tool, row) {
  const value = row.selection;
  assert.ok(value, `${tool}: selection receipt`);
  assert.ok(['complete', 'partial', 'unavailable'].includes(value.status));
  assert.equal(value.scope, 'selectedView');
  assert.equal(value.unit, 'chunks');
  assert.equal(
    value.selectedItems +
      value.excludedItems +
      (value.relatedItems ?? 0) +
      value.unknownItems,
    value.totalItems
  );
  return value;
}

const initial = query => ({ queries: [query], responseChars: 4000 });
async function walk(call, tool, input, { selected = false } = {}) {
  const pending = [input];
  const seen = new Set();
  const rows = [];
  const hits = [];
  const responses = [];
  let providerCallsAfterFirst;
  while (pending.length) {
    assert.ok(seen.size < 40, `${tool}: bounded continuation`);
    const current = pending.shift();
    const key = JSON.stringify(current);
    assert.ok(!seen.has(key), `${tool}: continuation repeats`);
    seen.add(key);
    const result = await call(tool, current);
    const row = data(result);
    responses.push(row);
    rows.push(...evidence(tool, row));
    hits.push(...evidence(tool, row, { hits: true }));
    if (selected) {
      receipt(tool, row);
      providerCallsAfterFirst ??= requests.length;
      assert.equal(
        requests.length,
        providerCallsAfterFirst,
        `${tool}: selected paging never re-scores`
      );
    }
    const continuations = result.responsePagination?.next
      ? [result.responsePagination.next]
      : Object.entries(row.next ?? {})
          .filter(
            ([name]) => !['unfiltered', 'unfilteredUnknown'].includes(name)
          )
          .map(([, lead]) => lead);
    for (const lead of continuations) {
      assert.equal(lead.kind, 'continue');
      assert.equal(lead.tool, tool);
      assert.deepEqual(Object.keys(lead.input), ['cursor']);
      pending.push(lead.input);
    }
  }
  return { first: responses[0], responses, rows, hits, pages: seen.size };
}

async function recover(call, tool, selectedRun) {
  const lead = selectedRun.first.next?.unfiltered;
  assert.ok(lead, `${tool}: next.unfiltered`);
  assert.equal(lead.tool, tool);
  assert.equal(lead.kind, 'continue');
  assert.deepEqual(Object.keys(lead.input), ['cursor']);
  const before = requests.length;
  const recovered = await walk(call, tool, lead.input);
  assert.equal(
    requests.length,
    before,
    `${tool}: recovery performs no JEV work`
  );
  return recovered;
}

const context = suffix => ({
  classification: {
    predicates: [`${predicates[0]}${suffix ? ` ${suffix}` : ''}`],
    context:
      'Find direct cancellation evidence; preserve unknown evidence for recovery.',
  },
});
const cases = [
  ['localSearch', { path: searchPath, matchString: ['topic'] }, 3, 1],
  ['localFetch', { path: fetchPath }, lines.length, 2],
];
const reports = [];
try {
  const client = await mcp();
  try {
    for (const [surface, call] of [
      ['CLI', callCli],
      ['MCP', callMcp(client)],
    ]) {
      for (const [tool, query, items, minPages] of cases) {
        const beforeOrdinary = requests.length;
        const ordinary = await walk(call, tool, initial(query));
        assert.equal(
          ordinary.rows.length,
          items,
          `${surface} ${tool}: complete ordinary view`
        );
        assert.equal(
          requests.length,
          beforeOrdinary,
          `${surface} ${tool}: no context means no JEV work`
        );
        assert.equal(ordinary.first.selection, undefined);

        const beforeSelected = requests.length;
        const selectedRun = await walk(
          call,
          tool,
          initial({ ...query, ...context('') }),
          { selected: true }
        );
        const selectedBody = selectedRun.rows.join('\n');
        assert.ok(
          selectedBody.includes(selectedMarker),
          `${surface} ${tool}: selected evidence returned`
        );
        assert.ok(
          !selectedBody.includes(unknownMarker),
          `${surface} ${tool}: unknown body hidden`
        );
        assert.ok(
          !selectedBody.includes(excludedMarker),
          `${surface} ${tool}: excluded body hidden`
        );
        assert.ok(selectedRun.pages >= minPages, 'selected cursor coverage');
        const firstReceipt = receipt(tool, selectedRun.first);
        assert.ok(firstReceipt.selectedItems > 0);
        assert.ok(firstReceipt.excludedItems > 0);
        assert.ok(firstReceipt.unknownItems > 0);
        assert.ok(
          requests.length > beforeSelected,
          `${surface} ${tool}: scorer called`
        );

        const beforeUnknown = requests.length;
        const unknown = await walk(
          call,
          tool,
          selectedRun.first.next.unfilteredUnknown.input
        );
        assert.equal(
          requests.length,
          beforeUnknown,
          'unknown evidence recovery performs no inference'
        );
        const unknownBody = unknown.rows.join('\n');
        assert.ok(unknownBody.includes(unknownMarker));
        assert.ok(
          !unknownBody.includes(selectedMarker) &&
            !unknownBody.includes(excludedMarker)
        );
        const recovered = await recover(call, tool, selectedRun);
        assert.deepEqual(
          [...recovered.hits].sort(),
          [...ordinary.hits].sort(),
          `${surface} ${tool}: lossless unfiltered recovery`
        );
        reports.push({
          surface,
          tool,
          selectedPages: selectedRun.pages,
          recoveryPages: recovered.pages,
        });
      }
    }

    // Provider refusal must fail closed and preserve executable recovery.
    scorer.failure = true;
    for (const [surface, call, refused] of [
      ['CLI', callCli, refusedCli],
      ['MCP', callMcp(client), refusedMcp(client)],
    ]) {
      for (const [tool, query] of cases) {
        const before = requests.length;
        const result = await refused(
          tool,
          initial({
            ...query,
            ...context('UNJUDGED_FAILURE_PROBE'),
          })
        );
        assert.equal(result.results?.length, 1, JSON.stringify(result));
        const [failedRow] = result.results;
        assert.equal(failedRow.status, 'error');
        assert.equal(failedRow.data.errorCode, 'classificationProviderError');
        const failed = { first: failedRow.data };
        const failedReceipt = receipt(tool, failed.first);
        assert.ok(['partial', 'unavailable'].includes(failedReceipt.status));
        assert.ok(failedReceipt.unknownItems > 0);
        assert.deepEqual(
          evidence(tool, failed.first),
          [],
          `${surface} ${tool}: provider failure cannot fail open`
        );
        assert.ok(
          requests.length > before,
          `${surface} ${tool}: failed attempt is accounted`
        );
        scorer.failure = false;
        const ordinary = await walk(call, tool, initial(query));
        const recovered = await recover(call, tool, failed);
        assert.deepEqual(
          [...recovered.hits].sort(),
          [...ordinary.hits].sort(),
          `${surface} ${tool}: failure recovery is complete`
        );
        scorer.failure = true;
      }
    }
    scorer.failure = false;
  } finally {
    await client.close();
  }
} finally {
  await scorer.close();
  await fs.rm(fixture, { recursive: true, force: true });
  await fs.rm(isolated.OCTOCODE_HOME, { recursive: true, force: true });
}

console.log(`PASS selection execution: ${JSON.stringify(reports)}`);
