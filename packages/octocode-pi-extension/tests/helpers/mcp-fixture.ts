import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { projectMcpPath } from '../../src/tools/mcp/config.js';
const MCP_SERVER_ENTRY = import.meta.resolve('@modelcontextprotocol/server');
const MCP_STDIO_ENTRY = import.meta.resolve('@modelcontextprotocol/server/stdio');

export function createDelayedMcpFixture(delayMs: number, tools = [{ name: 'mockTool', description: 'Mocked cache-flow tool', inputSchema: { type: 'object' } }]): {
  ctx: import("../../src/types.js").PiContext;
  serverPath: string;
  discoveryMarker: string;
  discoveryStartedMarker: string;
  cleanup: () => void;
} {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), ".tmp-mcp-warm-flow-"));
  process.env["OCTOCODE_HOME"] = path.join(cwd, ".octocode-home");
  const serverPath = path.join(cwd, "server.mjs");
  const discoveryMarker = path.join(cwd, "listed.marker");
  const discoveryStartedMarker = path.join(cwd, "listing.marker");
  fs.writeFileSync(
    serverPath,
    `
    import fs from 'node:fs';
    import { Server } from ${JSON.stringify(MCP_SERVER_ENTRY)};
    import { StdioServerTransport } from ${JSON.stringify(MCP_STDIO_ENTRY)};
    const server = new Server({ name: 'mock-cache-server', version: '1.0.0' }, { capabilities: { tools: {} } });
    server.setRequestHandler('tools/list', async () => {
      fs.writeFileSync(${JSON.stringify(discoveryStartedMarker)}, 'listing');
      await new Promise((resolve) => setTimeout(resolve, ${delayMs}));
      fs.writeFileSync(${JSON.stringify(discoveryMarker)}, 'listed');
      return { tools: ${JSON.stringify(tools)} };
    });
    await server.connect(new StdioServerTransport());
  `,
  );
  const configPath = projectMcpPath(cwd);
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(
    configPath,
    JSON.stringify({
      mcpServers: {
        octocode: {
          command: process.execPath,
          args: [serverPath],
          cwd,
          timeoutMs: 5_000,
        },
      },
    }),
  );
  return {
    ctx: {
      cwd,
      isProjectTrusted: () => true,
    } as unknown as import("../../src/types.js").PiContext,
    serverPath,
    discoveryMarker,
    discoveryStartedMarker,
    // A detached catalog warm may still be persisting its snapshot into this
    // fixture's OCTOCODE_HOME when the test tears down; rmSync then sees
    // ENOTEMPTY. Node's bounded retry absorbs that race, and a write that lands
    // after removal fails inside the warm's own error handler.
    cleanup: () => fs.rmSync(cwd, { recursive: true, force: true, maxRetries: 5, retryDelay: 20 }),
  };
}
