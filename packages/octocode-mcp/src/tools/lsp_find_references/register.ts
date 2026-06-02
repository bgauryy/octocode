import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { BulkLSPFindReferencesQuerySchema } from '../../scheme/lspSchemaOverlay.js';
import { executeFindReferences } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LspFindReferencesOutputLocalSchema } from '../../scheme/lspOutputSchemaOverlay.js';
import { TOOL_NAME } from './constants.js';
import { DESCRIPTIONS } from '../toolMetadata/proxies.js';

export function registerLSPFindReferencesTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAME,
    {
      description: DESCRIPTIONS[TOOL_NAME],
      inputSchema: toMCPSchema(BulkLSPFindReferencesQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LspFindReferencesOutputLocalSchema)
      ),
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
