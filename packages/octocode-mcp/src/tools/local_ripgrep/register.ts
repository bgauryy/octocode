import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import {
  TOOL_NAMES,
  DESCRIPTIONS,
  LocalRipgrepBulkQuerySchema,
  executeRipgrepSearch,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { withBasicSecurityValidation } from '@octocodeai/octocode-tools-core';
import { LocalSearchCodeOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

export function registerLocalRipgrepTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_RIPGREP,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_RIPGREP],
      inputSchema: toMCPSchema(LocalRipgrepBulkQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalSearchCodeOutputSchema)
      ),
      annotations: {
        title: 'Local Ripgrep Search',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeRipgrepSearch, TOOL_NAMES.LOCAL_RIPGREP)
  );
}
