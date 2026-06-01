import { describe, it, expect } from 'vitest';
import { transformCodeSearchResult } from '../../../src/providers/github/githubSearch.js';
import { buildPaginationHints } from '../../../src/tools/providerMappers.js';
import type { OptimizedCodeSearchResult } from '@octocodeai/octocode-core/extra-types';

/**
 * Code search lost its real page size in the transform: unlike
 * transformRepoSearchResult, the code transform never carried the API's
 * `perPage` into `entriesPerPage`, so buildPaginationHints fell back to the
 * hardcoded `|| 10` and emitted "showing 1-10" regardless of the caller's
 * itemsPerPage. Repos was correct; code was not. This pins the symmetry.
 */
describe('transformCodeSearchResult — pagination page size', () => {
  const makeData = (perPage: number): OptimizedCodeSearchResult =>
    ({
      items: [
        {
          path: 'src/a.ts',
          matches: [{ context: 'x', positions: [] }],
          url: 'https://github.com/o/r/blob/main/src/a.ts',
          repository: {
            nameWithOwner: 'o/r',
            url: 'https://github.com/o/r',
          },
        },
      ],
      total_count: 179,
      pagination: {
        currentPage: 1,
        totalPages: 18,
        perPage,
        totalMatches: 179,
        hasMore: true,
      },
    }) as unknown as OptimizedCodeSearchResult;

  it('carries the real page size into entriesPerPage (not a hardcoded 10)', () => {
    const result = transformCodeSearchResult(makeData(4));
    expect(result.pagination.entriesPerPage).toBe(4);
    expect(result.pagination.totalMatches).toBe(179);
    expect(result.pagination.hasMore).toBe(true);
  });

  it('drives a truthful "showing X-Y" item-range hint (not the 10 default)', () => {
    const result = transformCodeSearchResult(makeData(4));
    const hints = buildPaginationHints(
      result.pagination as Parameters<typeof buildPaginationHints>[0],
      'matches'
    );
    // perPage=4 → first page shows items 1-4, not 1-10.
    expect(hints[0]).toContain('showing 1-4 of 179 matches');
  });
});
