import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, rm, utimes, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import { getDirectToolDefinitionsWithAddons } from '@octocodeai/config/schema';
import {
  checkResponsePages,
  checkToolPages,
} from './response-pagination-checks.mjs';

// The MCP server under test: the launcher by default (it starts the native
// binary's `octocode mcp`).
const [server = path.resolve(import.meta.dirname, '../../dist/index.js')] =
  process.argv.slice(2);

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
  // An ambient provider key would switch on context selection, which the
  // instructions check below does not model.
  OCTOCODE_CLASSIFICATION_API: '',
};
const require = createRequire(import.meta.url);
const {
  resolveNativeBinaryPath,
} = require('../../../octocode-native/bin/resolve-binary.cjs');
const nativeBinary =
  process.env.OCTOCODE_NATIVE_BIN ?? resolveNativeBinaryPath();
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

function surfaces(client) {
  return [
    ['native CLI', args => cliCall(nativeBinary, [], args)],
    ['npm CLI', args => cliCall(process.execPath, [npmCli], args)],
    ['MCP', args => client.callTool({ name: 'localFetch', arguments: args })],
  ];
}

async function connect() {
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
  // multi-byte characters intact: a read row is the numbered source lines its
  // own pagination names.
  const envelope = JSON.parse(
    pages.map(page => page.structuredContent.responseWindow).join('')
  );
  const row = envelope.results?.[0]?.data;
  const { offset, length, unit } = row?.pagination ?? {};
  assert.equal(unit, 'lines');
  assert.equal(
    row?.content,
    sourceText
      .split('\n')
      .slice(0, -1)
      .map((line, index) => `${index + 1}\t${line}\n`)
      .slice(offset, offset + length)
      .join('')
  );
  assert.match(row.content, /😀/u);
  return pages;
}

const native = await connect();
try {
  const nativeTools = await native.listTools();
  const availableTools = nativeTools.tools.map(tool => tool.name);
  // The binary composes instructions from its embedded copy of core's
  // presentation; core's own renderer must give the same text.
  assert.equal(
    native.getInstructions(),
    buildMcpInstructions({ availableTools, selection: false })
  );
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
  const actual = await collect(native);

  const stale = await native.callTool({
    name: 'localFetch',
    arguments: { responseSnapshot: 'response-v1:stale' },
  });
  assert.equal(stale.isError, true);
  assert.equal(stale.structuredContent.errorCode, 'responseSnapshotExpired');
  assert.equal(stale.structuredContent.responsePagination, undefined);
  const invalid = await native.callTool({
    name: 'localFetch',
    arguments: { queries: [{}] },
  });
  assert.equal(invalid.isError, true);
  assert.equal(invalid.structuredContent.errorCode, 'invalidInput');
  const calls = surfaces(native);
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
  await Promise.allSettled([native.close()]);
  await rm(fixture, { recursive: true, force: true });
}
