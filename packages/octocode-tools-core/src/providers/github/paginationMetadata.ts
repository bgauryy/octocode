import type { PaginationInfo } from '../../types/toolResults.js';

/**
 * Agent-facing count-honesty fields for search/PR pagination envelopes.
 * `totalMatches` (set on the base pagination) is the count. Certainty is not
 * redundant when that count is only a lower bound, so expose that exceptional
 * kind along with the cap flag used for GitHub's 1000-result search window.
 */
export function countPaginationMetadata(
  pagination: PaginationInfo | undefined
): {
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
  uniqueFileCount?: number;
} {
  const capped =
    typeof pagination?.totalMatchesCapped === 'boolean'
      ? pagination.totalMatchesCapped
      : typeof pagination?.reportedTotalMatches === 'number' &&
          typeof pagination?.reachableTotalMatches === 'number'
        ? pagination.reportedTotalMatches > pagination.reachableTotalMatches
        : undefined;
  return {
    ...(pagination?.totalMatchesKind === 'lowerBound'
      ? { totalMatchesKind: 'lowerBound' as const }
      : {}),
    ...(typeof capped === 'boolean' ? { totalMatchesCapped: capped } : {}),
    ...(typeof pagination?.uniqueFileCount === 'number'
      ? { uniqueFileCount: pagination.uniqueFileCount }
      : {}),
  };
}
