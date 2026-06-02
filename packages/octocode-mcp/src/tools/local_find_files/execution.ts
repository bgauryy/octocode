import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type FindFilesQuery,
  FindFilesQuerySchema,
} from '../../scheme/localSchemaOverlay.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { findFiles } from './findFiles.js';
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
export { applyFindFilesVerbosity } from './findFiles.js';

/**
 * @param concise when true the display `files` array was dropped by concise
 *   verbosity, so `answerReady` is derived from the count and display-pagination
 *   "has more" reasons are suppressed (the count IS the complete probe answer).
 */
export function buildFindFilesEvidence(
  result: unknown,
  concise: boolean
): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const files = records(data.files);
  const hasResults =
    files.length > 0 || paginationTotal(data.pagination, 'totalFiles') > 0;
  const reasons: string[] = [];

  if (!concise) {
    if (hasMorePagination(data.pagination)) {
      reasons.push('File pagination has more results.');
    }
    // Check outputPagination first (canonical); fall back to charPagination
    // (the upstream type name set by findFiles.ts before shim promotion).
    if (hasMorePagination(data.outputPagination ?? data.charPagination)) {
      reasons.push('Character pagination has more data.');
    }
  }
  reasons.push(...incompleteHintReasons(data));

  return buildEvidenceMetadata({
    kind: 'metadata',
    answerReady: hasResults,
    incompleteReasons: reasons,
    emptyReason: 'No files matched the supplied metadata filters.',
  });
}

/**
 * Execute bulk find files operation.
 * Wraps findFiles with bulk operation handling for multiple queries.
 * Validates each query individually so one invalid query doesn't block the batch.
 */
export async function executeFindFiles(
  args: ToolExecutionArgs<FindFilesQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries || [],
    async (query: FindFilesQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_FIND_FILES,
        query,
        contextMessage: 'localFindFiles execution failed',
        execute: async () => {
          const validation = FindFilesQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await findFiles(validation.data);
          return attachEvidence(
            result as ProcessedBulkResult,
            buildFindFilesEvidence(result, isConcise(validation.data.verbosity))
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
