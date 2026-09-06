export function buildResultPagination(pagination: {
  currentPage: number;
  totalPages?: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
}) {
  return {
    currentPage: pagination.currentPage,
    ...(pagination.totalMatchesKind !== 'lowerBound' &&
    pagination.totalPages !== undefined
      ? { totalPages: pagination.totalPages }
      : {}),
    perPage: pagination.entriesPerPage ?? 10,
    ...(pagination.totalMatches !== undefined
      ? { totalMatches: pagination.totalMatches }
      : {}),
    ...(pagination.reportedTotalMatches !== undefined
      ? { reportedTotalMatches: pagination.reportedTotalMatches }
      : {}),
    ...(pagination.reachableTotalMatches !== undefined
      ? { reachableTotalMatches: pagination.reachableTotalMatches }
      : {}),
    ...(pagination.totalMatchesKind !== undefined
      ? { totalMatchesKind: pagination.totalMatchesKind }
      : {}),
    ...(pagination.totalMatchesCapped !== undefined
      ? { totalMatchesCapped: pagination.totalMatchesCapped }
      : {}),
    hasMore: pagination.hasMore,
    ...(pagination.hasMore ? { nextPage: pagination.currentPage + 1 } : {}),
  };
}
