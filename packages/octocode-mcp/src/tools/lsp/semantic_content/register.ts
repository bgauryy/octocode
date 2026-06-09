import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../../types/toolTypes.js';
import { withResponseEnvelope } from '../../../scheme/responseEnvelope.js';
import { withBasicSecurityValidation } from '../../../utils/securityBridge.js';
import { BulkLspGetSemanticContentQuerySchema } from './scheme.js';
import { LspGetSemanticContentOutputSchema } from './outputSchema.js';
import { executeLspGetSemanticContent } from './execution.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../shared/semanticTypes.js';
import { DESCRIPTIONS } from '../../toolMetadata/proxies.js';

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
