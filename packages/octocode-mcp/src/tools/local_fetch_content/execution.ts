import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { fetchContent } from './fetchContent.js';
import {
  FetchContentQuerySchema,
  type FetchContentQuery,
} from '../../scheme/localSchemaOverlay.js';
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
} from '../evidence.js';

// Verbosity shaping is defined alongside fetchContent (used internally before
// `createSuccessResult`). Re-exported here so every tool exposes
// `apply<Tool>Verbosity` from execution.ts.
export { applyFetchContentVerbosity } from './fetchContent.js';

function buildFetchContentEvidence(result: unknown): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const hasContent =
    typeof data.content === 'string'
      ? data.content.length > 0
      : typeof data.totalLines === 'number';
  const reasons: string[] = [];

  if (data.isPartial === true) {
    reasons.push('File content is partial.');
  }
  if (hasMorePagination(data.pagination)) {
    reasons.push('Character pagination has more data.');
  }
  reasons.push(...incompleteHintReasons(data));

  return buildEvidenceMetadata({
    kind: 'content',
    answerReady: hasContent,
    incompleteReasons: reasons,
    emptyReason: 'No file content was returned.',
  });
}

/**
 * Execute bulk fetch content operation.
 * Wraps fetchContent with bulk operation handling for multiple queries.
 * Validates each query individually so one invalid query doesn't block the batch.
 */
export async function executeFetchContent(
  args: ToolExecutionArgs<FetchContentQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries || [],
    async (query: FetchContentQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        query,
        contextMessage: 'localGetFileContent execution failed',
        execute: async () => {
          const validation = FetchContentQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await fetchContent(validation.data);
          return attachEvidence(
            result as ProcessedBulkResult,
            buildFetchContentEvidence(result)
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
