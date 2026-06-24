import { describe, expect, it } from 'vitest';

import {
  buildSearchResult,
  type LocalSearchEngine,
} from '../../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import type { RipgrepQuery } from '../../../src/tools/local_ripgrep/scheme.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

// 5 matched files, each with a single match. maxFiles=2 must act as a per-page
// ceiling, NOT a lossy hard cap: every file must remain reachable by paging.
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

type Pagination = {
  currentPage: number;
  totalPages: number;
  filesPerPage: number;
  totalFiles: number;
  hasMore: boolean;
  nextPage?: number;
  totalFilesFound?: number;
};

type ResultShape = {
  files: Array<{ path: string }>;
  pagination: Pagination;
  next?: { nextPage?: { query?: Record<string, unknown> } };
};

const baseQuery = (page: number): RipgrepQuery =>
  ({
    keywords: 'match',
    sort: 'relevance',
    maxFiles: 2,
    page,
  }) as unknown as RipgrepQuery;

describe('localSearchCode maxFiles is a per-page ceiling (lossless)', () => {
  it('page 1 returns ≤ maxFiles, true totalFiles, hasMore, and a next page', async () => {
    const files = makeFiles(5);
    const result = (await buildSearchResult(
      files,
      baseQuery(1),
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    expect(result.files).toHaveLength(2);
    // True total of the full ranked set, NOT the per-page slice length.
    expect(result.pagination.totalFiles).toBe(5);
    expect(result.pagination.filesPerPage).toBe(2);
    expect(result.pagination.totalPages).toBe(3);
    expect(result.pagination.hasMore).toBe(true);
    expect(result.pagination.nextPage).toBe(2);
    expect(result.next?.nextPage?.query).toMatchObject({ page: 2 });
  });

  it('paging reaches the NEXT disjoint files; union across pages covers ALL found files', async () => {
    const files = makeFiles(5);

    const page1 = (await buildSearchResult(
      files,
      baseQuery(1),
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;
    const page2 = (await buildSearchResult(
      files,
      baseQuery(2),
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;
    const page3 = (await buildSearchResult(
      files,
      baseQuery(3),
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    const p1 = page1.files.map(f => f.path);
    const p2 = page2.files.map(f => f.path);
    const p3 = page3.files.map(f => f.path);

    // Disjoint pages.
    expect(p2).not.toEqual(p1);
    expect(new Set([...p1, ...p2, ...p3]).size).toBe(5);

    // Last page: no further page.
    expect(page3.files).toHaveLength(1);
    expect(page3.pagination.hasMore).toBe(false);
    expect(page3.pagination.nextPage).toBeUndefined();

    // Union reaches every found file — nothing silently dropped.
    const union = new Set([...p1, ...p2, ...p3]);
    for (const f of files) {
      expect(union.has(f.path)).toBe(true);
    }
  });

  it('without maxFiles, default per-page ceiling still paginates the full set', async () => {
    const files = makeFiles(25);
    const q = {
      keywords: 'match',
      sort: 'relevance',
      page: 1,
    } as unknown as RipgrepQuery;
    const result = (await buildSearchResult(
      files,
      q,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;
    // DEFAULT_FILES_PER_PAGE = 20.
    expect(result.files).toHaveLength(20);
    expect(result.pagination.totalFiles).toBe(25);
    expect(result.pagination.hasMore).toBe(true);
  });
});
