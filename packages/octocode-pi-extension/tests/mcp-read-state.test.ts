import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import {
  __test__ as mcpTestHooks,
  handleMcpAction,
  stopAllMcpServers,
} from '../src/tools/mcp-tool.js';
import { checkReadState, clearReadStatesForTests } from '../src/tools/file-state.js';
import { projectMcpPath } from '../src/tools/mcp-config.js';
import type { PiContext } from '../src/types.js';

const serverEntry = import.meta.resolve('@modelcontextprotocol/server');
const stdioEntry = import.meta.resolve('@modelcontextprotocol/server/stdio');
const originalHome = process.env.OCTOCODE_HOME;
let fixtureRoot: string | undefined;

afterEach(() => {
  stopAllMcpServers();
  mcpTestHooks.clearCachedMcpCatalog();
  clearReadStatesForTests();
  if (originalHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = originalHome;
  if (fixtureRoot) fs.rmSync(fixtureRoot, { recursive: true, force: true });
  fixtureRoot = undefined;
});

function fixture(payload: Record<string, unknown>) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), '.tmp-mcp-read-state-'));
  fixtureRoot = cwd;
  process.env.OCTOCODE_HOME = path.join(cwd, '.octocode-home');
  const paths = ['first.ts', 'second.ts', 'third.ts'].map(name => path.join(cwd, name));
  for (const file of paths) fs.writeFileSync(file, 'export const value = 1;\n');
  const serverPath = path.join(cwd, 'server.mjs');
  fs.writeFileSync(serverPath, `
    import { Server } from ${JSON.stringify(serverEntry)};
    import { StdioServerTransport } from ${JSON.stringify(stdioEntry)};
    const server = new Server({ name: 'read-state-fixture', version: '1.0.0' }, { capabilities: { tools: {} } });
    server.setRequestHandler('tools/list', async () => ({ tools: [{
      name: 'localGetFileContent', description: 'Read-state regression fixture.',
      inputSchema: { type: 'object', required: ['queries'], properties: {
        queries: { type: 'array', items: { type: 'object', required: ['path'], properties: { path: { type: 'string' } } } }
      } }
    }] }));
    server.setRequestHandler('tools/call', async () => (${JSON.stringify(payload)}));
    await server.connect(new StdioServerTransport());
  `);
  const configPath = projectMcpPath(cwd);
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(configPath, JSON.stringify({ mcpServers: { octocode: {
    command: process.execPath, args: [serverPath], cwd, timeoutMs: 5_000,
  } } }));
  return {
    paths,
    call: () => handleMcpAction({
      action: 'call', server: 'octocode', tool: 'localGetFileContent',
      arguments: { queries: paths.map(file => ({ path: file })) },
    }, undefined, { cwd, isProjectTrusted: () => true } as unknown as PiContext),
  };
}

test('a failed MCP file read does not establish fresh read state', async () => {
  const probe = fixture({ isError: true, content: [{ type: 'text', text: 'Read failed.' }] });
  const result = await probe.call();
  assert.equal(result.isError, true);
  await assert.rejects(() => checkReadState(probe.paths[0]!, true), /No prior localGetFileContent read state/);
});

test('mixed MCP reads establish state only for successful content, using result indices', async () => {
  const probe = fixture({
    content: [{ type: 'text', text: 'One successful read and one error.' }],
    structuredContent: { results: [
      { index: 1, data: { content: 'export const value = 1;\n' } },
      { index: 0, status: 'error', data: { error: 'Read failed.' } },
    ] },
  });
  const result = await probe.call();
  assert.equal(result.isError, false);
  await assert.rejects(() => checkReadState(probe.paths[0]!, true), /No prior localGetFileContent read state/);
  assert.equal((await checkReadState(probe.paths[1]!, true)).state, 'fresh');
  await assert.rejects(() => checkReadState(probe.paths[2]!, true), /No prior localGetFileContent read state/);
});

test('empty or metadata-only MCP query results do not establish read state', async () => {
  const probe = fixture({
    content: [{ type: 'text', text: 'No requested content found.' }],
    structuredContent: { results: [
      { index: 0, status: 'empty', data: { content: 'No matching content found.' } },
      { index: 1, data: { totalLines: 1 } },
    ] },
  });
  await probe.call();
  for (const file of probe.paths) {
    await assert.rejects(() => checkReadState(file, true), /No prior localGetFileContent read state/);
  }
});
