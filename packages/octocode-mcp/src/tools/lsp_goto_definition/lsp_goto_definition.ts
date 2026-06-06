import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';

import { BulkLSPGotoDefinitionQuerySchema } from '../../scheme/lspSchemaOverlay.js';
import { executeGotoDefinition, TOOL_NAME } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LspGotoDefinitionOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';
import { DESCRIPTIONS } from '../toolMetadata/proxies.js';

export function registerLSPGotoDefinitionTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAME,
    {
      description: DESCRIPTIONS[TOOL_NAME],
      inputSchema: toMCPSchema(BulkLSPGotoDefinitionQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LspGotoDefinitionOutputSchema)
      ),
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
