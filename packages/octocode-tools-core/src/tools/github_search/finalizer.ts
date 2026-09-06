import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import { MAX_PAGE_NUMBER } from '../../config.js';
import { formatFinalizedResponse } from '../../utils/response/groupedFinalizer.js';
import { buildGhSearchCodeFinalizer } from '../github_search_code/finalizer/build.js';
import type { GitHubSearchQuery } from './scheme.js';

const GITHUB_SEARCH_RESULT_WINDOW = 1000;

const OPERATION_BY_INTERNAL_RUNNER = {
  'github.code': 'code',
  'github.repositories': 'repositories',
  'github.tree': 'tree',
} as const;

export function buildGitHubSearchFinalizer(): BulkFinalizer<GitHubSearchQuery> {
  return ({ queries, results }) => {
    const codeEntries = queries.flatMap((query, originalIndex) =>
      query.operation === 'code' ? [{ query, originalIndex }] : []
    );
    const codeIndexByOriginal = new Map(
      codeEntries.map((entry, codeIndex) => [entry.originalIndex, codeIndex])
    );
    const originalIndexByCode = codeEntries.map(entry => entry.originalIndex);
    const codeQueries = codeEntries.map(({ query }) => {
      const { operation: _operation, ...input } = query;
      return input;
    });
    const codeResults = results
      .filter(row => codeIndexByOriginal.has(row.index))
      .map(row => ({ ...row, index: codeIndexByOriginal.get(row.index)! }));
    const finalizedCodeRows = finalizeCodeRows(codeQueries, codeResults).map(
      row => ({ ...row, index: originalIndexByCode[row.index] ?? row.index })
    );
    const codeOriginalIndexes = new Set(originalIndexByCode);
    const otherRows = results.filter(
      row => !codeOriginalIndexes.has(row.index)
    );
    const mergedRows = [...finalizedCodeRows, ...otherRows]
      .map(row => addOperationAndNormalize(row, queries[row.index]))
      .sort((left, right) => left.index - right.index);
    const structuredContent = { results: mergedRows };

    return formatFinalizedResponse(
      structuredContent,
      [
        'results',
        'index',
        'status',
        'meta',
        'data',
        'operation',
        'files',
        'repositories',
        'structure',
        'pagination',
        'next',
        'tool',
        'query',
        'why',
        'confidence',
        'error',
      ],
      mergedRows.length > 0 && mergedRows.every(row => row.status === 'error')
    );
  };
}

function finalizeCodeRows(
  queries: Array<Record<string, unknown>>,
  results: FlatQueryResult[]
): FlatQueryResult[] {
  if (queries.length === 0) return [];
  const finalized = buildGhSearchCodeFinalizer<Record<string, unknown>>()({
    queries,
    results,
    config: { toolName: 'github.code' },
  });
  const output = finalized.structuredContent as {
    results?: FlatQueryResult[];
  };
  return output.results ?? [];
}

function addOperationAndNormalize(
  row: FlatQueryResult,
  query: GitHubSearchQuery | undefined
): FlatQueryResult {
  const operation = query?.operation;
  if (!operation || !query) return row;
  const normalized = normalizeContinuations({ operation, ...row.data });
  const capped = addProviderCapState(normalized);
  const paginated = addPageContinuation(capped, operation, query);
  return {
    ...row,
    data: addProviderPartialState(paginated, operation, query),
  };
}

function publicPageQuery(
  query: GitHubSearchQuery,
  page: number
): Record<string, unknown> {
  const { goal: _goal, reasoning: _reasoning, ...publicQuery } = query;
  return { ...publicQuery, page };
}

function addPageContinuation(
  data: Record<string, unknown>,
  operation: GitHubSearchQuery['operation'],
  query: GitHubSearchQuery | undefined
): Record<string, unknown> {
  if (!query) return data;
  const pagination = data.pagination;
  if (!pagination || typeof pagination !== 'object') return data;
  const page = pagination as Record<string, unknown>;
  if (page.hasMore !== true) return data;
  const { nextPage, ...pageWithoutNextPage } = page;
  const perPage =
    typeof page.perPage === 'number' && page.perPage > 0
      ? page.perPage
      : typeof query.pageSize === 'number' && query.pageSize > 0
        ? query.pageSize
        : undefined;
  if (
    page.totalMatchesCapped === true &&
    typeof nextPage === 'number' &&
    perPage !== undefined &&
    (nextPage - 1) * perPage >= GITHUB_SEARCH_RESULT_WINDOW
  ) {
    const existingNext =
      data.next && typeof data.next === 'object'
        ? (data.next as Record<string, unknown>)
        : undefined;
    const { nextPage: _staleNextPage, ...remainingNext } = existingNext ?? {};
    return {
      ...data,
      pagination: {
        ...pageWithoutNextPage,
        continuationUnavailable: {
          reason: 'providerResultCap',
          maxResults: GITHUB_SEARCH_RESULT_WINDOW,
        },
      },
      ...(Object.keys(remainingNext).length > 0 ? { next: remainingNext } : {}),
    };
  }
  if (typeof nextPage !== 'number' || !Number.isInteger(nextPage)) {
    return {
      ...data,
      terminalLimit: true,
      pagination: {
        ...pageWithoutNextPage,
        continuationUnavailable: {
          reason: 'missingProviderCursor',
        },
      },
    };
  }
  if (nextPage > MAX_PAGE_NUMBER) {
    return {
      ...data,
      terminalLimit: true,
      pagination: {
        ...pageWithoutNextPage,
        continuationUnavailable: {
          reason: 'schemaPageLimit',
          maxPage: MAX_PAGE_NUMBER,
        },
      },
    };
  }
  const next =
    data.next && typeof data.next === 'object'
      ? (data.next as Record<string, unknown>)
      : {};
  return {
    ...data,
    next: {
      ...next,
      nextPage: {
        tool: 'ghSearch',
        query: publicPageQuery(query, nextPage),
        why: `Continue ${operation} results on page ${nextPage}.`,
        confidence: 'exact',
      },
    },
  };
}

function addProviderCapState(
  data: Record<string, unknown>
): Record<string, unknown> {
  const pagination = data.pagination;
  if (!pagination || typeof pagination !== 'object') return data;
  if ((pagination as Record<string, unknown>).totalMatchesCapped !== true) {
    return data;
  }
  const existingReasons = Array.isArray(data.partialReasons)
    ? data.partialReasons.filter(
        (reason): reason is string => typeof reason === 'string'
      )
    : [];
  return {
    ...data,
    isPartial: true,
    terminalLimit: true,
    partialReasons: existingReasons.includes('providerResultCap')
      ? existingReasons
      : [...existingReasons, 'providerResultCap'],
    providerLimit: {
      reason: 'providerResultCap',
      maxResults: GITHUB_SEARCH_RESULT_WINDOW,
    },
  };
}

function addProviderPartialState(
  data: Record<string, unknown>,
  operation: GitHubSearchQuery['operation'],
  query: GitHubSearchQuery
): Record<string, unknown> {
  const incompleteResults = data.incompleteResults === true;
  const existingReasons = Array.isArray(data.partialReasons)
    ? data.partialReasons.filter(
        (reason): reason is string => typeof reason === 'string'
      )
    : [];
  const retryableTreeFailure = existingReasons.includes('partialTreeFailures');
  if (!incompleteResults && !retryableTreeFailure) return data;

  const partialReasons = [
    ...existingReasons,
    ...(incompleteResults &&
    !existingReasons.includes('providerIncompleteResults')
      ? ['providerIncompleteResults']
      : []),
  ];
  const next =
    data.next && typeof data.next === 'object'
      ? (data.next as Record<string, unknown>)
      : {};
  const currentPage =
    typeof query.page === 'number' && Number.isInteger(query.page)
      ? query.page
      : 1;

  return {
    ...data,
    isPartial: true,
    partialReasons,
    next: {
      ...next,
      ...(next.retry
        ? {}
        : {
            retry: {
              tool: 'ghSearch',
              query: publicPageQuery(query, currentPage),
              why: `Retry the same ${operation} provider page because the provider reported incomplete results.`,
              confidence: 'exact',
            },
          }),
    },
  };
}

function normalizeContinuations<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map(normalizeContinuations) as T;
  }
  if (!value || typeof value !== 'object') return value;

  const record = Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key,
      normalizeContinuations(item),
    ])
  ) as Record<string, unknown>;
  const operation =
    typeof record.tool === 'string'
      ? OPERATION_BY_INTERNAL_RUNNER[
          record.tool as keyof typeof OPERATION_BY_INTERNAL_RUNNER
        ]
      : undefined;
  if (operation) {
    record.tool = 'ghSearch';
    if (record.query && typeof record.query === 'object') {
      const query = { ...(record.query as Record<string, unknown>) };
      if (operation === 'tree') {
        if (query.itemsPerPage !== undefined)
          query.pageSize = query.itemsPerPage;
        delete query.itemsPerPage;
      } else if (operation === 'repositories') {
        if (query.limit !== undefined) query.pageSize = query.limit;
        delete query.limit;
        if (query.topicsToSearch !== undefined)
          query.topics = query.topicsToSearch;
        delete query.topicsToSearch;
      } else if (operation === 'code') {
        if (query.limit !== undefined) query.pageSize = query.limit;
        delete query.limit;
      }
      record.query = {
        operation,
        ...query,
      };
    }
  }
  return record as T;
}
