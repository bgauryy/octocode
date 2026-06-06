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

export { applyRipgrepVerbosity } from './ripgrepResultBuilder.js';

export function buildRipgrepEvidence(result: unknown): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const files = records(data.files);
  const filesWithMoreMatches = files.filter(file =>
    hasMorePagination(file.pagination)
  ).length;
  const hasResults =
    files.length > 0 || paginationTotal(data.pagination, 'totalFiles') > 0;
  const reasons: string[] = [];

  if (hasMorePagination(data.pagination)) {
    reasons.push('File pagination has more results.');
  }
  if (filesWithMoreMatches > 0) {
    reasons.push(`${filesWithMoreMatches} file(s) have more matches.`);
  }
  reasons.push(...incompleteHintReasons(data));

  return buildEvidenceMetadata({
    kind: 'code',
    answerReady: hasResults,
    incompleteReasons: reasons,
    emptyReason: 'No code matches were returned for the supplied pattern.',
  });
}

export async function executeRipgrepSearch(
  args: ToolExecutionArgs<RipgrepQuery>
): Promise<CallToolResult> {
  const { queries } = args;

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
            buildRipgrepEvidence(result)
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
