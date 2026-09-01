import type { CallToolResult } from '@modelcontextprotocol/server';
import { access } from 'node:fs/promises';
import { ToolErrors } from '../../errors/errorFactories.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { findFiles } from '../local_find_files/findFiles.js';
import type { FindFilesQuery } from '../local_find_files/scheme.js';
import { searchContentRipgrep } from '../local_ripgrep/searchContentRipgrep.js';
import {
  LocalRipgrepQuerySchema,
  type RipgrepQuery,
} from '../local_ripgrep/scheme.js';
import { viewStructure } from '../local_view_structure/local_view_structure.js';
import type { ViewStructureQuery } from '../local_view_structure/scheme.js';
import {
  LocalSearchQuerySchema,
  type LocalTextResultView,
  type LocalSearchQuery,
  toLegacyTextQuery,
} from './scheme.js';
import { LOCAL_SEARCH_TOOL_NAME } from '../toolNames.js';
export { LOCAL_SEARCH_TOOL_NAME } from '../toolNames.js';

const OPERATION_BY_INTERNAL_RUNNER = {
  'local.text': 'text',
  'local.files': 'files',
  'local.tree': 'tree',
} as const;

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
            await runOperation(parsed.data)
          );
        },
      }),
    { toolName: LOCAL_SEARCH_TOOL_NAME },
    args
  );
}

async function runOperation(
  query: LocalSearchQuery
): Promise<ProcessedBulkResult> {
  switch (query.operation) {
    case 'text': {
      const { operation: _operation, resultView, ...input } = query;
      return searchContentRipgrep(
        LocalRipgrepQuerySchema.parse(
          toLegacyTextQuery(input, resultView as LocalTextResultView)
        ) as RipgrepQuery
      );
    }
    case 'structural': {
      const { operation: _operation, resultView, ...input } = query;
      return searchContentRipgrep(
        LocalRipgrepQuerySchema.parse({
          ...toLegacyTextQuery(input, resultView as LocalTextResultView),
          mode: 'structural',
          output: resultView,
        }) as RipgrepQuery
      );
    }
    case 'files': {
      const {
        operation: _operation,
        pathRegex,
        limit,
        sort,
        pageSize,
        ...input
      } = query;
      return findFiles({
        ...input,
        regex: pathRegex,
        limit,
        sortBy: sort,
        itemsPerPage: pageSize,
      } as FindFilesQuery);
    }
    case 'tree': {
      const {
        operation: _operation,
        namePattern,
        limit,
        sort,
        pageSize,
        ...input
      } = query;
      return viewStructure({
        ...input,
        pattern: namePattern,
        limit,
        sortBy: sort,
        itemsPerPage: pageSize,
      } as ViewStructureQuery);
    }
    default:
      throw new Error('Unsupported localSearch operation');
  }
}

function normalizeOperationContinuations<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map(normalizeOperationContinuations) as T;
  }
  if (!value || typeof value !== 'object') return value;

  const record = Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key,
      normalizeOperationContinuations(item),
    ])
  ) as Record<string, unknown>;
  let operation: 'text' | 'structural' | 'files' | 'tree' | undefined =
    typeof record.tool === 'string'
      ? OPERATION_BY_INTERNAL_RUNNER[
          record.tool as keyof typeof OPERATION_BY_INTERNAL_RUNNER
        ]
      : undefined;
  if (
    record.tool === 'local.text' &&
    record.query &&
    typeof record.query === 'object' &&
    (record.query as Record<string, unknown>).mode === 'structural'
  ) {
    operation = 'structural';
  }
  if (operation) {
    record.tool = LOCAL_SEARCH_TOOL_NAME;
    if (record.query && typeof record.query === 'object') {
      const query = { ...(record.query as Record<string, unknown>) };
      // Internal runners annotate continuations with execution-context fields.
      // They are not part of the public per-query contract and make a copied
      // continuation fail strict validation.
      delete query.goal;
      delete query.reasoning;
      if (operation === 'text') {
        query.resultView =
          typeof query.output === 'string' && query.output !== 'content'
            ? query.output
            : typeof query.mode === 'string' && query.mode !== 'structural'
              ? query.mode
              : 'paginated';
        delete query.mode;
        delete query.output;
      } else if (operation === 'structural') {
        query.resultView =
          typeof query.output === 'string' ? query.output : 'content';
        delete query.mode;
        delete query.output;
        // The legacy runner materializes text-search defaults even though AST
        // queries cannot accept them. Do not leak those internal defaults into
        // a public continuation that strict localSearch validation rejects.
        delete query.searchText;
        delete query.regex;
        delete query.caseMode;
        delete query.wholeWord;
        delete query.invertMatch;
        delete query.multiline;
        delete query.unique;
        delete query.matchWindow;
      }
      if (operation === 'text' || operation === 'structural') {
        query.pageSize = query.itemsPerPage;
        query.reverse = query.sortReverse;
        delete query.itemsPerPage;
        delete query.sortReverse;
      } else {
        query.pageSize = query.itemsPerPage;
        query.sort = query.sortBy;
        delete query.itemsPerPage;
        delete query.sortBy;
        if (operation === 'files') {
          query.pathRegex = query.regex;
          delete query.regex;
        } else {
          query.namePattern = query.pattern;
          delete query.pattern;
        }
      }
      record.query = { operation, ...query };
    }
  }
  return record as T;
}
