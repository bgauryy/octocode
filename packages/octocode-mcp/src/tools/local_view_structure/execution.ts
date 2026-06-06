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
import type {
  EvidenceMetadata,
  ProcessedBulkResult,
} from '../../types/toolResults.js';
import { attachEvidence, buildCollectionEvidence } from '../evidence.js';

export { applyViewStructureVerbosity } from './local_view_structure.js';

export function buildViewStructureEvidence(result: unknown): EvidenceMetadata {
  return buildCollectionEvidence({
    result,
    collectionField: 'entries',
    totalKeys: ['totalEntries'],
    paginationMoreReason: 'Entry pagination has more results.',
    kind: 'structure',
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
          const validation = ViewStructureQuerySchema.safeParse(query);
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
      peerHints: true,
      peerEvidence: true,
    }
  );
}
