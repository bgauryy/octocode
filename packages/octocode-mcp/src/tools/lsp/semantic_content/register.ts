import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../../types/toolTypes.js';
import { withResponseEnvelope } from '../../../scheme/responseEnvelope.js';
import { withBasicSecurityValidation } from '../../../utils/securityBridge.js';
import { BulkLspGetSemanticContentQuerySchema } from './scheme.js';
import { LspGetSemanticContentOutputSchema } from './outputSchema.js';
import { executeLspGetSemanticContent } from './execution.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../shared/semanticTypes.js';

const DESCRIPTION = `\
Typed semantic queries on a local language server (TS/JS built-in; 30+ langs via installed servers). \
Pair with lspGetDiagnostics: use this tool to navigate and understand code, use lspGetDiagnostics to verify health after edits.

PREREQUISITE — always run localSearchCode first to get exact filePath + lineHint.

TYPES (set type=):
  definition      → jump to where a symbol is declared (returns snippet)
  references      → all usages across the workspace; groupByFile=true for file-level summary
  callers         → who calls this function (incoming); depth controls recursion (default 1)
  callees         → what this function calls (outgoing); depth controls recursion
  callHierarchy   → bidirectional callers+callees in one shot; use type="callers"/"callees" for focus
  hover           → type signature + doc comment (fast, no lineHint search)
  documentSymbols → full symbol outline of a file (no lineHint needed); paginate with page=
  typeDefinition  → jump to the declared type of an expression
  implementation  → concrete implementations of an interface or abstract method

FLOW (typical research chain):
  1. localSearchCode → filePath + lineHint
  2. lspGetSemanticContent type=definition → confirm symbol location
  3. type=references or type=callers/callees → blast radius / call graph
  4. lspGetDiagnostics → verify impacted files after edits`;

export function registerLspGetSemanticContentTool(server: McpServer) {
  return server.registerTool(
    LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
    {
      description: DESCRIPTION,
      inputSchema: toMCPSchema(BulkLspGetSemanticContentQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LspGetSemanticContentOutputSchema)
      ),
      annotations: {
        title: 'Get Semantic Content',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeLspGetSemanticContent,
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
    )
  );
}

export { DESCRIPTION as LSP_GET_SEMANTIC_CONTENT_DESCRIPTION };
