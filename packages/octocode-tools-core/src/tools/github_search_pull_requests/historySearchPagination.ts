import { MAX_PAGE_NUMBER } from '../../config.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import { GITHUB_SEARCH_HISTORY_TOOL_NAME } from '../toolNames.js';
import { publicSearchContinuationQuery } from './historyContinuations.js';
import {
  mergePartialReasons,
  withTruncatedCommitContinuation,
} from './historyPartialContinuations.js';

const GITHUB_SEARCH_RESULT_WINDOW = 1000;

export type SearchHistoryOperation = 'pullRequests' | 'issues' | 'commits';

export function withSearchPageContinuation(
  result: ProcessedBulkResult,
  query: Record<string, unknown>,
  operation: SearchHistoryOperation
): ProcessedBulkResult {
  const messageEnriched =
    operation === 'commits'
      ? withTruncatedCommitContinuation(result, query)
      : result;
  const providerIncomplete = messageEnriched.incompleteResults === true;
  const currentNext = asNextMap(messageEnriched.next);
  const currentPage =
    typeof query.page === 'number' && Number.isInteger(query.page)
      ? query.page
      : 1;
  const enrichedResult: ProcessedBulkResult = providerIncomplete
    ? {
        ...messageEnriched,
        isPartial: true,
        partialReasons: mergePartialReasons(
          messageEnriched.partialReasons,
          'providerIncompleteResults'
        ),
        next: {
          ...currentNext,
          retry: {
            tool: GITHUB_SEARCH_HISTORY_TOOL_NAME,
            query: publicSearchContinuationQuery(query, operation, currentPage),
            why: `Retry the same ${operation} provider page because the provider reported incomplete results.`,
            confidence: 'exact',
          },
        },
      }
    : messageEnriched;
  const pagination = enrichedResult.pagination;
  if (!pagination || typeof pagination !== 'object') return enrichedResult;

  const page = pagination as Record<string, unknown>;
  const providerCapped = page.totalMatchesCapped === true;
  const capped = providerCapped
    ? withProviderCap(enrichedResult)
    : enrichedResult;
  if (page.hasMore !== true) return capped;

  const nextPage = page.nextPage;
  if (typeof nextPage !== 'number' || !Number.isInteger(nextPage)) {
    return withTerminalPagination(capped, page, {
      reason: 'missingProviderCursor',
    });
  }
  if (nextPage > MAX_PAGE_NUMBER) {
    return withTerminalPagination(capped, page, {
      reason: 'schemaPageLimit',
      maxPage: MAX_PAGE_NUMBER,
    });
  }
  const perPage =
    typeof page.perPage === 'number' && page.perPage > 0
      ? page.perPage
      : typeof query.pageSize === 'number' && query.pageSize > 0
        ? query.pageSize
        : undefined;
  if (
    providerCapped &&
    perPage !== undefined &&
    (nextPage - 1) * perPage >= GITHUB_SEARCH_RESULT_WINDOW
  ) {
    return withTerminalPagination(capped, page, {
      reason: 'providerResultCap',
      maxResults: GITHUB_SEARCH_RESULT_WINDOW,
    });
  }
  return {
    ...capped,
    next: {
      ...asNextMap(capped.next),
      nextPage: {
        tool: GITHUB_SEARCH_HISTORY_TOOL_NAME,
        query: publicSearchContinuationQuery(query, operation, nextPage),
        why: `Continue ${operation} search/list results on page ${nextPage}.`,
        confidence: 'exact',
      },
    },
  };
}

function withProviderCap(result: ProcessedBulkResult): ProcessedBulkResult {
  return {
    ...result,
    isPartial: true,
    terminalLimit: true,
    partialReasons: mergePartialReasons(
      result.partialReasons,
      'providerResultCap'
    ),
    providerLimit: {
      reason: 'providerResultCap',
      maxResults: GITHUB_SEARCH_RESULT_WINDOW,
    },
  };
}

function withTerminalPagination(
  result: ProcessedBulkResult,
  page: Record<string, unknown>,
  continuationUnavailable: Record<string, unknown>
): ProcessedBulkResult {
  const { nextPage: _numericNextPage, ...terminalPagination } = page;
  const { nextPage: _executableNextPage, ...remainingNext } = asNextMap(
    result.next
  );
  return {
    ...result,
    terminalLimit: true,
    pagination: { ...terminalPagination, continuationUnavailable },
    ...(Object.keys(remainingNext).length > 0 ? { next: remainingNext } : {}),
  };
}

function asNextMap(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object'
    ? (value as Record<string, unknown>)
    : {};
}
