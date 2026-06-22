import type { Pagination } from '../types.js';

export interface ToolPaginationPayload {
  currentPage?: number;
  totalPages?: number;
  nextPage?: number;
  hasMore?: boolean;
  itemsPerPage?: number;
  entriesPerPage?: number;
  filesPerPage?: number;
  perPage?: number;
  totalItems?: number;
  totalEntries?: number;
  totalFiles?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: string;
  totalMatchesCapped?: boolean;
  uniqueFileCount?: number;
}

export function toOqlPagination(
  pagination: ToolPaginationPayload | undefined,
  fallbackHasMore = false
): Pagination | undefined {
  if (!pagination) {
    return fallbackHasMore ? { hasMore: true } : undefined;
  }

  // Resolve the OQL-normalized unit fields up front. The backing tools report
  // page size and total under several aliases (entries/files/matches); collapse
  // them to one `itemsPerPage`/`totalItems` pair so page math is consistent.
  const itemsPerPage =
    pagination.itemsPerPage ??
    pagination.entriesPerPage ??
    pagination.filesPerPage ??
    pagination.perPage;
  const totalItems =
    pagination.totalItems ??
    pagination.totalEntries ??
    pagination.totalFiles ??
    pagination.totalMatches;
  const capped = pagination.totalMatchesCapped === true;

  // When both the unit fields are concrete numbers and the total is NOT a
  // capped estimate, the page count is derivable — recompute it instead of
  // trusting the upstream `totalPages`, which is often expressed in a different
  // unit (P2: e.g. files vs. per-match rows). When capped, the true total is
  // unknown, so we must not assert an exact `totalPages` (omit it) and keep the
  // upstream `hasMore` honest.
  const canDerive =
    !capped &&
    typeof totalItems === 'number' &&
    Number.isFinite(totalItems) &&
    typeof itemsPerPage === 'number' &&
    Number.isFinite(itemsPerPage) &&
    itemsPerPage > 0;
  const derivedTotalPages = canDerive
    ? Math.max(1, Math.ceil(totalItems! / itemsPerPage!))
    : undefined;

  const resolvedTotalPages = canDerive
    ? derivedTotalPages
    : capped
      ? undefined
      : pagination.totalPages;

  const hasMore = capped
    ? // Capped total: trust the upstream signal (more is reachable).
      (pagination.hasMore ??
      (typeof pagination.currentPage === 'number' &&
      typeof pagination.totalPages === 'number'
        ? pagination.currentPage < pagination.totalPages
        : fallbackHasMore))
    : typeof pagination.currentPage === 'number' &&
        typeof resolvedTotalPages === 'number'
      ? pagination.currentPage < resolvedTotalPages
      : (pagination.hasMore ?? fallbackHasMore);

  // Only surface a `nextPage` while there is a next page (consistency with the
  // recomputed `hasMore`); drop a stale upstream pointer once exhausted.
  const nextPage =
    pagination.nextPage !== undefined && hasMore
      ? pagination.nextPage
      : undefined;

  return {
    hasMore: Boolean(hasMore),
    ...(pagination.currentPage !== undefined
      ? { currentPage: pagination.currentPage }
      : {}),
    ...(resolvedTotalPages !== undefined
      ? { totalPages: resolvedTotalPages }
      : {}),
    ...(nextPage !== undefined ? { nextPage } : {}),
    ...(itemsPerPage !== undefined ? { itemsPerPage } : {}),
    ...(totalItems !== undefined ? { totalItems } : {}),
    ...(pagination.reportedTotalMatches !== undefined
      ? { reportedTotalItems: pagination.reportedTotalMatches }
      : {}),
    ...(pagination.reachableTotalMatches !== undefined
      ? { reachableTotalItems: pagination.reachableTotalMatches }
      : {}),
    ...(pagination.totalMatchesKind !== undefined
      ? { totalItemsKind: pagination.totalMatchesKind }
      : {}),
    ...(pagination.totalMatchesCapped !== undefined
      ? { totalItemsCapped: pagination.totalMatchesCapped }
      : {}),
    ...(pagination.uniqueFileCount !== undefined
      ? { uniqueFileCount: pagination.uniqueFileCount }
      : {}),
  };
}
