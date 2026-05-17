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
          return fetchContent(validation.data);
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
      responseCharOffset,
      responseCharLength,
    }
  );
}
