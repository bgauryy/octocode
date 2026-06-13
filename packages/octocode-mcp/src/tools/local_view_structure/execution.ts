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
import type {
  EvidenceMetadata,
  ProcessedBulkResult,
} from '../../types/toolResults.js';
import type { LocalViewStructureToolResult } from '@octocodeai/octocode-core/extra-types';
import {
  attachEvidence,
  buildEvidenceMetadata,
  hasMorePagination,
  paginationTotal,
} from '../evidence.js';

export { finalizeViewStructureResult } from './local_view_structure.js';

export function buildViewStructureEvidence(result: unknown): EvidenceMetadata {
  const data =
    typeof result === 'object' && result !== null
      ? (result as Record<string, unknown>)
      : {};
  // Lean mode emits string[] for files/folders; details mode emits object[]
  // for entries. records() filters to objects only, so it cannot be used
  // for string arrays. Check array length directly.
  const hasResults =
    (Array.isArray(data.files) && data.files.length > 0) ||
    (Array.isArray(data.folders) && data.folders.length > 0) ||
    (Array.isArray(data.entries) && data.entries.length > 0) ||
    paginationTotal(data.pagination, 'totalEntries') > 0;

  const reasons: string[] = [];
  if (hasMorePagination(data.pagination)) {
    reasons.push('Entry pagination has more results.');
  }

  return buildEvidenceMetadata({
    kind: 'structure',
    answerReady: hasResults,
    incompleteReasons: reasons,
    emptyReason: 'No directory entries matched the supplied view.',
  });
}

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
          return attachEvidence(
            result as ProcessedBulkResult,
            buildViewStructureEvidence(result)
          );
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
      // Hoist compact metadata before the file/folder lists so the agent
      // sees scope and pagination status before scanning names.
      keysPriority: [
        'path',
        'summary',
        'pagination',
        'files',
        'folders',
        'entries',
      ] satisfies Array<keyof LocalViewStructureToolResult>,
      peerHints: true,
      peerEvidence: true,
    },
    args
  );
}
