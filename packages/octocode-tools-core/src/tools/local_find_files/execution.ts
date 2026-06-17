import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { type FindFilesQuery, LocalFindFilesQuerySchema } from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { findFiles } from './findFiles.js';
import { createErrorResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
export { finalizeFindFilesResult } from './findFiles.js';

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
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
      peerHints: true,
    },
    args
  );
}
