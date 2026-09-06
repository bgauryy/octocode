import type { CallToolResult } from '@modelcontextprotocol/server';
import { type RipgrepQuery, LocalRipgrepQuerySchema } from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { searchContentRipgrep } from './searchContentRipgrep.js';
import { safeParseOrError } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

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
        contextMessage: 'Local text operation failed',
        execute: async () => {
          const parsed = safeParseOrError<RipgrepQuery>(
            LocalRipgrepQuerySchema,
            query
          );
          if (parsed.ok === false) {
            return parsed.error;
          }
          const result = await searchContentRipgrep(parsed.data);
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
    },
    args
  );
}
