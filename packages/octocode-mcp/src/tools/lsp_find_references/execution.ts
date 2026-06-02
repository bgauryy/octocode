import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
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

// Re-exported so every tool exposes `apply<Tool>Verbosity` from execution.ts.
export { applyFindReferencesVerbosity } from './lsp_find_references.js';

/**
 * Execute bulk find references operation.
 * Wraps findReferences with bulk operation handling for multiple queries.
 */
export async function executeFindReferences(
  args: ToolExecutionArgs<LSPFindReferencesQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset } = args;
  // LSP reference results must always be returned in full — char-based
  // pagination forces agents to make multiple calls to reconstruct a complete
  // reference list, breaking research flow. Bypass the env-var default.
  const responseCharLength = Number.MAX_SAFE_INTEGER;

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
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    }
  );
}
