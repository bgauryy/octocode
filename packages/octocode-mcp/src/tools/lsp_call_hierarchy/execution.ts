import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { LSPCallHierarchyQuery as UpstreamLSPCallHierarchyQuery } from '@octocodeai/octocode-core';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { processCallHierarchy } from './callHierarchy.js';
import type { CallHierarchyResult } from '../../lsp/types.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type LSPCallHierarchyQuery = WithOptionalMeta<UpstreamLSPCallHierarchyQuery> & {
  orderHint?: number;
};
import { TOOL_NAME } from './constants.js';
import { executeWithToolBoundary } from '../executionGuard.js';

/**
 * Execute bulk LSP call hierarchy operation.
 * Wraps processCallHierarchy with bulk operation handling for multiple queries.
 */
export async function executeCallHierarchy(
  args: ToolExecutionArgs<LSPCallHierarchyQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength, format } = args;

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
      responseCharOffset,
      responseCharLength,

      format,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    }
  );
}

/**
 * Attach cross-tool evidence so the bulk runner can lift it to the response
 * envelope. Confidence reflects whether the call graph came from the LSP
 * (semantic) or the text-pattern fallback; `complete` is false when output
 * was truncated by char-pagination.
 */
function attachCallHierarchyEvidence(
  result: CallHierarchyResult
): CallHierarchyResult {
  // Only annotate well-shaped LSP results. Skip early/raw error envelopes
  // (`{ isError, message }` or `{ error }`) — they don't carry call-hierarchy
  // status and the test contract for those shapes is strict.
  const status = (result as { status?: string }).status;
  if (status !== 'hasResults' && status !== 'empty') return result;
  const hasResults = status === 'hasResults';
  const mode = (result as { lspMode?: 'semantic' | 'fallback' }).lspMode;
  const outputPagination = (
    result as { outputPagination?: { hasMore?: boolean } }
  ).outputPagination;
  const evidence = {
    kind: 'calls' as const,
    answerReady: hasResults,
    complete: hasResults && !(outputPagination?.hasMore ?? false),
    confidence:
      mode === 'semantic'
        ? ('high' as const)
        : mode === 'fallback'
          ? ('low' as const)
          : undefined,
    ...(mode === 'fallback'
      ? {
          reason:
            'Call graph derived from text pattern matching; cross-file edges may be missed and naive identifier matches may produce false positives.',
        }
      : {}),
  };
  // Mutate in place so any non-enumerable raw-chars symbol attached
  // upstream (see attachRawResponseChars) survives.
  (result as Record<string, unknown>).evidence = evidence;
  return result;
}
