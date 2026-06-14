import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import {
  TOOL_NAMES,
  DESCRIPTIONS,
  LocalViewStructureBulkQuerySchema,
  executeViewStructure,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalViewStructureOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

export function registerLocalViewStructureTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_VIEW_STRUCTURE],
      inputSchema: toMCPSchema(LocalViewStructureBulkQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalViewStructureOutputSchema)
      ),
      annotations: {
        title: 'Local View Structure',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeViewStructure,
      TOOL_NAMES.LOCAL_VIEW_STRUCTURE
    )
  );
}
