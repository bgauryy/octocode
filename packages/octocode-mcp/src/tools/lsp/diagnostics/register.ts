import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../../types/toolTypes.js';
import { withResponseEnvelope } from '../../../scheme/responseEnvelope.js';
import { withBasicSecurityValidation } from '../../../utils/securityBridge.js';
import { BulkLspGetDiagnosticsQuerySchema } from './scheme.js';
import { LspGetDiagnosticsOutputSchema } from './outputSchema.js';
import { executeLspGetDiagnostics } from './execution.js';
import { LSP_GET_DIAGNOSTICS_TOOL_NAME } from '../shared/semanticTypes.js';

const DESCRIPTION = `\
Language-server diagnostics (errors, warnings, info, hints) for one or more local files. \
Pair with lspGetSemanticContent: use that tool to navigate code, use this tool to verify health after edits.

WHEN TO USE:
  • After any code change — check impacted files before running build/test
  • When lspGetSemanticContent finds references/callers — run diagnostics on those files
  • Fast triage of type errors, unused imports, or lint warnings without a full build

SEVERITY FILTER (set severity=):
  error       → type errors and hard failures only
  warning     → warnings and above
  information → info messages and above
  hint        → all diagnostics including style hints
  all         → same as hint (default)

FLOW (typical edit-verify chain):
  1. lspGetSemanticContent type=references → find all impacted files
  2. lspGetDiagnostics on each impacted file → verify no new errors
  3. If errors found → lspGetSemanticContent type=definition on the failing symbol to trace root cause`;

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
