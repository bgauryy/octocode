import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import { BulkFetchContentQuerySchema } from '../../scheme/localSchemaOverlay.js';
import { executeFetchContent } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalGetFileContentOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

/**
 * Register the local fetch content tool with the MCP server.
 */
export function registerLocalFetchContentTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_FETCH_CONTENT,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_FETCH_CONTENT],
      inputSchema: toMCPSchema(BulkFetchContentQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalGetFileContentOutputSchema)
      ),
      annotations: {
        title: 'Local Fetch Content',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeFetchContent,
      TOOL_NAMES.LOCAL_FETCH_CONTENT
    )
  );
}
