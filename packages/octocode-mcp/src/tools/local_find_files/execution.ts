import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { type FindFilesQuery, LocalFindFilesQuerySchema } from './scheme.js';
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
import { attachEvidence, buildCollectionEvidence } from '../evidence.js';

export { finalizeFindFilesResult } from './findFiles.js';

export function buildFindFilesEvidence(result: unknown): EvidenceMetadata {
  return buildCollectionEvidence({
    result,
    collectionField: 'files',
    totalKeys: ['totalFiles'],
    paginationMoreReason: 'File pagination has more results.',
    kind: 'metadata',
    emptyReason: 'No files matched the supplied metadata filters.',
  });
}

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
          const validation = LocalFindFilesQuerySchema.safeParse(query);
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
    },
    args
  );
}
