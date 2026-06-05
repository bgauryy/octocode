import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import { BulkRipgrepQuerySchema } from '../../scheme/localSchemaOverlay.js';
import { executeRipgrepSearch } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalSearchCodeOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

export function registerLocalRipgrepTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_RIPGREP,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_RIPGREP],
      inputSchema: toMCPSchema(BulkRipgrepQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalSearchCodeOutputSchema)
      ),
      annotations: {
        title: 'Local Ripgrep Search',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeRipgrepSearch, TOOL_NAMES.LOCAL_RIPGREP)
  );
}
