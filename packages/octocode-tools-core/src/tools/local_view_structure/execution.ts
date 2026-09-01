import type { CallToolResult } from '@modelcontextprotocol/server';
import {
  type ViewStructureQuery,
  LocalViewStructureQuerySchema,
} from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { viewStructure } from './local_view_structure.js';
import { safeParseOrError } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import type { LocalViewStructureToolResult } from '@octocodeai/octocode-core/extra-types';

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
        contextMessage: 'Local tree operation failed',
        execute: async () => {
          const parsed = safeParseOrError<ViewStructureQuery>(
            LocalViewStructureQuerySchema,
            query
          );
          if (parsed.ok === false) {
            return parsed.error;
          }
          const result = await viewStructure(parsed.data);
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
    },
    args
  );
}
