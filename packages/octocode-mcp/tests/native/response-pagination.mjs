import assert from 'node:assert/strict';
import { mkdtemp, rm, utimes, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { buildMcpInstructions } from '@octocodeai/octocode-core/mcp';

const [referenceServer, nativeServer, addon, regexWorker] = process.argv.slice(2);
assert.ok(referenceServer && nativeServer && addon && regexWorker,
  'usage: node response-pagination.mjs <reference> <native> <addon> <regex-worker>');

const fixture = await mkdtemp(path.join(tmpdir(), 'octocode-native-mcp-'));
const source = path.join(fixture, 'unicode.txt');
await writeFile(source, `${'alpha 😀 beta\n'.repeat(40)}omega\n`);
const frozenTime = new Date('2020-09-13T12:26:40.443Z');
await utimes(source, frozenTime, frozenTime);
const env = {
  ...process.env,
  ENABLE_LOCAL: 'true',
  WORKSPACE_ROOT: fixture,
  ALLOWED_PATHS: fixture,
  OCTOCODE_NATIVE_BINDING: addon,
  OCTOCODE_REGEX_WORKER: regexWorker,
};

async function connect(server) {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.resolve(server)],
    cwd: fixture,
    env,
    stderr: 'pipe',
  });
  const client = new Client({ name: 'native-response-pagination', version: '1' });
  await client.connect(transport);
  return client;
}

function withoutOpaqueCursors(value) {
  if (Array.isArray(value)) return value.map(withoutOpaqueCursors);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== 'cursor')
      .map(([key, nested]) => [key, withoutOpaqueCursors(nested)]),
  );
}

async function collect(client) {
  let args = {
    queries: [{ path: source, fullContent: true, goal: 'page', reasoning: 'Exercise response pagination parity.', debug: false }],
    responseCharLength: 96,
  };
  const pages = [];
  for (let index = 0; index < 100; index += 1) {
    const result = await client.callTool({ name: 'localFetch', arguments: args });
    pages.push(result);
    const pagination = result.structuredContent?.responsePagination;
    if (!pagination?.hasMore) break;
    const continuation = pagination.next;
    assert.equal(continuation?.tool, 'localFetch');
    assert.ok(continuation.query?.responseSnapshot);
    args = continuation.query;
  }
  assert.ok(pages.length > 1);
  assert.equal(pages.at(-1).structuredContent.responsePagination.hasMore, false);
  return pages;
}

const reference = await connect(referenceServer);
const native = await connect(nativeServer);
try {
  const nativeTools = await native.listTools();
  const nativeToolNames = nativeTools.tools.map(tool => tool.name);
  assert.equal(native.getInstructions(), buildMcpInstructions(nativeToolNames));
  const localFetch = nativeTools.tools.find(tool => tool.name === 'localFetch');
  assert.ok(localFetch, 'native catalog must advertise localFetch');
  assert.ok(localFetch.title);
  assert.ok(localFetch.outputSchema);
  assert.deepEqual(localFetch.annotations, {
    title: localFetch.title,
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  });
  const [expected, actual] = await Promise.all([collect(reference), collect(native)]);
  assert.deepEqual(withoutOpaqueCursors(actual), withoutOpaqueCursors(expected));

  const first = actual[0].structuredContent.responsePagination;
  const stale = await native.callTool({
    name: 'localFetch',
    arguments: {
      queries: [{ path: source, fullContent: true, reasoning: 'Exercise stale response pagination restart parity.', debug: false }],
      responseCharLength: 96,
      responseCharOffset: first.nextCharOffset,
      responseSnapshot: 'response-v1:stale',
    },
  });
  assert.equal(stale.structuredContent.responsePagination.restart, true);
  assert.equal(stale.structuredContent.responsePagination.next.query.responseCharOffset, 0);
  const [expectedInvalid, actualInvalid] = await Promise.all([
    reference.callTool({ name: 'localFetch', arguments: { queries: [{}] } }),
    native.callTool({ name: 'localFetch', arguments: { queries: [{}] } }),
  ]);
  assert.deepEqual(actualInvalid, expectedInvalid);
  console.log(JSON.stringify({ pages: actual.length, unicode: true, staleRestart: true }));
} finally {
  await Promise.allSettled([reference.close(), native.close()]);
  await rm(fixture, { recursive: true, force: true });
}
