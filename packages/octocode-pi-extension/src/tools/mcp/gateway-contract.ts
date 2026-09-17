import { z } from 'zod';

const nonempty = z.string().min(1);
const record = (description: string) =>
  z.record(z.string(), z.unknown()).describe(description);
const scope = z.enum(['project', 'global']).optional().describe('project or global.');
const researchServer = nonempty.optional().describe('Defaults to octocode.');

/** One strict branch per MCP operation; impossible field combinations never reach runtime. */
export function mcpGatewayItemSchema() {
  return z.union([
    z.strictObject({
      action: z.enum(['list']),
      offset: z.number().int().nonnegative().optional().describe('Continuation row.'),
      textOffset: z.number().int().nonnegative().optional().describe('Text offset.'),
      limit: z.number().int().min(1).max(50).optional().describe('Page size ≤50.'),
      catalogRevision: z.string().optional().describe('Catalog revision.'),
    }),
    z.strictObject({
      action: z.enum(['describe']),
      server: researchServer,
      tool: nonempty,
    }),
    z.strictObject({
      action: z.enum(['call']),
      server: researchServer,
      tool: nonempty,
      arguments: record('Tool input. Octocode nests target calls under arguments.queries[].').optional(),
      responseView: z.enum(['full', 'table']).optional().describe('full (default) or table.'),
    }),
    z.strictObject({ action: z.enum(['resources']), server: researchServer }),
    z.strictObject({
      action: z.enum(['read-resource']),
      server: researchServer,
      uri: nonempty,
    }),
    z.strictObject({ action: z.enum(['prompts']), server: researchServer }),
    z.strictObject({
      action: z.enum(['get-prompt']),
      server: researchServer,
      name: nonempty,
      arguments: record('Prompt arguments.').optional(),
    }),
    z.strictObject({
      action: z.enum(['complete']),
      server: researchServer,
      ref: record('Completion reference.'),
      argument: record('Completion argument.'),
    }),
    z.strictObject({
      action: z.enum(['enable']),
      server: nonempty,
      tool: nonempty.optional().describe('Omit to enable the server.'),
      scope,
    }),
    z.strictObject({
      action: z.enum(['disable']),
      server: nonempty,
      tool: nonempty.optional().describe('Omit to disable the server.'),
      scope,
    }),
    z.strictObject({ action: z.enum(['status']) }),
    z.strictObject({ action: z.enum(['restart']), server: nonempty }),
    z.strictObject({ action: z.enum(['stop']), server: nonempty.optional().describe('Omit to stop all servers.') }),
    z.strictObject({ action: z.enum(['config']) }),
    z.strictObject({
      action: z.enum(['add']),
      server: nonempty,
      config: record('Server config: stdio {command,...} or HTTP {url,...}.'),
      scope,
    }),
    z.strictObject({
      action: z.enum(['remove']),
      server: nonempty,
      scope,
    }),
  ]);
}
