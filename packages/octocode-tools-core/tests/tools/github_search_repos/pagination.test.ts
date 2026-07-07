import { describe, expect, it } from 'vitest';

import {
  buildResultPagination,
  buildMergedPagination,
  buildPartialFailureWarnings,
} from '../../../src/tools/github_search_repos/execution.js';

type MergeVariant = Parameters<typeof buildMergedPagination>[0][number];

function variantWithPagination(pagination: {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
}): MergeVariant {
  return {
    response: { data: { pagination } },
  } as unknown as MergeVariant;
}

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

describe('ghSearchRepos merged pagination', () => {
  it('reports the deduped count as a lower bound instead of summing overlapping variant totals', () => {
    // Two variants each report 40 matches, but after dedup only 55 distinct
    // repositories survive. Summing (80) would overcount the overlap.
    const merged = buildMergedPagination(
      [
        variantWithPagination({
          currentPage: 1,
          totalPages: 4,
          hasMore: true,
          entriesPerPage: 10,
          totalMatches: 40,
          reachableTotalMatches: 40,
          totalMatchesKind: 'reported',
        }),
        variantWithPagination({
          currentPage: 1,
          totalPages: 3,
          hasMore: false,
          entriesPerPage: 10,
          totalMatches: 40,
          reachableTotalMatches: 40,
          totalMatchesKind: 'reported',
        }),
      ],
      55
    );

    expect(merged).toBeDefined();
    expect(merged!.totalMatches).toBe(55);
    expect(merged!.reachableTotalMatches).toBe(55);
    expect(merged!.totalMatchesKind).toBe('lowerBound');
    // Structural fields still merge as before.
    expect(merged!.totalPages).toBe(4);
    expect(merged!.hasMore).toBe(true);
  });

  it('returns undefined when no variant carried pagination', () => {
    expect(
      buildMergedPagination(
        [{ response: { data: {} } } as unknown as MergeVariant],
        0
      )
    ).toBeUndefined();
  });
});

describe('ghSearchRepos partial-failure warnings', () => {
  it('names the failed variant(s) when some variants error', () => {
    const warnings = buildPartialFailureWarnings([
      { label: 'topics' },
      { label: 'keywords' },
    ]);
    expect(warnings).toHaveLength(1);
    expect(warnings![0]).toContain("'topics'");
    expect(warnings![0]).toContain("'keywords'");
    expect(warnings![0]).toMatch(/incomplete/i);
  });

  it('emits nothing when all variants succeeded', () => {
    expect(buildPartialFailureWarnings([])).toBeUndefined();
  });
});
