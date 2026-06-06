import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { BulkLSPCallHierarchyQuerySchema } from '../../scheme/lspSchemaOverlay.js';
import { LspCallHierarchyOutputLocalSchema } from '../../scheme/lspOutputSchemaOverlay.js';
import { executeCallHierarchy } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';

export function registerLSPCallHierarchyTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LSP_CALL_HIERARCHY,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LSP_CALL_HIERARCHY],
      inputSchema: toMCPSchema(BulkLSPCallHierarchyQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LspCallHierarchyOutputLocalSchema)
      ),
      annotations: {
        title: 'Call Hierarchy',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeCallHierarchy,
      TOOL_NAMES.LSP_CALL_HIERARCHY
    )
  );
}
