import type { CallToolResult } from '@modelcontextprotocol/server';
import { type FindFilesQuery, LocalFindFilesQuerySchema } from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { findFiles } from './findFiles.js';
import { safeParseOrError } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

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
        contextMessage: 'Local files operation failed',
        execute: async () => {
          const parsed = safeParseOrError<FindFilesQuery>(
            LocalFindFilesQuerySchema,
            query
          );
          if (parsed.ok === false) {
            return parsed.error;
          }
          const result = await findFiles(parsed.data);
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FIND_FILES,
    },
    args
  );
}
