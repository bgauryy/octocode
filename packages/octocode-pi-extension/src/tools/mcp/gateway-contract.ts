import { z } from 'zod';

export function mcpGatewayItemSchema() {
  return z.looseObject({
    action: z.enum([
      'list','describe','call','resources','read-resource','prompts','get-prompt',
      'complete','enable','disable','status','restart','stop','config','add','remove',
    ]).describe('list|describe|call|resources|read-resource|prompts|get-prompt|complete|enable|disable|status|restart|stop|config|add|remove'),
    offset: z.number().int().nonnegative().optional().describe('Continuation row.'),
    textOffset: z.number().int().nonnegative().optional().describe('Text offset.'),
    limit: z.number().int().min(1).max(50).optional().describe('Page size ≤50.'),
    catalogRevision: z.string().optional().describe('Catalog revision.'),
    server: z.string().optional().describe('Server name.'),
    tool: z.string().optional().describe('Tool name.'),
    uri: z.string().optional().describe('Resource URI.'),
    name: z.string().optional().describe('Prompt name.'),
    ref: z.record(z.string(), z.unknown()).optional().describe('Reference for complete.'),
    argument: z.record(z.string(), z.unknown()).optional().describe('Argument for complete.'),
    arguments: z.record(z.string(), z.unknown()).optional().describe('Tool input for call. Octocode nests under arguments.queries[].'),
    responseView: z.enum(['full', 'table']).optional().describe('full (default) or table.'),
    config: z.record(z.string(), z.unknown()).optional().describe('Server config: stdio {command,...} or http {url,...}.'),
    scope: z.enum(['project', 'global']).optional().describe('project or global.'),
  });
}
