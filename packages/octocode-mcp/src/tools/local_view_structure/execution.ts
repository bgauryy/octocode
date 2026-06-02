import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type ViewStructureQuery,
  ViewStructureQuerySchema,
} from '../../scheme/localSchemaOverlay.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { viewStructure } from './local_view_structure.js';
import { createErrorResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import type {
  EvidenceMetadata,
  ProcessedBulkResult,
} from '../../types/toolResults.js';
import {
  attachEvidence,
  buildEvidenceMetadata,
  hasMorePagination,
  incompleteHintReasons,
  isRecord,
  paginationTotal,
  records,
} from '../evidence.js';
import { isConcise } from '../../scheme/verbosity.js';

// Re-exported so every tool exposes `apply<Tool>Verbosity` from execution.ts.
export { applyViewStructureVerbosity } from './local_view_structure.js';

/**
 * @param concise when true the `entries` array was dropped by concise verbosity.
 *   `answerReady` is then derived from the entry count (so a non-empty tree is
 *   not mislabelled "No directory entries matched"), and display-pagination
 *   "has more" reasons are suppressed.
 */
export function buildViewStructureEvidence(
  result: unknown,
  concise: boolean
): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const entries = records(data.entries);
  const hasResults =
    entries.length > 0 || paginationTotal(data.pagination, 'totalEntries') > 0;
  const reasons: string[] = [];

  if (!concise && hasMorePagination(data.pagination)) {
    reasons.push('Entry pagination has more results.');
  }
  reasons.push(...incompleteHintReasons(data));

  return buildEvidenceMetadata({
    kind: 'structure',
    answerReady: hasResults,
    incompleteReasons: reasons,
    emptyReason: 'No directory entries matched the supplied view.',
  });
}

/**
 * Execute bulk view structure operation.
 * Wraps viewStructure with bulk operation handling for multiple queries.
 * Validates each query individually so one invalid query doesn't block the batch.
 */
export async function executeViewStructure(
  args: ToolExecutionArgs<ViewStructureQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries || [],
    async (query: ViewStructureQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        query,
        contextMessage: 'localViewStructure execution failed',
        execute: async () => {
          const validation = ViewStructureQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await viewStructure(validation.data);
          return attachEvidence(
            result as ProcessedBulkResult,
            buildViewStructureEvidence(
              result,
              isConcise(validation.data.verbosity)
            )
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
