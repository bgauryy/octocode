import { describe, expect, it } from 'vitest';

import {
  buildSearchResult,
  type LocalSearchEngine,
} from '../../../src/tools/local_ripgrep/ripgrepResultBuilder/buildResult.js';
import type { RipgrepQuery } from '../../../src/tools/local_ripgrep/scheme.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

const makeFiles = (count: number): LocalSearchCodeFile[] =>
  Array.from(
    { length: count },
    (_, i) =>
      ({
        path: `src/file${String(i).padStart(2, '0')}.ts`,
        matchCount: 1,
        matches: [{ line: 1, column: 0, value: `match ${i}` }],
      }) as unknown as LocalSearchCodeFile
  );

type ResultShape = {
  files: Array<{ path: string }>;
  pagination: {
    currentPage: number;
    totalPages: number;
    hasMore: boolean;
    outOfRange?: boolean;
  };
  warnings?: string[];
};

describe('local.text file-level page out of range', () => {
  it('does not silently return an empty file list with no signal when page exceeds totalPages', async () => {
    const files = makeFiles(4);
    const query = {
      keywords: 'match',
      sort: 'relevance',
      maxFiles: 2,
      page: 50,
    } as unknown as RipgrepQuery;

    const result = (await buildSearchResult(
      files,
      query,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    expect(result.pagination.totalPages).toBe(2);
    expect(result.files).toHaveLength(0);
    const explained =
      result.pagination.outOfRange === true ||
      (result.warnings ?? []).some(w => w.toLowerCase().includes('page'));
    expect(explained).toBe(true);
  });

  it('a valid page still returns its files with no out-of-range signal', async () => {
    const files = makeFiles(4);
    const query = {
      keywords: 'match',
      sort: 'relevance',
      maxFiles: 2,
      page: 2,
    } as unknown as RipgrepQuery;

    const result = (await buildSearchResult(
      files,
      query,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    expect(result.files).toHaveLength(2);
    expect(result.pagination.outOfRange).toBeFalsy();
  });
});
