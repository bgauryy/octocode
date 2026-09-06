import { describe, expect, it } from 'vitest';

import { buildResultPagination } from '../../../src/tools/github_search_repos/execution/pagination.js';
import { countPaginationMetadata } from '../../../src/providers/github/paginationMetadata.js';

describe('github.repositories pagination output', () => {
  it('preserves lower-bound certainty without claiming an exact totalPages', () => {
    expect(
      buildResultPagination({
        currentPage: 2,
        totalPages: 3,
        hasMore: true,
        entriesPerPage: 2,
        totalMatches: 5,
        reachableTotalMatches: 4,
        totalMatchesKind: 'lowerBound',
      })
    ).toEqual({
      currentPage: 2,
      perPage: 2,
      totalMatches: 5,
      reachableTotalMatches: 4,
      totalMatchesKind: 'lowerBound',
      hasMore: true,
      nextPage: 3,
    });
  });

  it('keeps exact and capped provider pagination behavior', () => {
    expect(
      buildResultPagination({
        currentPage: 2,
        totalPages: 10,
        hasMore: true,
        entriesPerPage: 100,
        totalMatches: 1000,
        reportedTotalMatches: 1200,
        reachableTotalMatches: 1000,
        totalMatchesKind: 'reported',
        totalMatchesCapped: true,
      })
    ).toMatchObject({
      totalPages: 10,
      totalMatchesKind: 'reported',
      totalMatchesCapped: true,
      nextPage: 3,
    });
  });

  it('keeps non-exact count metadata at the provider boundary', () => {
    expect(
      countPaginationMetadata({
        currentPage: 1,
        totalPages: 2,
        hasMore: true,
        totalMatches: 3,
        totalMatchesKind: 'lowerBound',
      })
    ).toEqual({ totalMatchesKind: 'lowerBound' });
    expect(
      countPaginationMetadata({
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        totalMatches: 1,
        totalMatchesKind: 'exact',
      })
    ).toEqual({});
  });

  it('preserves provider total metadata instead of collapsing to a lossy shape', () => {
    expect(
      buildResultPagination({
        currentPage: 2,
        totalPages: 4,
        hasMore: true,
        entriesPerPage: 25,
        totalMatches: 75,
        reportedTotalMatches: 1000,
        reachableTotalMatches: 100,
        totalMatchesKind: 'lowerBound',
        totalMatchesCapped: true,
      })
    ).toEqual({
      currentPage: 2,
      perPage: 25,
      totalMatches: 75,
      reportedTotalMatches: 1000,
      reachableTotalMatches: 100,
      totalMatchesKind: 'lowerBound',
      totalMatchesCapped: true,
      hasMore: true,
      nextPage: 3,
    });
  });

  it('does not manufacture totalMatches:0 when the provider omitted totals', () => {
    expect(
      buildResultPagination({
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
      })
    ).toEqual({
      currentPage: 1,
      totalPages: 1,
      perPage: 10,
      hasMore: false,
    });
  });
});
