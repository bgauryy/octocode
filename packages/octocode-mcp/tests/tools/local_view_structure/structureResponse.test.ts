/**
 * Unit tests for tools/local_view_structure/structureResponse.
 *
 * Covers all three buildWalkWarnings branches, the
 * with-preview/without-preview branches of buildEntryPaginationHints, and
 * pagination edge cases (page > totalPages clamps, empty entry list).
 */
import { describe, it, expect } from 'vitest';
import {
  summarizeEntries,
  paginateEntries,
  buildEntryPaginationHints,
  buildWalkWarnings,
} from '../../../src/tools/local_view_structure/structureResponse.js';
import type { DirectoryEntry } from '../../../src/tools/local_view_structure/structureFilters.js';
import type { WalkStats } from '../../../src/tools/local_view_structure/structureWalker.js';

function makeFile(name: string, size: string): DirectoryEntry {
  return { name, type: 'file', size };
}

function makeDir(name: string): DirectoryEntry {
  return { name, type: 'directory' };
}

describe('structureResponse.summarizeEntries', () => {
  it('counts files and directories and sums file sizes', () => {
    const entries: DirectoryEntry[] = [
      makeFile('a.ts', '1KB'),
      makeFile('b.ts', '2KB'),
      makeDir('src'),
      makeDir('docs'),
    ];
    const summary = summarizeEntries(entries);
    expect(summary).toContain('4 entries');
    expect(summary).toContain('2 files');
    expect(summary).toContain('2 dirs');
  });

  it('handles an empty entry list as 0/0/0', () => {
    const summary = summarizeEntries([]);
    expect(summary).toContain('0 entries');
    expect(summary).toContain('0 files');
    expect(summary).toContain('0 dirs');
  });

  it('treats files with missing size as zero bytes (no NaN propagation)', () => {
    const entries: DirectoryEntry[] = [
      { name: 'noSize.ts', type: 'file' },
      makeFile('a.ts', '1KB'),
    ];
    const summary = summarizeEntries(entries);
    expect(summary).not.toMatch(/NaN/);
    expect(summary).toContain('2 entries');
  });
});

describe('structureResponse.paginateEntries', () => {
  const entries: DirectoryEntry[] = Array.from({ length: 25 }, (_, i) =>
    makeFile(`f${String(i).padStart(2, '0')}.ts`, '1KB')
  );

  it('returns a single page when total fits in entriesPerPage', () => {
    const result = paginateEntries(entries.slice(0, 5), {
      itemsPerPage: 10,
    });
    expect(result.pagination.currentPage).toBe(1);
    expect(result.pagination.totalPages).toBe(1);
    expect(result.pagination.hasMore).toBe(false);
    expect(result.paginatedEntries).toHaveLength(5);
  });

  it('paginates and reports hasMore=true on the first page of many', () => {
    const result = paginateEntries(entries, {
      itemsPerPage: 10,
      page: 1,
    });
    expect(result.pagination.totalPages).toBe(3);
    expect(result.pagination.hasMore).toBe(true);
    expect(result.paginatedEntries).toHaveLength(10);
  });

  it('returns the partial last page with hasMore=false', () => {
    const result = paginateEntries(entries, {
      itemsPerPage: 10,
      page: 3,
    });
    expect(result.pagination.hasMore).toBe(false);
    expect(result.paginatedEntries).toHaveLength(5);
  });

  it('clamps page beyond totalPages to the last page', () => {
    const result = paginateEntries(entries, {
      itemsPerPage: 10,
      page: 99,
    });
    expect(result.pagination.currentPage).toBe(3);
    expect(result.paginatedEntries).toHaveLength(5);
  });

  it('uses sensible defaults when page/itemsPerPage are absent', () => {
    const result = paginateEntries(entries, {});
    expect(result.pagination.currentPage).toBe(1);
    expect(result.pagination.entriesPerPage).toBeGreaterThan(0);
  });
});

describe('structureResponse.buildEntryPaginationHints', () => {
  const allEntries: DirectoryEntry[] = Array.from({ length: 10 }, (_, i) =>
    makeFile(`f${i}.ts`, '1KB')
  );

  it('emits one cursor line (with starts-with preview) when more pages remain', () => {
    const hints = buildEntryPaginationHints(
      allEntries,
      5,
      {
        currentPage: 1,
        totalPages: 2,
        totalEntries: 10,
        hasMore: true,
      },
      5
    );
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('Page 1/2');
    expect(hints[0]).toContain('page=2');
    expect(hints[0]).toContain('starts with: f5');
  });

  it('omits "starts with" preview when the next-page window is empty', () => {
    const hints = buildEntryPaginationHints(
      allEntries,
      10,
      {
        currentPage: 1,
        totalPages: 2,
        totalEntries: 10,
        hasMore: true,
      },
      allEntries.length
    );
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('page=2');
    expect(hints[0]).not.toContain('starts with:');
  });

  it('emits no hint on the final page (no "Final page" tautology)', () => {
    const hints = buildEntryPaginationHints(
      allEntries,
      5,
      {
        currentPage: 2,
        totalPages: 2,
        totalEntries: 10,
        hasMore: false,
      },
      10
    );
    expect(hints).toEqual([]);
  });
});

describe('structureResponse.buildWalkWarnings', () => {
  const stats = (overrides: Partial<WalkStats> = {}): WalkStats => ({
    skipped: 0,
    permissionDenied: 0,
    ...overrides,
  });

  it('returns no warnings when nothing was skipped', () => {
    expect(buildWalkWarnings(stats())).toEqual([]);
  });

  it('reports combined permission+other counts when both are present', () => {
    const result = buildWalkWarnings(
      stats({ skipped: 5, permissionDenied: 2 })
    );
    expect(result).toHaveLength(1);
    expect(result[0]).toContain('5 entries skipped');
    expect(result[0]).toContain('2 permission denied');
    expect(result[0]).toContain('3 other errors');
  });

  it('reports permission-only message in singular form for exactly one entry', () => {
    const result = buildWalkWarnings(
      stats({ skipped: 1, permissionDenied: 1 })
    );
    expect(result).toEqual(['1 entry skipped due to permission denied']);
  });

  it('reports permission-only message in plural form for multiple entries', () => {
    const result = buildWalkWarnings(
      stats({ skipped: 3, permissionDenied: 3 })
    );
    expect(result).toEqual(['3 entries skipped due to permission denied']);
  });

  it('reports generic-access-error message when no permission errors occurred', () => {
    const result = buildWalkWarnings(
      stats({ skipped: 4, permissionDenied: 0 })
    );
    expect(result).toEqual(['4 entries skipped due to access errors']);
  });

  it('uses singular form for a single generic access error', () => {
    const result = buildWalkWarnings(
      stats({ skipped: 1, permissionDenied: 0 })
    );
    expect(result).toEqual(['1 entry skipped due to access errors']);
  });
});
