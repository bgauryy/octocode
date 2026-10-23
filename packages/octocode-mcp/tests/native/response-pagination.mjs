import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdtemp, rm, utimes, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import { getDirectToolDefinitionsWithAddons } from '@octocodeai/config/schema';
import {
  checkResponsePages,
  checkToolPages,
} from './response-pagination-checks.mjs';

// A candidate addon to check; without one, the packaged runtime the servers load.
const [
  referenceServer,
  nativeServer,
  addon = createRequire(import.meta.url).resolve(
    '@octocodeai/octocode-native/runtime'
  ),
] = process.argv.slice(2);
assert.ok(
  referenceServer && nativeServer,
  'usage: node response-pagination.mjs <reference> <native> [<addon>]'
);

const fixture = await mkdtemp(path.join(tmpdir(), 'octocode-native-mcp-'));
const source = path.join(fixture, 'unicode.txt');
const sourceText = `${'alpha 😀 beta\n'.repeat(40)}omega\n`;
await writeFile(source, sourceText);
const frozenTime = new Date('2020-09-13T12:26:40.443Z');
await utimes(source, frozenTime, frozenTime);
const env = {
  ...process.env,
  OCTOCODE_ENABLE_LOCAL: 'true',
  WORKSPACE_ROOT: fixture,
  ALLOWED_PATHS: fixture,
  NODE_ENV: 'development',
  OCTOCODE_NATIVE_BINDING: addon,
};
const require = createRequire(import.meta.url);
const { NativeRuntime } = require(path.resolve(addon));
const {
  resolveNativeBinaryPath,
} = require('@octocodeai/octocode-native/bin/resolve-binary.cjs');
const nativeBinary = resolveNativeBinaryPath();
const npmCli = path.resolve(
  import.meta.dirname,
  '../../../octocode/out/octocode.js'
);
const exec = promisify(execFile);

async function cliCall(command, prefix, args) {
  try {
    const { stdout } = await exec(
      command,
      [...prefix, 'localFetch', JSON.stringify(args), '--json'],
      {
        cwd: fixture,
        env,
        maxBuffer: 8 * 1024 * 1024,
        timeout: 30_000,
      }
    );
    return { structuredContent: JSON.parse(stdout) };
  } catch (error) {
    if (error.stdout)
      return { isError: true, structuredContent: JSON.parse(error.stdout) };
    throw error;
  }
}

function surfaces(client, runtime) {
  return [
    ['native CLI', args => cliCall(nativeBinary, [], args)],
    ['npm CLI', args => cliCall(process.execPath, [npmCli], args)],
    [
      'direct N-API',
      args => runtime.executeMcp(`page-${randomUUID()}`, 'localFetch', args),
    ],
    ['MCP', args => client.callTool({ name: 'localFetch', arguments: args })],
  ];
}

async function connect(server) {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.resolve(server)],
    cwd: fixture,
    env,
    stderr: 'pipe',
  });
  transport.stderr?.on('data', chunk => process.stderr.write(chunk));
  const client = new Client({
    name: 'native-response-pagination',
    version: '1',
  });
  await client.connect(transport);
  return client;
}

// A page as two servers can agree on it: its structuredContent (each page's
// text is asserted to be exactly that JSON) without the per-server snapshot
// expiry clock. The content-addressed responseSnapshot cursors must match.
function comparablePage({ isError, structuredContent }) {
  return { isError, structuredContent: withoutExpiry(structuredContent) };
}

function withoutExpiry(value) {
  if (Array.isArray(value)) return value.map(withoutExpiry);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== 'expiresAt')
      .map(([key, nested]) => [key, withoutExpiry(nested)])
  );
}

async function collect(client) {
  let args = {
    queries: [
      {
        path: source,
        fullContent: true,
        mainGoal: 'page',
        reasoning: 'Exercise response pagination parity.',
      },
    ],
    // Whole rows are the default page unit; character windows of the
    // serialized structuredContent (the MCP text is exactly that JSON) split
    // the multi-byte characters this harness exists to check.
    responseLength: 96,
    responseScope: 'structured',
  };
  const pages = [];
  for (let index = 0; index < 100; index += 1) {
    const result = await client.callTool({
      name: 'localFetch',
      arguments: args,
    });
    assert.notEqual(
      result.isError,
      true,
      JSON.stringify(result.structuredContent)
    );
    // The MCP text block is exactly the serialized structuredContent.
    assert.deepEqual(result.content, [
      { type: 'text', text: JSON.stringify(result.structuredContent) },
    ]);
    pages.push(result);
    const pagination = result.structuredContent?.responsePagination;
    if (!pagination?.hasMore) break;
    const continuation = pagination.next;
    assert.equal(continuation?.tool, 'localFetch');
    // The continuation is one opaque cursor: copying that field alone pages on.
    assert.deepEqual(Object.keys(continuation.query ?? {}), [
      'responseSnapshot',
    ]);
    args = { responseSnapshot: continuation.query.responseSnapshot };
  }
  assert.ok(pages.length > 1);
  // Saved reads retain replay/lifetime metadata after the final page.
  const last = pages.at(-1).structuredContent.responsePagination;
  assert.equal(last?.hasMore ?? false, false);
  assert.equal(last?.next, undefined);
  // Concatenating the windows in order reconstructs the exact envelope JSON,
  // multi-byte characters intact: the row holds the source bytes its own
  // pagination names.
  const envelope = JSON.parse(
    pages.map(page => page.structuredContent.responseWindow).join('')
  );
  const row = envelope.results?.[0]?.data;
  const { offset, length } = row?.pagination ?? {};
  assert.equal(
    row?.content,
    Buffer.from(sourceText)
      .subarray(offset, offset + length)
      .toString()
  );
  assert.match(row.content, /😀/u);
  return pages;
}

function nativeInstructions() {
  const runtime = new NativeRuntime({
    surface: 'mcp',
    env: Object.fromEntries(
      Object.entries(env).filter(([, value]) => typeof value === 'string')
    ),
  });
  try {
    const catalog = runtime.catalog();
    return buildMcpInstructions({
      availableTools: catalog.tools
        .filter(tool => tool.available)
        .map(tool => tool.name),
      selection: catalog.presentation?.selection === true,
    });
  } finally {
    void runtime.close();
  }
}

const reference = await connect(referenceServer);
const native = await connect(nativeServer);
const directRuntime = new NativeRuntime({
  surface: 'mcp',
  env: Object.fromEntries(
    Object.entries(env).filter(([, value]) => typeof value === 'string')
  ),
});
try {
  const nativeTools = await native.listTools();
  // The server composes instructions from the native catalog (available tools
  // and the selection switch); rebuild them from the same addon.
  assert.equal(native.getInstructions(), nativeInstructions());
  const localFetch = nativeTools.tools.find(tool => tool.name === 'localFetch');
  assert.ok(localFetch, 'native catalog must advertise localFetch');
  assert.ok(localFetch.title);
  assert.equal(Object.hasOwn(localFetch, 'outputSchema'), false);
  // tools/list carries core's annotations and omits output schemas (ARCHITECTURE.md).
  assert.deepEqual(
    localFetch.annotations,
    getDirectToolDefinitionsWithAddons({ availableTools: ['localFetch'] }).find(
      definition => definition.name === 'localFetch'
    ).annotations
  );
  const [expected, actual] = await Promise.all([
    collect(reference),
    collect(native),
  ]);
  assert.deepEqual(actual.map(comparablePage), expected.map(comparablePage));

  const stale = await native.callTool({
    name: 'localFetch',
    arguments: { responseSnapshot: 'response-v1:stale' },
  });
  assert.equal(stale.isError, true);
  assert.equal(stale.structuredContent.errorCode, 'responseSnapshotExpired');
  assert.equal(stale.structuredContent.responsePagination, undefined);
  const [expectedInvalid, actualInvalid] = await Promise.all([
    reference.callTool({ name: 'localFetch', arguments: { queries: [{}] } }),
    native.callTool({ name: 'localFetch', arguments: { queries: [{}] } }),
  ]);
  assert.deepEqual(actualInvalid, expectedInvalid);
  const calls = surfaces(native, directRuntime);
  const toolPages = await checkToolPages(calls, source, sourceText);
  const responsePages = await checkResponsePages(calls, source);
  console.log(
    JSON.stringify({
      pages: actual.length,
      toolPages,
      responsePages,
      surfaces: calls.map(([name]) => name),
      unicode: true,
      staleReadRejected: true,
    })
  );
} finally {
  await Promise.allSettled([
    reference.close(),
    native.close(),
    directRuntime.close(),
  ]);
  await rm(fixture, { recursive: true, force: true });
}
