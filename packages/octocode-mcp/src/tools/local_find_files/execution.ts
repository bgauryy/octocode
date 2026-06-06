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

// Re-exported so every tool exposes `apply<Tool>Verbosity` from execution.ts.
export { applyFindFilesVerbosity } from './findFiles.js';

export function buildFindFilesEvidence(result: unknown): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const files = records(data.files);
  const hasResults =
    files.length > 0 || paginationTotal(data.pagination, 'totalFiles') > 0;
  const reasons: string[] = [];

  if (hasMorePagination(data.pagination)) {
    reasons.push('File pagination has more results.');
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
  const { queries } = args;

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
            buildFindFilesEvidence(result)
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
