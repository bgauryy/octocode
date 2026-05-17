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
          return findFiles(validation.data);
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
      responseCharOffset,
      responseCharLength,
    }
  );
}
