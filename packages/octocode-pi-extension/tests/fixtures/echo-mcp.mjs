// Minimal stdio MCP server used by the end-to-end test.
import { McpServer } from '@modelcontextprotocol/server';
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import { readFileSync } from 'node:fs';
import { z } from 'zod';

const server = new McpServer({ name: 'echo', version: '1.0.0' }, { instructions: 'Echo test server.' });
server.registerTool(
  'shout',
  { description: 'Uppercase the given text', inputSchema: z.object({ text: z.string() }) },
  async ({ text }) => ({ content: [{ type: 'text', text: text.toUpperCase() }] }),
);
server.registerTool(
  'pick',
  {
    description: 'Echo union queries back (stand-in for Octocode operation branches)',
    inputSchema: z.object({
      queries: z.array(z.discriminatedUnion('operation', [
        z.object({ operation: z.literal('text'), searchText: z.string() }).strict(),
        z.object({ operation: z.literal('tree'), depth: z.number().int().optional() }).strict(),
      ])),
    }),
  },
  async ({ queries }) => ({ content: [{ type: 'text', text: JSON.stringify(queries) }] }),
);
server.registerTool(
  'fail',
  { description: 'Always fails', inputSchema: z.object({}) },
  async () => ({ content: [{ type: 'text', text: 'boom' }], isError: true }),
);
server.registerTool(
  'roots',
  { description: 'Report the client roots', inputSchema: z.object({}) },
  async () => ({ content: [{ type: 'text', text: JSON.stringify((await server.server.listRoots()).roots) }] }),
);
server.registerTool(
  'grow',
  { description: 'Register a new tool at runtime', inputSchema: z.object({}) },
  async () => {
    server.registerTool('late', { description: 'Added after startup', inputSchema: z.object({}) }, async () => ({ content: [{ type: 'text', text: 'late tool works' }] }));
    return { content: [{ type: 'text', text: 'grown' }] };
  },
);
server.registerTool(
  'localFetch',
  { description: 'Read files (stand-in for Octocode)', inputSchema: z.object({ queries: z.array(z.object({ path: z.string() })) }) },
  async ({ queries }) => ({ content: [{ type: 'text', text: queries.map((query) => readFileSync(query.path, 'utf8')).join('\n') }] }),
);
server.registerTool(
  'exit',
  { description: 'Stop the server process shortly after answering', inputSchema: z.object({}) },
  async () => {
    setTimeout(() => process.exit(0), 50);
    return { content: [{ type: 'text', text: 'bye' }] };
  },
);
await server.connect(new StdioServerTransport());
