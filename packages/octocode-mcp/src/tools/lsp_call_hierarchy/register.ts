import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import {
  LSP_CALL_HIERARCHY_DESCRIPTION,
  LspCallHierarchyOutputSchema,
} from '@octocodeai/octocode-core';
import { BulkLSPCallHierarchyQuerySchema } from '../../scheme/lspSchemaOverlay.js';
import { executeCallHierarchy } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';

/**
 * Register the LSP call hierarchy tool with the MCP server.
 */
export function registerLSPCallHierarchyTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LSP_CALL_HIERARCHY,
    {
      description: LSP_CALL_HIERARCHY_DESCRIPTION,
      inputSchema: toMCPSchema(BulkLSPCallHierarchyQuerySchema),
      outputSchema: toMCPSchema(LspCallHierarchyOutputSchema),
      annotations: {
        title: 'Call Hierarchy',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeCallHierarchy,
      TOOL_NAMES.LSP_CALL_HIERARCHY
    )
  );
}
