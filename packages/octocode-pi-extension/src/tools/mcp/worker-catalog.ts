import type { CapabilitySnapshot } from '../../contracts/capabilities.js';
import { buildMcpCatalogSnapshot, type McpCatalogServerInput } from './catalog.js';

export function workerMcpCatalogSnapshot(snapshot: CapabilitySnapshot, cwd: string) {
  const servers = new Map<string, McpCatalogServerInput>();
  for (const tool of snapshot.mcpTools) {
    const server = servers.get(tool.server) ?? { name: tool.server, instructions: tool.instructions, tools: [] };
    server.tools.push({ name: tool.tool, description: tool.description, inputSchema: tool.inputSchema });
    servers.set(tool.server, server);
  }
  return buildMcpCatalogSnapshot({ cwd, sources: [], configSignatures: { parent: snapshot.revision }, servers: [...servers.values()] });
}
