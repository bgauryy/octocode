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

// Verbosity shaping is defined alongside the result builder to avoid a circular
// import (execution → searchContentRipgrep → ripgrepExecutor → builder).
// Re-exported here so every tool exposes `apply<Tool>Verbosity` from execution.ts.
export { applyRipgrepVerbosity } from './ripgrepResultBuilder.js';

/**
 * Execute bulk ripgrep search operation.
 * Wraps searchContentRipgrep with bulk operation handling for multiple queries.
 * Validates each query individually so one invalid query doesn't block the batch.
 */
export async function executeRipgrepSearch(
  args: ToolExecutionArgs<RipgrepQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength, format } = args;

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
          return searchContentRipgrep(validation.data);
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      responseCharOffset,
      responseCharLength,

      format,
      peerHints: true,
    }
  );
}
