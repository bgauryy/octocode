import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../../types/toolTypes.js';
import { withResponseEnvelope } from '../../../scheme/responseEnvelope.js';
import { withBasicSecurityValidation } from '../../../utils/securityBridge.js';
import { BulkLspGetDiagnosticsQuerySchema } from './scheme.js';
import { LspGetDiagnosticsOutputSchema } from './outputSchema.js';
import { executeLspGetDiagnostics } from './execution.js';
import { LSP_GET_DIAGNOSTICS_TOOL_NAME } from '../shared/semanticTypes.js';

const DESCRIPTION =
  'Return language-server diagnostics for a local file. Use this after localSearchCode and lspGetSemanticContent identify impacted files, or whenever fast file-health evidence is needed before heavier build, lint, or test commands.';

export function registerLspGetDiagnosticsTool(server: McpServer) {
  return server.registerTool(
    LSP_GET_DIAGNOSTICS_TOOL_NAME,
    {
      description: DESCRIPTION,
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

export { DESCRIPTION as LSP_GET_DIAGNOSTICS_DESCRIPTION };
