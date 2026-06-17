import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { type RipgrepQuery, LocalRipgrepQuerySchema } from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { searchContentRipgrep } from './searchContentRipgrep.js';
import { createErrorResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
export { finalizeRipgrepResult } from './ripgrepResultBuilder.js';

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
          const validation = LocalRipgrepQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await searchContentRipgrep(validation.data);
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      peerHints: true,
    },
    args
  );
}
