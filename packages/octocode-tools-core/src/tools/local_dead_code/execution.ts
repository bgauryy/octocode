import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type FindDeadCodeQuery,
  LocalFindDeadCodeQuerySchema,
} from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { findDeadCode } from './local_dead_code.js';
import { safeParseOrError } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

export async function executeFindDeadCode(
  args: ToolExecutionArgs<FindDeadCodeQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: FindDeadCodeQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
        query,
        contextMessage: 'localFindDeadCode execution failed',
        execute: async () => {
          const parsed = safeParseOrError<FindDeadCodeQuery>(
            LocalFindDeadCodeQuerySchema,
            query
          );
          if (parsed.ok === false) {
            return parsed.error;
          }
          const result = await findDeadCode(parsed.data);
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
    },
    args
  );
}
