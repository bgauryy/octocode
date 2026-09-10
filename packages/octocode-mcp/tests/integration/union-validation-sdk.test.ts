import { describe, expect, it } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport, McpServer } from '@modelcontextprotocol/server';
import {
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  buildDirectToolCommandPatterns,
  prepareDirectToolInput,
} from '@octocodeai/octocode-core/schema';

describe('canonical union repair through the real MCP SDK', () => {
  it('returns actionable validation errors before execution for all four opaque unions', async () => {
    const server = new McpServer({ name: 'union-validation', version: '1' });
    let executions = 0;
    for (const tool of DIRECT_TOOL_DISCOVERY_DEFINITIONS) {
      server.registerTool(
        tool.name,
        { inputSchema: tool.inputSchema },
        async () => {
          executions += 1;
          return { content: [] };
        }
      );
    }
    const client = new Client({
      name: 'union-validation-client',
      version: '1',
    });
    const [serverTransport, clientTransport] =
      InMemoryTransport.createLinkedPair();
    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);
    try {
      const cases = [
        {
          name: 'artifactSearch',
          query: { type: 'npm' },
          expected: /packageName.*keywords/,
        },
        {
          name: 'artifactSearch',
          query: { type: 'npm', name: 'zod' },
          expected: /name/,
        },
        {
          name: 'artifactSearch',
          query: { type: 'pypi', packageName: 'httpx', keywords: ['http'] },
          expected: /keywords/,
        },
        { name: 'ghGetFileContent', query: {}, expected: /owner|repo|path/ },
        { name: 'localFetch', query: {}, expected: /path/ },
        {
          name: 'astSearch',
          query: { operation: 'tree', treeKind: 'syntax' },
          expected: /path/,
        },
      ];
      for (const item of cases) {
        const result = await client.callTool({
          name: item.name,
          arguments: { queries: [item.query] },
        });
        expect(result.isError).toBe(true);
        const content = Array.isArray(result.content) ? result.content : [];
        const text = content
          .filter(
            (block): block is { type: 'text'; text: string } =>
              typeof block === 'object' &&
              block !== null &&
              block.type === 'text' &&
              typeof block.text === 'string'
          )
          .map(block => block.text)
          .join('\n');
        expect(text).toMatch(item.expected);
        expect(text).toContain('queries.0');
      }
      expect(executions).toBe(0);
      for (const tool of DIRECT_TOOL_DISCOVERY_DEFINITIONS) {
        const example = buildDirectToolCommandPatterns(tool.name)[0]!;
        const result = await client.callTool({
          name: tool.name,
          arguments: prepareDirectToolInput(tool.name, example.query),
        });
        expect(result.isError).not.toBe(true);
      }
      expect(executions).toBe(DIRECT_TOOL_DISCOVERY_DEFINITIONS.length);
    } finally {
      await client.close();
      await server.close();
    }
  });
});
