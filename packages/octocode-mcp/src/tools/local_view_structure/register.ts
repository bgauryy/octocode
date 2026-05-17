import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LOCAL_VIEW_STRUCTURE_DESCRIPTION } from '@octocodeai/octocode-core';
import { BulkViewStructureSchema } from '../../scheme/localSchemaOverlay.js';
import { executeViewStructure } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalViewStructureOutputSchema } from '@octocodeai/octocode-core';

/**
 * Register the local view structure tool with the MCP server.
 * Follows the same pattern as other local tools for consistency.
 */
export function registerLocalViewStructureTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    {
      description: LOCAL_VIEW_STRUCTURE_DESCRIPTION,
      inputSchema: toMCPSchema(BulkViewStructureSchema),
      outputSchema: toMCPSchema(LocalViewStructureOutputSchema),
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
