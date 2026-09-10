import { z } from 'zod';

export function mcpGatewayItemSchema() {
  return z.looseObject({
    action: z.enum([
      'list','describe','call','resources','read-resource','prompts','get-prompt',
      'complete','enable','disable','status','restart','stop','config','add','remove',
    ]).describe('list: enabled server instructions and tool descriptions with continuations. describe: exact schema. Calls validate against cached exact schemas.'),
    offset: z.number().int().nonnegative().optional().describe('list continuation row; copy next.params unchanged.'),
    textOffset: z.number().int().nonnegative().optional().describe('list continuation within a long instruction or description.'),
    limit: z.number().int().min(1).max(50).optional().describe('list rows per page, up to 50.'),
    catalogRevision: z.string().optional().describe('list continuation revision; copy from next.params.'),
    server: z.string().optional().describe('MCP server name. For add/remove this is the key written to mcp.json.'),
    tool: z.string().optional().describe('MCP tool name for describe/call.'),
    uri: z.string().optional().describe('Resource URI for read-resource.'),
    name: z.string().optional().describe('Prompt name for get-prompt.'),
    ref: z.record(z.string(), z.unknown()).optional().describe('Prompt or resource-template reference for complete.'),
    argument: z.record(z.string(), z.unknown()).optional().describe('Partial argument for complete.'),
    arguments: z.record(z.string(), z.unknown()).optional().describe('Selected tool input for action:call. Put every target-tool field here, never beside action/server/tool. Octocode tool queries nest under arguments.queries[].'),
    responseView: z.enum(['full', 'table']).optional().describe('call output: full evidence (default) or a compact table for large batches.'),
    config: z.record(z.string(), z.unknown()).optional().describe('Server config for add: stdio {command,args?,env?,cwd?} or HTTP {url,headers?}.'),
    scope: z.enum(['project', 'global']).optional().describe('add/remove target: project (.agents/mcp.json) or global ($OCTOCODE_HOME/mcp.json).'),
  });
}
