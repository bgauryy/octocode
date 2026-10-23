// Shared state and MCP/CLI call helpers for stdio.acceptance.mjs: one built
// server over real stdio, the receipt every check writes, and the guidance
// assertions every MCP response passes through.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { createLocalAcceptanceFixture } from './local-acceptance-fixture.mjs';
import { suggestedCall } from '../../../../packages/octocode-benchmark/local/e2e/harness/lead-predicates.mjs';

export const suggestion = (data, action) =>
  (data?.hints ?? []).map(suggestedCall).find(lead => lead?.key === action);

export const { values } = parseArgs({
  options: {
    server: { type: 'string', default: 'packages/octocode-mcp/dist/index.js' },
    cli: { type: 'string', default: 'packages/octocode/out/octocode.js' },
    cwd: { type: 'string', default: process.cwd() },
    node: { type: 'string', default: process.execPath },
    fixture: { type: 'string' },
    receipt: {
      type: 'string',
      default: '.octocode/octocode-research/preproduction-mcp-receipt.json',
    },
    quick: { type: 'boolean', default: false },
    live: { type: 'boolean', default: false },
    'cli-mcp-parity': { type: 'boolean', default: false },
  },
});
export const acceptanceCwd = path.resolve(values.cwd);
// Discovery honors .gitignore. The fixture must be a normal workspace subtree,
// not under .octocode/tmp (ignored by this repository).
export const fixture = values.fixture
  ? path.resolve(values.fixture)
  : await createLocalAcceptanceFixture(acceptanceCwd);
export const acceptanceEnv = {
  ...process.env,
  OCTOCODE_ENABLE_LOCAL: 'true',
  OCTOCODE_BETA: 'true',
  OCTOCODE_STORAGE_MODE: 'persistent',
  // An ambient credential must not turn the hermetic run into a provider test.
  ...(!values.live && { OCTOCODE_CLASSIFICATION_API: '' }),
};
const { continuationChannel } = await import('@octocodeai/config/schema');
/**
 * Runtime facts the checks share: `list` is the tools/list result and
 * `expectedTools` its names. CLI-only tools run only from the CLI; MCP never
 * lists or executes them.
 */
/** @type {{ list: any, expectedTools: string[] }} */
export const state = { list: undefined, expectedTools: [] };
/** Default `Array#sort` order (UTF-16 code units) as an explicit comparator. */
export const byCodeUnit = (left, right) =>
  Number(left > right) - Number(left < right);
export const receipt = {
  server: path.resolve(values.server),
  node: values.node,
  fixture,
  checks: [],
  calls: [],
  transportErrors: [],
  stderrBytes: 0,
  stderrTail: '',
  guidance: {
    recoveryHints: 0,
    maxHintChars: 0,
    continuations: 0,
    validationErrors: 0,
  },
};
export const cliCalls = [];
export const transport = new StdioClientTransport({
  command: values.node,
  args: [path.resolve(values.server)],
  cwd: acceptanceCwd,
  env: acceptanceEnv,
  stderr: 'pipe',
});
export const client = new Client({
  name: 'octocode-stdio-acceptance',
  version: '1.0.0',
});
export const check = async (name, fn) => {
  try {
    await fn();
    receipt.checks.push({ name, status: 'passed' });
  } catch (error) {
    receipt.checks.push({ name, status: 'failed', error: error.message });
  }
};

// Required continuations stay structured; optional strings retain complete suggested inputs.
function assertRunnable(name, kind, lead) {
  assert.ok(
    state.expectedTools.includes(lead?.tool),
    `${name}: continuation ${kind} has no runnable tool`
  );
  assert.ok(
    Array.isArray(lead?.query?.queries) && lead.query.queries.length > 0,
    `${name}: continuation ${kind} has no executable query`
  );
  receipt.guidance.continuations += 1;
}

function assertHints(name, hints, recovery) {
  assert.ok(
    Array.isArray(hints) &&
      hints.every(hint => typeof hint === 'string' && hint.trim()),
    `${name}: hints must be nonempty strings`
  );
  assert.equal(
    new Set(hints).size,
    hints.length,
    `${name}: duplicate guidance`
  );
  const leads = hints.map(suggestedCall).filter(Boolean);
  const text = hints.filter(hint => !suggestedCall(hint));
  if (text.length)
    assert.ok(recovery, `${name}: hints.text on a successful result`);
  assert.ok(
    text.every(hint => hint.length <= 160),
    `${name}: long hint`
  );
  receipt.guidance.recoveryHints += text.length;
  receipt.guidance.maxHintChars = Math.max(
    receipt.guidance.maxHintChars,
    ...text.map(hint => hint.length)
  );
  for (const lead of leads) {
    const kind = lead.key;
    assert.equal(
      continuationChannel(name, kind),
      'lead',
      `${name}: hints.${kind} is a page`
    );
    assertRunnable(name, kind, lead);
    if (!recovery)
      assert.equal(lead.why, undefined, `${name}: success continuation prose`);
  }
}

function assertNext(name, next, recovery) {
  for (const [kind, lead] of Object.entries(next)) {
    assert.equal(
      continuationChannel(name, kind),
      'page',
      `${name}: next.${kind} is a lead`
    );
    assertRunnable(name, kind, lead);
    if (!recovery)
      assert.equal(lead.why, undefined, `${name}: success continuation prose`);
  }
}

const EVIDENCE_KEYS = [
  'query',
  'content',
  'body',
  'patch',
  'text',
  'value',
  'matches',
];

/** Walks a result row; evidence fields are data, not guidance. */
function inspectGuidance(name, value, recovery) {
  if (!value || typeof value !== 'object') return;
  for (const [key, child] of Object.entries(value)) {
    if (EVIDENCE_KEYS.includes(key)) continue;
    if (key === 'hints') assertHints(name, child, recovery);
    if (key === 'next') assertNext(name, child, recovery);
    if (key !== 'hints' && key !== 'next')
      inspectGuidance(name, child, recovery);
  }
}

// Briefs (`mainGoal`, `reasoning`) are optional, so fixtures send only what
// they exercise: a call without a brief is the common, single-lookup shape.
export const invoke = async (name, args) => {
  const startedAt = performance.now();
  const response = await client.callTool({ name, arguments: args });
  const durationMs = Number((performance.now() - startedAt).toFixed(2));
  const responseBytes = Buffer.byteLength(JSON.stringify(response));
  receipt.calls.push({
    name,
    arguments: args,
    durationMs,
    responseBytes,
    response,
  });
  // MCP text is JSON only: one block, the serialized structuredContent.
  assert.deepEqual(
    response.content,
    [{ type: 'text', text: JSON.stringify(response.structuredContent) }],
    `${name}: the text block is not the structuredContent JSON`
  );
  for (const row of response.structuredContent?.results ?? [])
    inspectGuidance(
      name,
      row,
      row.status === 'empty' || row.status === 'error'
    );
  return response;
};
export const call = async (name, query) => {
  const response = await invoke(name, { queries: [query] });
  assert.equal(response.isError, false, `${name} returned a tool error`);
  assert.ok(response.structuredContent, `${name} has no structured content`);
  assert.ok(
    response.content.some(block => block.type === 'text' && block.text.length),
    `${name} has no text representation`
  );
  const row = response.structuredContent.results?.[0];
  assert.ok(row?.data, `${name} has no result data`);
  assert.notEqual(row.status, 'error', `${name} result failed`);
  assert.equal(row.data.error, undefined, `${name} returned an error payload`);
  return row.data;
};

const joinPath = (...parts) => parts.filter(Boolean).join('/');
const entryName = entry =>
  (/^(.*) \([^()]*\)$/.exec(entry)?.[1] ?? entry).replace(/\/$/, '');
/**
 * structureSearch `files` rows as paths: bare entries are the row `path`'s
 * own, group `dir`s are relative to it (root/dir/name).
 */
export const listedPaths = (files, root) => {
  const at = (dir, entry) =>
    entryName(entry) === '.'
      ? joinPath(root, dir)
      : joinPath(root, dir, entryName(entry));
  return files.flatMap(group =>
    typeof group === 'string'
      ? [at('', group)]
      : group.files.map(entry => at(group.dir, entry))
  );
};

export const continuationRow = async continuation => {
  assert.ok(
    state.expectedTools.includes(continuation?.tool),
    'continuation has no runnable tool'
  );
  assert.ok(
    continuation.query && typeof continuation.query === 'object',
    'continuation has no query'
  );
  // A continuation is the complete input: run it verbatim.
  assert.ok(
    Array.isArray(continuation.query.queries),
    'continuation is not a complete input'
  );
  const response = await invoke(continuation.tool, continuation.query);
  const row = response.structuredContent?.results?.[0];
  assert.ok(row?.data, `${continuation.tool} has no result data`);
  if (response.isError) assert.equal(row.status, 'error');
  return row;
};
export const nextCall = async continuation => {
  const row = await continuationRow(continuation);
  assert.notEqual(row.status, 'error', `${continuation.tool} result failed`);
  assert.equal(
    row.data.error,
    undefined,
    `${continuation.tool} returned an error payload`
  );
  return row.data;
};
export const executeCliTool = (name, queries, expectedStatus = 0) => {
  const startedAt = performance.now();
  const child = spawnSync(
    values.node,
    [path.resolve(values.cli), name, JSON.stringify({ queries }), '--json'],
    {
      encoding: 'utf8',
      timeout: 120_000,
      maxBuffer: 8 * 1024 * 1024,
      cwd: acceptanceCwd,
      env: acceptanceEnv,
    }
  );
  const durationMs = Number((performance.now() - startedAt).toFixed(2));
  cliCalls.push({ name, durationMs, status: child.status });
  if (child.error) throw child.error;
  assert.strictEqual(
    child.status,
    expectedStatus,
    `${name}: CLI exited ${child.status}: ${child.stderr.trim()}`
  );
  assert.ok(child.stdout.trim(), `${name}: CLI returned no JSON`);
  return JSON.parse(child.stdout);
};
export const pages = async (first, nextKey, collect) => {
  let rows = [...collect(first)];
  let current = first;
  let count = 1;
  let restarts = 0;
  for (;;) {
    const continuation = current.next?.[nextKey];
    if (!continuation) break;
    assert.ok(count++ < 100, 'continuation did not terminate');
    const row = await continuationRow(continuation);
    if (row.status === 'error') {
      assert.equal(row.data.errorCode, 'staleSnapshot');
      assert.ok(
        row.data.next?.restart,
        'changed snapshot has no restart continuation'
      );
      assert.ok(restarts++ < 5, 'snapshot did not stabilize');
      const restarted = await continuationRow(row.data.next.restart);
      assert.notEqual(restarted.status, 'error', 'snapshot restart failed');
      current = restarted.data;
      rows = [...collect(current)];
      count = 1;
      continue;
    }
    current = row.data;
    rows.push(...collect(current));
  }
  return { rows, count };
};
