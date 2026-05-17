import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type ViewStructureQuery,
  ViewStructureQuerySchema,
} from '../../scheme/localSchemaOverlay.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { viewStructure } from './local_view_structure.js';
import { createErrorResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

/**
 * Execute bulk view structure operation.
 * Wraps viewStructure with bulk operation handling for multiple queries.
 * Validates each query individually so one invalid query doesn't block the batch.
 */
export async function executeViewStructure(
  args: ToolExecutionArgs<ViewStructureQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries || [],
    async (query: ViewStructureQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        query,
        contextMessage: 'localViewStructure execution failed',
        execute: async () => {
          const validation = ViewStructureQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          return viewStructure(validation.data);
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      responseCharOffset,
      responseCharLength,
    }
  );
}
