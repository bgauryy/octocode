import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  type BinaryInspectQuery,
  LocalBinaryInspectQuerySchema,
} from './scheme.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { createErrorResult } from '../utils.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { inspectBinary } from './binaryInspector.js';

export async function executeInspectBinary(
  args: ToolExecutionArgs<BinaryInspectQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: BinaryInspectQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_BINARY_INSPECT,
        query,
        contextMessage: 'localBinaryInspect execution failed',
        execute: async () => {
          const validation = LocalBinaryInspectQuerySchema.safeParse(query);
          if (!validation.success) {
            const messages = validation.error.issues
              .map(i => i.message)
              .join('; ');
            return createErrorResult(`Validation error: ${messages}`, query);
          }
          const result = await inspectBinary(validation.data);
          const rawResult = result as unknown as Record<string, unknown>;
          const rawSize =
            typeof rawResult.content === 'string'
              ? rawResult.content.length
              : typeof rawResult.strings === 'object' &&
                  rawResult.strings !== null
                ? JSON.stringify(rawResult.strings).length
                : 0;
          attachRawResponseChars(result as ProcessedBulkResult, rawSize);
          return result as unknown as ProcessedBulkResult;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_BINARY_INSPECT,
      peerHints: true,
    },
    args
  );
}
