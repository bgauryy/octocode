/**
 * Integration test: end-to-end crash isolation through a real MCP transport.
 *
 * Spawns a real `McpServer` + `Client` pair connected via `InMemoryTransport`,
 * registers tools via the `withOutputSanitization` proxy, then
 * verifies that:
 *
 *   1. A throwing tool returns an `isError: true` `CallToolResult` to the
 *      client (the MCP server does not crash).
 *   2. The error response is sanitized (no secrets leak through the wire).
 *   3. A throwing tool with a strict `outputSchema` does NOT cause the SDK
 *      to reject our structured error payload — confirms that `isError: true`
 *      short-circuits `validateToolOutput` (caveat #2 regression test).
 *   4. After a tool exception the server still responds to a
 *      well-behaved tool call (liveness guarantee).
 */

import { describe, it, expect } from 'vitest';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { z } from 'zod';
import { withOutputSanitization } from '../../src/utils/secureServer.js';

async function setupPair() {
  const server = new McpServer({
    name: 'test-server',
    version: '0.0.0',
  });
  const secure = withOutputSanitization(server);

  const client = new Client({ name: 'test-client', version: '0.0.0' });
  const [serverTransport, clientTransport] =
    InMemoryTransport.createLinkedPair();

  return { server, secure, client, serverTransport, clientTransport };
}

describe('secureServer integration (real McpServer + InMemoryTransport)', () => {
  it('returns isError result for a throwing tool without crashing the server', async () => {
    const { secure, client, serverTransport, clientTransport, server } =
      await setupPair();

    secure.registerTool(
      'explode',
      {
        description: 'Always throws',
        inputSchema: {},
      },
      async () => {
        throw new Error(
          'leaking ghp_abc123xyz456789012345678901234567890 in error'
        );
      }
    );

    secure.registerTool(
      'ping',
      { description: 'simple', inputSchema: {} },
      async () => ({
        content: [{ type: 'text', text: 'pong' }],
      })
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({ name: 'explode', arguments: {} });

    expect(result.isError).toBe(true);
    const text = (result.content as Array<{ type: string; text: string }>)[0]!
      .text;
    expect(text).toContain('explode');
    expect(text).not.toContain('ghp_abc123xyz456789012345678901234567890');

    // Liveness: server is still serving requests after the tool exception.
    const okResult = await client.callTool({ name: 'ping', arguments: {} });
    expect(okResult.isError).toBeFalsy();
    expect(
      (okResult.content as Array<{ type: string; text: string }>)[0]!.text
    ).toBe('pong');

    await client.close();
    await server.close();
  });

  it('does NOT trigger outputSchema validation on the error path (isError: true)', async () => {
    const { secure, client, serverTransport, clientTransport, server } =
      await setupPair();

    // Register a tool with a STRICT output schema that the error response
    // shape cannot satisfy. If the SDK validated structuredContent on the
    // error path it would emit an MCP protocol error instead of our
    // sanitized CallToolResult.
    secure.registerTool(
      'strictExplode',
      {
        description: 'strict schema, always throws',
        inputSchema: {},
        outputSchema: {
          totalMatches: z.number(),
          files: z.array(z.string()),
        },
      },
      async () => {
        throw new Error('boom');
      }
    );

    await Promise.all([
      server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: 'strictExplode',
      arguments: {},
    });

    expect(result.isError).toBe(true);
    // The SDK accepted our structured error payload (which intentionally
    // does NOT match the declared outputSchema) because isError short-circuits
    // validateToolOutput. This is the regression guard for caveat #2.
    expect(result.structuredContent).toBeDefined();

    await client.close();
    await server.close();
  });
});
