import type { CallToolResult } from '@modelcontextprotocol/server';
import { stat } from 'node:fs/promises';
import { ToolErrors } from '../../errors/errorFactories.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { validateToolPath } from '../../utils/file/toolHelpers.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { AST_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import { createGraphAnalysisRunner } from './topology/execution.js';
import { GraphAnalysisQuerySchema } from './topology/scheme.js';
import { findFiles } from './filesystem/files.js';
import { viewFilesystemTree } from './filesystem/tree.js';
import { searchContentStructural } from '../local_ripgrep/structuralSearch.js';
import { LocalRipgrepQuerySchema } from '@octocodeai/octocode-core/schema';
import {
  AstSearchQuerySchema,
  type AstSearchQuery,
} from '@octocodeai/octocode-core/schema';
import { normalizeAstContinuations } from './continuations.js';
import { inspectSyntax, inspectSymbols } from './inspect.js';

export async function executeAstSearch(
  args: ToolExecutionArgs<AstSearchQuery>
): Promise<CallToolResult> {
  const analyze = createGraphAnalysisRunner();
  return executeBulkOperation(
    args.queries || [],
    query =>
      executeWithToolBoundary({
        toolName: AST_SEARCH_TOOL_NAME,
        query,
        contextMessage: 'astSearch execution failed',
        execute: async () => {
          const validationResult = AstSearchQuerySchema.safeParse(query);
          if (!validationResult.success)
            return {
              status: 'error' as const,
              errorCode:
                query.operation === 'match'
                  ? 'structural.query.invalid'
                  : 'ast.query.invalid',
              error: validationResult.error.message,
            };
          const parsed = validationResult.data;
          if (parsed.operation === 'topology') {
            const { operation: _operation, analysis, ...scope } = parsed;
            const result = await analyze(
              GraphAnalysisQuerySchema.parse({ ...scope, operation: analysis })
            );
            return normalizeAstContinuations({
              ...result,
              operation: 'topology',
              analysis,
            });
          }
          const validation = validateToolPath(parsed, AST_SEARCH_TOOL_NAME);
          if (!validation.isValid) return validation.errorResult;
          const input = { ...parsed, path: validation.sanitizedPath };
          switch (input.operation) {
            case 'match': {
              const {
                operation: _operation,
                resultView,
                pageSize,
                reverse,
                ...scope
              } = input;
              if (
                !scope.langType &&
                !(
                  await stat(scope.path).catch((error: unknown) => {
                    throw ToolErrors.fileAccessFailed(
                      scope.path,
                      error instanceof Error ? error : undefined
                    );
                  })
                ).isFile()
              ) {
                return {
                  status: 'error' as const,
                  errorCode: 'ast.language.required',
                  error:
                    'Directory matching requires langType; choose the grammar from the source files.',
                };
              }
              return normalizeAstContinuations(
                await searchContentStructural(
                  LocalRipgrepQuerySchema.parse({
                    ...scope,
                    mode: 'structural',
                    output: resultView,
                    itemsPerPage: pageSize,
                    sortReverse: reverse,
                  })
                )
              );
            }
            case 'files':
              return findFiles(input);
            case 'tree':
              if (input.treeKind === 'syntax') return inspectSyntax(input);
              return viewFilesystemTree(input);
            case 'symbols':
              return inspectSymbols(input);
          }
        },
      }),
    { toolName: AST_SEARCH_TOOL_NAME },
    args
  );
}
