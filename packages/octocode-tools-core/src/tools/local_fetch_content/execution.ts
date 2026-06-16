import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { fetchContent } from './fetchContent.js';
import {
  LocalFetchContentQuerySchema,
  type FetchContentQuery,
} from './scheme.js';
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

export { finalizeFetchContentResult } from './fetchContent.js';

function buildFetchContentEvidence(result: unknown): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const hasContent =
    typeof data.content === 'string'
      ? data.content.length > 0
      : typeof data.totalLines === 'number';
  const reasons: string[] = [];

  const isMatchSlice =
    (Array.isArray(data.matchRanges) && data.matchRanges.length > 0) ||
    (Array.isArray(data.warnings) && data.warnings.length > 0);
  const isSkeleton = data.isSkeleton === true || data.contentView === 'symbols';
  if (data.isPartial === true && !isMatchSlice && !isSkeleton) {
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

export async function executeFetchContent(
  args: ToolExecutionArgs<FetchContentQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: FetchContentQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        query,
        contextMessage: 'localGetFileContent execution failed',
        execute: async () => {
          const validation = LocalFetchContentQuerySchema.safeParse(query);
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
      peerHints: true,
      peerEvidence: true,
    },
    args
  );
}
