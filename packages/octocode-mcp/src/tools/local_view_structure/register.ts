import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import { BulkViewStructureSchema } from '../../scheme/localSchemaOverlay.js';
import { executeViewStructure } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalViewStructureOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

export function registerLocalViewStructureTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_VIEW_STRUCTURE],
      inputSchema: toMCPSchema(BulkViewStructureSchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalViewStructureOutputSchema)
      ),
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
