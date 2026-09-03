import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { afterEach, test } from 'vitest';
import { Type } from 'typebox';
import {
  __test__ as mcpTestHooks,
  registerMcpTool,
  stopAllMcpServers,
} from '../src/tools/mcp-tool.js';
import { projectMcpPath } from '../src/tools/mcp-config.js';
import type { PiContext, PiInstance, ToolCallResult, ToolDefinition } from '../src/types.js';

const MCP_SERVER_ENTRY = import.meta.resolve('@modelcontextprotocol/server');
const MCP_STDIO_ENTRY = import.meta.resolve('@modelcontextprotocol/server/stdio');

function buildMcpTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  registerMcpTool(
    { registerTool: (definition: ToolDefinition) => tools.set(definition.name, definition) } as unknown as PiInstance,
    Type,
    new Set<string>(),
    (_pi, _names, definition) => tools.set(definition.name, definition),
  );
  const definition = tools.get('MCPTool');
  assert.ok(definition, 'MCPTool must be registered');
  return definition;
}

function invokeMcpTool(
  definition: ToolDefinition,
  params: Record<string, unknown>,
  context: PiContext,
): Promise<ToolCallResult> {
  return definition.execute(
    'external-node-mcp-test',
    { queries: [{ reasoning: 'Exercise the external MCP integration fixture.', ...params }] },
    undefined,
    undefined,
    context,
  );
}

function resultText(result: ToolCallResult): string {
  const first = result.content[0] as { text?: string } | undefined;
  return first?.text ?? '';
}

afterEach(() => {
  stopAllMcpServers();
  mcpTestHooks.clearCachedMcpCatalog();
});

test('real external Node stdio MCP loads and calls through the canonical project config', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), '.tmp-external-node-mcp-'));
  const previousOctocodeHome = process.env.OCTOCODE_HOME;
  process.env.OCTOCODE_HOME = path.join(root, 'octocode-home');
  const serverPath = path.join(root, 'server.mjs');
  fs.writeFileSync(serverPath, `
    import { Server } from ${JSON.stringify(MCP_SERVER_ENTRY)};
    import { StdioServerTransport } from ${JSON.stringify(MCP_STDIO_ENTRY)};
    const server = new Server(
      { name: 'external-node-fixture', version: '1.0.0' },
      { capabilities: { tools: {} }, instructions: 'External Node stdio MCP integration fixture.' },
    );
    server.setRequestHandler('tools/list', async () => ({ tools: [{ name:'probe', description:'Prove the external Node MCP process received its config.', inputSchema:{ type:'object', required:['message'], properties:{ message:{ type:'string' } } } }] }));
    server.setRequestHandler('tools/call', async (request) => ({
      content: [{ type: 'text', text: 'external:' + process.env.MCP_ALIAS + ':' + process.cwd() + ':' + request.params.arguments?.message }],
    }));
    await server.connect(new StdioServerTransport());
  `);

  const mcpTool = buildMcpTool();
  try {
    for (const label of ['canonical']) {
      const workspace = path.join(root, `workspace-${label}`);
      fs.mkdirSync(workspace, { recursive: true });
      const configPath = projectMcpPath(workspace);
      const serverName = 'external_canonical';
      fs.mkdirSync(path.dirname(configPath), { recursive: true });
      fs.writeFileSync(configPath, JSON.stringify({
        mcpServers: {
          [serverName]: {
            command: process.execPath,
            args: [serverPath],
            env: { MCP_ALIAS: label },
            cwd: '.',
            timeoutMs: 5_000,
          },
        },
      }, null, 2));

      const context = {
        cwd: workspace,
        isProjectTrusted: async () => true,
        ui: { setStatus: () => undefined },
      } as unknown as PiContext;

      const described = await invokeMcpTool(mcpTool, {
        action: 'describe',
        server: serverName,
        tool: 'probe',
      }, context);
      assert.equal(described.isError, false, `${label}: describe must succeed: ${resultText(described)}`);
      assert.match(resultText(described), /Prove the external Node MCP process received its config\./);
      assert.match(resultText(described), /"message"/);
      assert.match(resultText(described), /External Node stdio MCP integration fixture\./);

      const called = await invokeMcpTool(mcpTool, {
        action: 'call',
        server: serverName,
        tool: 'probe',
        arguments: { message: 'ok' },
      }, context);
      assert.equal(called.isError, false, `${label}: call must succeed`);
      assert.match(resultText(called), new RegExp(`external:${label}:.*:ok`));

      const stopped = await invokeMcpTool(mcpTool, {
        action: 'stop',
        server: serverName,
      }, context);
      assert.equal(stopped.isError, false, `${label}: stop must succeed`);
    }
  } finally {
    stopAllMcpServers();
    if (previousOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
    else process.env.OCTOCODE_HOME = previousOctocodeHome;
    fs.rmSync(root, { recursive: true, force: true });
  }
}, 30_000);
