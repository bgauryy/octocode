import type { PaginationInfo } from '../../types/toolResults.js';

/**
 * Agent-facing count-honesty fields for search/PR pagination envelopes.
 * `totalMatches` (set on the base pagination) is the single count; here we add
 * only `totalMatchesCapped` — true when GitHub reported more than the reachable
 * 1000-result page window. The internal `reportedTotalMatches`/
 * `reachableTotalMatches`/`totalMatchesKind` drive that flag but are redundant
 * to emit (they duplicate `totalMatches` in the common uncapped case).
 */
export function countPaginationMetadata(
  pagination: PaginationInfo | undefined
): {
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
    ...(typeof capped === 'boolean' ? { totalMatchesCapped: capped } : {}),
    ...(typeof pagination?.uniqueFileCount === 'number'
      ? { uniqueFileCount: pagination.uniqueFileCount }
      : {}),
  };
}
