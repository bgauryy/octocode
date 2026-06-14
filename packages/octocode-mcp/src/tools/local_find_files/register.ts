import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import {
  TOOL_NAMES,
  DESCRIPTIONS,
  LocalFindFilesBulkQuerySchema,
  executeFindFiles,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalFindFilesOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

export function registerLocalFindFilesTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_FIND_FILES,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_FIND_FILES],
      inputSchema: toMCPSchema(LocalFindFilesBulkQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalFindFilesOutputSchema)
      ),
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
