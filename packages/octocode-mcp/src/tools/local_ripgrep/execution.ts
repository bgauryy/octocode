import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type RipgrepQuery,
  RipgrepQuerySchema,
} from '../../scheme/localSchemaOverlay.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { searchContentRipgrep } from './searchContentRipgrep.js';
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

// Verbosity shaping is defined alongside the result builder to avoid a circular
// import (execution → searchContentRipgrep → ripgrepExecutor → builder).
// Re-exported here so every tool exposes `apply<Tool>Verbosity` from execution.ts.
export { applyRipgrepVerbosity } from './ripgrepResultBuilder.js';

/**
 * @param concise when true the `files` array was dropped by concise/discovery
 *   verbosity, so `answerReady` is derived from the match count and
 *   display-pagination "has more" reasons are suppressed.
 */
export function buildRipgrepEvidence(
  result: unknown,
  concise: boolean
): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const files = records(data.files);
  const filesWithMoreMatches = files.filter(file =>
    hasMorePagination(file.pagination)
  ).length;
  const hasResults =
    files.length > 0 || paginationTotal(data.pagination, 'totalFiles') > 0;
  const reasons: string[] = [];

  if (!concise) {
    if (hasMorePagination(data.pagination)) {
      reasons.push('File pagination has more results.');
    }
    if (filesWithMoreMatches > 0) {
      reasons.push(`${filesWithMoreMatches} file(s) have more matches.`);
    }
  }
  reasons.push(...incompleteHintReasons(data));

  return buildEvidenceMetadata({
    kind: 'code',
    answerReady: hasResults,
    incompleteReasons: reasons,
    emptyReason: 'No code matches were returned for the supplied pattern.',
  });
}

/**
 * Execute bulk ripgrep search operation.
 * Wraps searchContentRipgrep with bulk operation handling for multiple queries.
 * Validates each query individually so one invalid query doesn't block the batch.
 */
export async function executeRipgrepSearch(
  args: ToolExecutionArgs<RipgrepQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries || [],
    async (query: RipgrepQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        query,
        contextMessage: 'localSearchCode execution failed',
        execute: async () => {
          const validation = RipgrepQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await searchContentRipgrep(validation.data);
          return attachEvidence(
            result as ProcessedBulkResult,
            buildRipgrepEvidence(result, isConcise(validation.data.verbosity))
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
