import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';
import { createBasicToolRegistration } from '../../src/tools/registerBasicTool.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

const inputSchema = z.object({
  queries: z.array(z.object({ path: z.string() })).optional(),
});

describe('createBasicToolRegistration', () => {
  it('registers a basic MCP adapter using the shared tools-core security boundary', async () => {
    const mcp = createMockMcpServer();
    let receivedArgs: unknown;

    const register = createBasicToolRegistration({
      name: TOOL_NAMES.LOCAL_FIND_FILES,
      title: 'Local Find Files',
      inputSchema,
      executionFn: async args => {
        receivedArgs = args;
        return {
          content: [{ type: 'text', text: 'ok' }],
          structuredContent: { ok: true },
        };
      },
    });

    register(mcp.server);

    expect(mcp.registrations).toHaveLength(1);
    expect(mcp.registrations[0]).toMatchObject({
      name: TOOL_NAMES.LOCAL_FIND_FILES,
      options: {
        description: expect.any(String),
        inputSchema: expect.any(Object),
        annotations: {
          title: 'Local Find Files',
          readOnlyHint: true,
          destructiveHint: false,
          idempotentHint: true,
          openWorldHint: false,
        },
      },
    });

    const result = await mcp.callTool(TOOL_NAMES.LOCAL_FIND_FILES, {
      queries: [{ path: '.' }],
    });

    expect(receivedArgs).toEqual({ queries: [{ path: '.' }] });
    expect(result).toMatchObject({
      content: [{ type: 'text', text: 'ok' }],
      structuredContent: { ok: true },
    });
  });
});
