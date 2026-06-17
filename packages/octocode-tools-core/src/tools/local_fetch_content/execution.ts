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
export { finalizeFetchContentResult } from './fetchContent.js';

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
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
      peerHints: true,
    },
    args
  );
}
