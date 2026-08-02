import { describe, expect, it } from 'vitest';

import {
  buildSearchResult,
  type LocalSearchEngine,
} from '../../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import type { RipgrepQuery } from '../../../src/tools/local_ripgrep/scheme.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

// One file with 22 matches — mirrors the benchmark repro (resolveDispatcher
// call sites in React: 22 total, default cap truncates page 1).
const makeFileWithMatches = (count: number): LocalSearchCodeFile[] => [
  {
    path: 'src/file.ts',
    matchCount: count,
    matches: Array.from({ length: count }, (_, i) => ({
      line: i + 1,
      column: 0,
      value: `match ${i}`,
    })),
  } as unknown as LocalSearchCodeFile,
];

type ResultShape = {
  files: Array<{
    path: string;
    matches?: unknown[];
    totalMatchRows?: number;
    returnedMatchRows?: number;
    pagination?: {
      currentPage: number;
      totalPages: number;
      hasMore: boolean;
      outOfRange?: boolean;
    };
  }>;
  warnings?: string[];
};

describe('localSearchCode matchPage + maxMatchesPerFile composition', () => {
  it('does not silently return 0 rows when matchPage no longer exists under a changed maxMatchesPerFile', async () => {
    const files = makeFileWithMatches(22);
    // matchPage:2 under maxMatchesPerFile:25 is out of range — all 22 matches
    // fit on page 1 under this cap, so there IS no page 2.
    const query = {
      keywords: 'match',
      sort: 'relevance',
      matchPage: 2,
      maxMatchesPerFile: 25,
    } as unknown as RipgrepQuery;

    const result = (await buildSearchResult(
      files,
      query,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    const file = result.files[0];
    // The silent-empty bug: matches ends up [] with nothing explaining why.
    // Once fixed, either matches is non-empty (page re-derived) or the
    // response explicitly says why it's empty instead of leaving a bare [].
    if ((file?.matches?.length ?? 0) === 0) {
      const explained =
        (result.warnings ?? []).some(w => w.includes('matchPage')) ||
        file?.pagination?.outOfRange === true;
      expect(explained).toBe(true);
    }
  });

  it('a valid matchPage under the same cap still returns the correct window', async () => {
    const files = makeFileWithMatches(22);
    const query = {
      keywords: 'match',
      sort: 'relevance',
      matchPage: 2,
      maxMatchesPerFile: 10,
    } as unknown as RipgrepQuery;

    const result = (await buildSearchResult(
      files,
      query,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    const file = result.files[0];
    expect(file?.matches).toHaveLength(10);
    expect(file?.pagination?.currentPage).toBe(2);
    expect(file?.pagination?.hasMore).toBe(true);
  });

  it('the last valid page under a cap returns its remainder, not an out-of-range flag', async () => {
    const files = makeFileWithMatches(22);
    const query = {
      keywords: 'match',
      sort: 'relevance',
      matchPage: 3,
      maxMatchesPerFile: 10,
    } as unknown as RipgrepQuery;

    const result = (await buildSearchResult(
      files,
      query,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    const file = result.files[0];
    expect(file?.matches).toHaveLength(2);
    expect(file?.pagination?.hasMore).toBe(false);
    expect(file?.pagination?.outOfRange).toBeFalsy();
  });
});
