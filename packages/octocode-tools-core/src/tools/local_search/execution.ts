import type { CallToolResult } from '@modelcontextprotocol/server';
import { access } from 'node:fs/promises';
import { ToolErrors } from '../../errors/errorFactories.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import {
  LocalSearchQuerySchema,
  type LocalSearchQuery,
} from '@octocodeai/octocode-core/schema';
import { LOCAL_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import { runTypedLexicalSearch } from './typedLexicalService.js';

export async function executeLocalSearch(
  args: ToolExecutionArgs<LocalSearchQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    query =>
      executeWithToolBoundary({
        toolName: LOCAL_SEARCH_TOOL_NAME,
        query,
        contextMessage: 'localSearch execution failed',
        execute: async () => {
          const parsed = LocalSearchQuerySchema.safeParse(query);
          if (!parsed.success) throw parsed.error;
          try {
            await access(parsed.data.path);
          } catch (error) {
            throw ToolErrors.fileAccessFailed(
              parsed.data.path,
              error instanceof Error ? error : undefined
            );
          }
          return normalizeOperationContinuations(
            await runTypedLexicalSearch(parsed.data)
          );
        },
      }),
    { toolName: LOCAL_SEARCH_TOOL_NAME },
    args
  );
}

function normalizeOperationContinuations<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map(normalizeOperationContinuations) as T;
  }
  if (!value || typeof value !== 'object') return value;

  const record = Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== 'searchTime')
      .map(([key, item]) => [key, normalizeOperationContinuations(item)])
  ) as Record<string, unknown>;
  if (record.tool === 'local.text') {
    record.tool = LOCAL_SEARCH_TOOL_NAME;
    if (record.query && typeof record.query === 'object') {
      const query = { ...(record.query as Record<string, unknown>) };
      // Internal runners annotate continuations with execution-context fields.
      // They are not part of the public per-query contract and make a copied
      // continuation fail strict validation.
      delete query.goal;
      delete query.reasoning;
      query.resultView =
        typeof query.output === 'string' && query.output !== 'content'
          ? query.output
          : typeof query.mode === 'string' &&
              ['paginated', 'discovery', 'detailed'].includes(query.mode)
            ? query.mode
            : 'paginated';
      query.regex =
        query.regex === 'fixed'
          ? 'literal'
          : query.regex === 'perl'
            ? 'pcre2'
            : 'rust';
      delete query.mode;
      delete query.output;
      query.pageSize = query.itemsPerPage;
      query.reverse = query.sortReverse;
      delete query.itemsPerPage;
      delete query.sortReverse;
      record.query = query;
    }
  }
  return record as T;
}
