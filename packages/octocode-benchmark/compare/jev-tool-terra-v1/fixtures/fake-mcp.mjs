import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';

// Offline fixture: no network, credentials, or production tool implementations.
const names = process.env.TOOLS_TO_RUN.split(',');
const server = new Server({ name: 'offline-fixture', version: '1' }, {
  capabilities: { tools: {} }, instructions: `Fixture enabled names: ${names.join(',')}`,
});
const tools = names.map(name => ({ name, description: `Fixture ${name}`, inputSchema: {
  type: 'object', properties: { queries: { type: 'array', items: { type: 'object' } } },
} }));
let calls = 0;
server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools }));
server.setRequestHandler(CallToolRequestSchema, async ({ params }) => {
  calls++;
  const args = params.arguments;
  if (params.name !== 'jev') return { content: [{ type: 'text', text: 'untouched\nfixture response' }],
    structuredContent: { calls, echo: args }, _meta: { fixture: true } };
  const queries = args.queries ?? [args];
  const allFailed = queries[0].context?.value === 'fail';
  const owner = queries[0].context?.value === 'mixed' ? 1 : 0;
  const results = queries.map((_, index) => allFailed || index < owner
    ? { index, status: 'error', data: { errorCode: 'invalidProviderResponse' } }
    : { index, data: { model: 'fixture', answer: { type: 'noul', noul: 0.8 },
      usage: { input_tokens: index === owner ? 12 : 0, output_tokens: index === owner ? 3 : 0 },
      ...(queries.length > 1 ? { usageAttribution: { ownerIndex: owner, sharedWith: queries.map((_, i) => i) } } : {}) } });
  return { content: [{ type: 'text', text: JSON.stringify({ results }) }], structuredContent: { results } };
});
await server.connect(new StdioServerTransport());
