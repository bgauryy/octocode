import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { LSPFindReferencesQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPFindReferencesQuery = z.infer<
  typeof LSPFindReferencesQuerySchema
>;
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { findReferences } from './lsp_find_references.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type LSPFindReferencesQuery =
  WithOptionalMeta<UpstreamLSPFindReferencesQuery> & { orderHint?: number };
import { TOOL_NAME } from './constants.js';
import { executeWithToolBoundary } from '../executionGuard.js';

export { applyFindReferencesVerbosity } from './lsp_find_references.js';

export async function executeFindReferences(
  args: ToolExecutionArgs<LSPFindReferencesQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: LSPFindReferencesQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAME,
        query,
        contextMessage: 'lspFindReferences execution failed',
        execute: async () => findReferences(query),
      }),
    {
      toolName: TOOL_NAME,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    }
  );
}
