import { afterEach, describe, expect, it, vi } from 'vitest';
import { Client } from '@modelcontextprotocol/client';
import { McpServer, InMemoryTransport } from '@modelcontextprotocol/server';
import { z } from 'zod';

const FEATURE_FLAGS = ['ENABLE_LOCAL', 'ENABLE_CLONE'] as const;

afterEach(() => {
  for (const flag of FEATURE_FLAGS) delete process.env[flag];
  vi.resetModules();
});

describe('all-tool real SDK catalog parity', () => {
  it('lists the same 10 canonical contracts through a real MCP client without output schemas', async () => {
    for (const flag of FEATURE_FLAGS) process.env[flag] = 'true';
    vi.resetModules();

    const { DIRECT_TOOL_DISCOVERY_DEFINITIONS } =
      await import('@octocodeai/octocode-tools-core');
    const { ALL_TOOLS } = await import('../../src/tools/toolConfig.js');

    const expectedNames = [
      'ghSearch',
      'ghGetFileContent',
      'ghSearchHistory',
      'ghGetHistoryItem',
      'npmSearch',
      'ghCloneRepo',
      'localSearch',
      'localAnalyzeGraph',
      'localGetFileContent',
      'lspGetSemantics',
    ];
    const legacyNames = [
      'ghSearchPullRequests',
      'ghSearchIssues',
      'ghSearchCommits',
      'ghListReleases',
      'ghSearchDiscussions',
    ];

    expect(ALL_TOOLS.map(tool => tool.name)).toEqual(expectedNames);
    expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name)).toEqual(
      expectedNames
    );
    expect(ALL_TOOLS.map(tool => tool.name)).not.toEqual(
      expect.arrayContaining(legacyNames)
    );

    const server = new McpServer({ name: 'catalog-test', version: '0.0.0' });
    for (const tool of ALL_TOOLS) tool.fn(server);

    const client = new Client({ name: 'catalog-client', version: '0.0.0' });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    try {
      const listed = await client.listTools();
      expect(listed.tools.map(tool => tool.name)).toEqual(
        DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name)
      );

      for (const listedTool of listed.tools) {
        const canonical = DIRECT_TOOL_DISCOVERY_DEFINITIONS.find(
          tool => tool.name === listedTool.name
        );
        expect(canonical).toBeDefined();
        expect(listedTool.title).toBe(canonical!.title);
        expect(listedTool.description).toBe(canonical!.description);
        expect(listedTool.inputSchema).toMatchObject({
          type: 'object',
          properties: { queries: expect.any(Object) },
          required: ['queries'],
        });
        const expectedSchema = z.toJSONSchema(canonical!.inputSchema, {
          io: 'input',
        });
        expect(listedTool.inputSchema).toEqual(expectedSchema);
        expect(listedTool).not.toHaveProperty('outputSchema');
      }
    } finally {
      await client.close();
      await server.close();
    }
  });
});
