import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../../types/toolTypes.js';
import { withBasicSecurityValidation } from '@octocodeai/octocode-tools-core';
import {
  DESCRIPTIONS,
  BulkLspGetSemanticContentQuerySchema,
  LspGetSemanticContentOutputSchema,
  executeLspGetSemanticContent,
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';

export function registerLspGetSemanticContentTool(server: McpServer) {
  return server.registerTool(
    LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
    {
      description: DESCRIPTIONS[LSP_GET_SEMANTIC_CONTENT_TOOL_NAME],
      inputSchema: toMCPSchema(BulkLspGetSemanticContentQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LspGetSemanticContentOutputSchema)
      ),
      annotations: {
        title: 'Get Semantic Content',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeLspGetSemanticContent,
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
    )
  );
}
