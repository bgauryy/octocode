import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { LSP_FIND_REFERENCES_DESCRIPTION } from '@octocodeai/octocode-core';
import { BulkLSPFindReferencesQuerySchema } from '../../scheme/lspSchemaOverlay.js';
import { executeFindReferences } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LspFindReferencesOutputSchema } from '@octocodeai/octocode-core';
import { TOOL_NAME } from './constants.js';

export function registerLSPFindReferencesTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAME,
    {
      description: LSP_FIND_REFERENCES_DESCRIPTION,
      inputSchema: toMCPSchema(BulkLSPFindReferencesQuerySchema),
      outputSchema: toMCPSchema(LspFindReferencesOutputSchema),
      annotations: {
        title: 'Find References',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeFindReferences, TOOL_NAME)
  );
}
