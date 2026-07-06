import { describe, expect, it } from 'vitest';

import { buildResultPagination } from '../../../src/tools/github_search_repos/execution.js';

describe('ghSearchRepos pagination output', () => {
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
      totalPages: 4,
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
