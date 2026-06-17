import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type ViewStructureQuery,
  LocalViewStructureQuerySchema,
} from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { viewStructure } from './local_view_structure.js';
import { createErrorResult } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import type { LocalViewStructureToolResult } from '@octocodeai/octocode-core/extra-types';

export { finalizeViewStructureResult } from './local_view_structure.js';

export async function executeViewStructure(
  args: ToolExecutionArgs<ViewStructureQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: ViewStructureQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        query,
        contextMessage: 'localViewStructure execution failed',
        execute: async () => {
          const validation = LocalViewStructureQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await viewStructure(validation.data);
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      keysPriority: [
        'path',
        'summary',
        'pagination',
        'files',
        'folders',
        'entries',
      ] satisfies Array<keyof LocalViewStructureToolResult>,
      peerHints: true,
    },
    args
  );
}
