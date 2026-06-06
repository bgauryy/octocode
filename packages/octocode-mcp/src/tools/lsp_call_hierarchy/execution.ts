import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { LSPCallHierarchyQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPCallHierarchyQuery = z.infer<
  typeof LSPCallHierarchyQuerySchema
>;
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { processCallHierarchy } from './callHierarchy.js';
import type { CallHierarchyResult } from '../../lsp/types.js';
import { attachLspEvidence } from '../../lsp/evidence.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type LSPCallHierarchyQuery = WithOptionalMeta<UpstreamLSPCallHierarchyQuery> & {
  orderHint?: number;
};
import { TOOL_NAME } from './constants.js';
import { executeWithToolBoundary } from '../executionGuard.js';

export { applyCallHierarchyVerbosity } from './callHierarchy.js';

export async function executeCallHierarchy(
  args: ToolExecutionArgs<LSPCallHierarchyQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: LSPCallHierarchyQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAME,
        query,
        contextMessage: 'lspCallHierarchy execution failed',
        execute: async () =>
          attachCallHierarchyEvidence(await processCallHierarchy(query)),
      }),
    {
      toolName: TOOL_NAME,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    }
  );
}

function attachCallHierarchyEvidence(
  result: CallHierarchyResult
): CallHierarchyResult {
  return attachLspEvidence(result, {
    kind: 'calls',
    paginationKey: 'outputPagination',
  });
}
