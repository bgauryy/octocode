import type {
  FindReferencesResult,
  LSPPaginationInfo,
  ReferenceLocation,
} from '../../lsp/types.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAME } from './constants.js';

interface ReferenceLike {
  uri: string;
}

interface ReferencePageArgs {
  locations: ReferenceLocation[];
  filteredReferences: ReferenceLike[];
  page: number;
  totalPages: number;
  totalReferences: number;
  referencesPerPage: number;
  hasFilters: boolean;
  totalUnfiltered: number;
}

export function buildFindReferencesPageOutOfRangeResult(
  filteredReferences: ReferenceLike[],
  page: number,
  totalPages: number,
  totalReferences: number,
  referencesPerPage: number
): FindReferencesResult {
  return {
    status: 'empty',
    pagination: buildReferencePagination(
      page,
      totalPages,
      totalReferences,
      referencesPerPage,
      false
    ),
    hasMultipleFiles: hasReferencesInMultipleFiles(filteredReferences),
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      `Requested page ${page} is outside available range (1-${totalPages}).`,
      `Use page=${totalPages} for the last available page.`,
    ],
  };
}

export function buildFindReferencesPageResult({
  locations,
  filteredReferences,
  page,
  totalPages,
  totalReferences,
  referencesPerPage,
  hasFilters,
  totalUnfiltered,
}: ReferencePageArgs): FindReferencesResult {
  const pagination = buildReferencePagination(
    page,
    totalPages,
    totalReferences,
    referencesPerPage,
    page < totalPages
  );

  return {
    locations,
    pagination,
    hasMultipleFiles: hasReferencesInMultipleFiles(filteredReferences),
    hints: buildReferencePageHints({
      pagination,
      page,
      totalPages,
      totalReferences,
      hasFilters,
      totalUnfiltered,
    }),
  };
}

function buildReferencePagination(
  page: number,
  totalPages: number,
  totalReferences: number,
  referencesPerPage: number,
  hasMore: boolean
): LSPPaginationInfo {
  return {
    currentPage: page,
    totalPages,
    totalResults: totalReferences,
    hasMore,
    ...(referencesPerPage < Number.MAX_SAFE_INTEGER
      ? { resultsPerPage: referencesPerPage }
      : {}),
  };
}

function buildReferencePageHints({
  pagination,
  page,
  totalPages,
  totalReferences,
  hasFilters,
  totalUnfiltered,
}: {
  pagination: LSPPaginationInfo;
  page: number;
  totalPages: number;
  totalReferences: number;
  hasFilters: boolean;
  totalUnfiltered: number;
}): string[] {
  const hints: string[] = [];

  if (hasFilters && totalUnfiltered !== totalReferences) {
    hints.push(
      `Filtered: ${totalReferences} of ${totalUnfiltered} total references match patterns.`
    );
  }

  if (pagination.hasMore) {
    hints.push(
      `Showing page ${page} of ${totalPages}. Use page=${page + 1} for more.`
    );
  }

  return hints;
}

function hasReferencesInMultipleFiles(references: ReferenceLike[]): boolean {
  return new Set(references.map(ref => ref.uri)).size > 1;
}
