/**
 * Integration test: verifies that the MCP server correctly exposes
 * instructions and tool descriptions via the MCP protocol.
 *
 * Uses InMemoryTransport to connect a real McpServer + Client pair,
 * then inspects the `initialize` response and `listTools` output.
 */
import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import type { Implementation } from '@modelcontextprotocol/sdk/types.js';
import {
  buildMockMetadata,
  collectJsonSchemaDescriptions,
  TOOL_NAMES_MAP,
} from './fixtures.js';

const MOCK_INSTRUCTIONS =
  'You are the Octocode MCP research assistant. Use tools wisely.';

describe('MCP protocol exposure: instructions and tools', () => {
  let client: Client;
  let mcpServer: McpServer;

  beforeAll(async () => {
    vi.resetModules();

    const metadata = buildMockMetadata({
      instructions: MOCK_INSTRUCTIONS,
    });

    vi.doMock('@octocodeai/octocode-core', async importOriginal => {
      const actual =
        await importOriginal<typeof import('@octocodeai/octocode-core')>();
      return {
        ...actual,
        octocodeConfig: metadata,
        completeMetadata: metadata,
      };
    });

    const { _resetMetadataState, initializeToolMetadata } =
      await import('../../src/tools/toolMetadata/state.js');
    _resetMetadataState();
    await initializeToolMetadata();

    const { initialize } = await import('../../src/serverConfig.js');
    await initialize();

    const serverConfig: Implementation = {
      name: 'octocode-mcp-test',
      version: '1.0.0',
    };

    mcpServer = new McpServer(serverConfig, {
      capabilities: {
        tools: { listChanged: false },
        logging: {},
      },
      instructions: MOCK_INSTRUCTIONS,
    });

    const { registerTools } = await import('../../src/tools/toolsManager.js');
    await registerTools(mcpServer);

    const [clientTransport, serverTransport] =
      InMemoryTransport.createLinkedPair();

    client = new Client({ name: 'test-client', version: '1.0.0' });

    await mcpServer.connect(serverTransport);
    await client.connect(clientTransport);
  }, 30000);

  afterAll(async () => {
    await client?.close();
    await mcpServer?.close();
  });

  it('client receives non-empty instructions from the server', () => {
    const instructions = client.getInstructions();
    expect(instructions).toBeDefined();
    expect(typeof instructions).toBe('string');
    expect(instructions!.length).toBeGreaterThan(0);
  });

  it('instructions contain the metadata instructions text', () => {
    const instructions = client.getInstructions()!;
    expect(instructions).toContain(MOCK_INSTRUCTIONS);
  });

  it('server exposes tools with non-empty descriptions', async () => {
    const { tools } = await client.listTools();

    expect(tools.length).toBeGreaterThan(0);

    for (const tool of tools) {
      expect(
        tool.description,
        `Tool ${tool.name} should have a non-empty description`
      ).toBeTruthy();
      expect(tool.description!.length).toBeGreaterThan(5);
    }
  });

  it('every tool inputSchema has zero empty descriptions', async () => {
    const { tools } = await client.listTools();

    const KNOWN_UPSTREAM_EMPTY = new Set(['charOffset', 'charLength']);
    let totalDescriptions = 0;
    let emptyDescriptions = 0;

    for (const tool of tools) {
      expect(tool.inputSchema).toBeDefined();
      const descriptions = collectJsonSchemaDescriptions(
        tool.inputSchema,
        tool.name
      );
      totalDescriptions += descriptions.length;
      emptyDescriptions += descriptions.filter(
        d => d.description === '' && !KNOWN_UPSTREAM_EMPTY.has(d.fieldName)
      ).length;
    }

    expect(totalDescriptions).toBeGreaterThan(50);
    expect(emptyDescriptions).toBe(0);
  });

  it('every tool name matches one of the expected tool names', async () => {
    const { tools } = await client.listTools();
    const expectedNames = Object.values(TOOL_NAMES_MAP);

    for (const tool of tools) {
      expect(expectedNames).toContain(tool.name);
    }
  });
});
