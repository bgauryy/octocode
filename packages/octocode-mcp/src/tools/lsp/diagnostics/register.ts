import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../../types/toolTypes.js';
import { withResponseEnvelope } from '../../../scheme/responseEnvelope.js';
import { withBasicSecurityValidation } from '../../../utils/securityBridge.js';
import { BulkLspGetDiagnosticsQuerySchema } from './scheme.js';
import { LspGetDiagnosticsOutputSchema } from './outputSchema.js';
import { executeLspGetDiagnostics } from './execution.js';
import { LSP_GET_DIAGNOSTICS_TOOL_NAME } from '../shared/semanticTypes.js';
import { DESCRIPTIONS } from '../../toolMetadata/proxies.js';

export function registerLspGetDiagnosticsTool(server: McpServer) {
  return server.registerTool(
    LSP_GET_DIAGNOSTICS_TOOL_NAME,
    {
      description: DESCRIPTIONS[LSP_GET_DIAGNOSTICS_TOOL_NAME],
      inputSchema: toMCPSchema(BulkLspGetDiagnosticsQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LspGetDiagnosticsOutputSchema)
      ),
      annotations: {
        title: 'Get Diagnostics',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeLspGetDiagnostics,
      LSP_GET_DIAGNOSTICS_TOOL_NAME
    )
  );
}
