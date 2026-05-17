import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LOCAL_FETCH_CONTENT_DESCRIPTION } from '@octocodeai/octocode-core';
import { BulkFetchContentQuerySchema } from '../../scheme/localSchemaOverlay.js';
import { executeFetchContent } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalGetFileContentOutputSchema } from '@octocodeai/octocode-core';

/**
 * Register the local fetch content tool with the MCP server.
 */
export function registerLocalFetchContentTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_FETCH_CONTENT,
    {
      description: LOCAL_FETCH_CONTENT_DESCRIPTION,
      inputSchema: toMCPSchema(BulkFetchContentQuerySchema),
      outputSchema: toMCPSchema(LocalGetFileContentOutputSchema),
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
