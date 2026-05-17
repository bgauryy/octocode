/**
 * LSP Go To Definition tool
 * Navigates to the definition of a symbol using Language Server Protocol
 * @module tools/lsp_goto_definition
 */

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';

import { LSP_GOTO_DEFINITION_DESCRIPTION } from '@octocodeai/octocode-core';
import { BulkLSPGotoDefinitionQuerySchema } from '../../scheme/lspSchemaOverlay.js';
import { executeGotoDefinition, TOOL_NAME } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LspGotoDefinitionOutputSchema } from '@octocodeai/octocode-core';

/**
 * Register the LSP Go To Definition tool with the MCP server.
 */
export function registerLSPGotoDefinitionTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAME,
    {
      description: LSP_GOTO_DEFINITION_DESCRIPTION,
      inputSchema: toMCPSchema(BulkLSPGotoDefinitionQuerySchema),
      outputSchema: toMCPSchema(LspGotoDefinitionOutputSchema),
      annotations: {
        title: 'Go To Definition',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeGotoDefinition, TOOL_NAME)
  );
}
