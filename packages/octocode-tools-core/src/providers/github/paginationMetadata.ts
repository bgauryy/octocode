import type { PaginationInfo } from '../../types/toolResults.js';

/**
 * Shared count-honesty fields from search/PR pagination envelopes.
 * Single source so githubSearch / githubPullRequests / mappers stay aligned.
 */
export function countPaginationMetadata(
  pagination: PaginationInfo | undefined
): {
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: PaginationInfo['totalMatchesKind'];
  totalMatchesCapped?: boolean;
  uniqueFileCount?: number;
} {
  return {
    ...(typeof pagination?.reportedTotalMatches === 'number'
      ? { reportedTotalMatches: pagination.reportedTotalMatches }
      : {}),
    ...(typeof pagination?.reachableTotalMatches === 'number'
      ? { reachableTotalMatches: pagination.reachableTotalMatches }
      : {}),
    ...(pagination?.totalMatchesKind
      ? { totalMatchesKind: pagination.totalMatchesKind }
      : {}),
    ...(typeof pagination?.totalMatchesCapped === 'boolean'
      ? { totalMatchesCapped: pagination.totalMatchesCapped }
      : {}),
    ...(typeof pagination?.uniqueFileCount === 'number'
      ? { uniqueFileCount: pagination.uniqueFileCount }
      : {}),
  };
}
