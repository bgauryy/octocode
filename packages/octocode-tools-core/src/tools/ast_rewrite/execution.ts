import type { CallToolResult } from '@modelcontextprotocol/server';
import { getConfigSync } from '@octocodeai/config';
import {
  AST_REWRITE_TOOL_NAME,
  AstRewriteQuerySchema,
  type AstRewriteQuery,
} from '@octocodeai/octocode-core/schema';
import type { ToolExecutionArgs } from '../../types/execution.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import { validateToolPath } from '../../utils/file/toolHelpers.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { runAstRewrite } from './index.js';

export async function executeAstRewrite(
  args: ToolExecutionArgs<AstRewriteQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    query =>
      executeWithToolBoundary({
        toolName: AST_REWRITE_TOOL_NAME,
        query,
        contextMessage: 'astRewrite execution failed',
        execute: async () => {
          const parsed = AstRewriteQuerySchema.safeParse(query);
          if (!parsed.success) throw parsed.error;
          const validation = validateToolPath(
            parsed.data,
            AST_REWRITE_TOOL_NAME
          );
          if (!validation.isValid) return validation.errorResult;
          const result = await runAstRewrite(
            { ...parsed.data, path: validation.sanitizedPath },
            {
              allowApply: getConfigSync().local.enableAstRewriteApply,
            }
          );
          return { ...result } as ProcessedBulkResult;
        },
      }),
    { toolName: AST_REWRITE_TOOL_NAME },
    args
  );
}
