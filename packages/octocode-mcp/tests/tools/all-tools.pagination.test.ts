import { describe, it, expect } from 'vitest';
import { buildPaginationHints } from '../../../octocode-tools-core/src/tools/providerMappers.js';
import {
  generatePaginationHints,
  generateStructurePaginationHints,
} from '../../../octocode-tools-core/src/utils/pagination/hints.js';

describe('pagination cursor uniformity', () => {
  const buildPagination = (hasMore: boolean) =>
    buildPaginationHints(
      {
        currentPage: 2,
        totalPages: 5,
        hasMore,
        totalMatches: 50,
        perPage: 10,
      },
      'matches'
    );

  it('providerMappers.buildPaginationHints: cursor + enumeration hint on hasMore, [] on final', () => {
    expect(buildPagination(true).length).toBeGreaterThanOrEqual(1);
    expect(buildPagination(true)[0]).toMatch(/Page 2\/5.*page=3/);
    expect(buildPagination(true).some(h => h.includes('page through'))).toBe(
      true
    );
    expect(buildPagination(false)).toEqual([]);
  });

  it('generic generatePaginationHints: 1 line on hasMore, [] on final', () => {
    const meta = (hasMore: boolean) => ({
      paginatedContent: 'x',
      charOffset: 0,
      charLength: 10,
      totalChars: 30,
      byteOffset: 0,
      byteLength: 10,
      totalBytes: 30,
      hasMore,
      nextCharOffset: 10,
      currentPage: 1,
      totalPages: 3,
    });
    expect(generatePaginationHints(meta(true))).toHaveLength(1);
    expect(generatePaginationHints(meta(true))[0]).toMatch(
      /Page 1\/3.*charOffset=10/
    );
    expect(generatePaginationHints(meta(false))).toEqual([]);
  });

  it('Structure cursor uses page; final page silent', () => {
    expect(
      generateStructurePaginationHints(
        {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          entriesPerPage: 20,
          totalEntries: 55,
        },
        {
          owner: 'o',
          repo: 'r',
          branch: 'main',
          pageFiles: 1,
          pageFolders: 1,
          allFiles: 1,
          allFolders: 1,
        }
      )[0]
    ).toMatch(/Page 1\/3.*page=2/);
    expect(
      generateStructurePaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
          entriesPerPage: 20,
          totalEntries: 55,
        },
        {
          owner: 'o',
          repo: 'r',
          branch: 'main',
          pageFiles: 1,
          pageFolders: 1,
          allFiles: 1,
          allFolders: 1,
        }
      )
    ).toEqual([]);
  });
});
