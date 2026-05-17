import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LOCAL_FIND_FILES_DESCRIPTION } from '@octocodeai/octocode-core';
import { BulkFindFilesSchema } from '../../scheme/localSchemaOverlay.js';
import { executeFindFiles } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalFindFilesOutputSchema } from '@octocodeai/octocode-core';

/**
 * Register the local find files tool with the MCP server.
 */
export function registerLocalFindFilesTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_FIND_FILES,
    {
      description: LOCAL_FIND_FILES_DESCRIPTION,
      inputSchema: toMCPSchema(BulkFindFilesSchema),
      outputSchema: toMCPSchema(LocalFindFilesOutputSchema),
      annotations: {
        title: 'Local Find Files',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeFindFiles, TOOL_NAMES.LOCAL_FIND_FILES)
  );
}
