import { describe, expect, it } from 'vitest';
import {
  generatePaginationHints,
  generateStructurePaginationHints,
} from '../../src/utils/pagination/hints.js';

// ---------------------------------------------------------------------------
// generatePaginationHints
// ---------------------------------------------------------------------------

describe('generatePaginationHints', () => {
  it('returns empty array with no metadata fields', () => {
    const hints = generatePaginationHints({
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      charOffset: 0,
      charLength: 0,
    });
    expect(hints).toHaveLength(0);
  });

  it('includes navigation hint when hasMore and nextCharOffset are set', () => {
    const hints = generatePaginationHints({
      currentPage: 1,
      totalPages: 3,
      hasMore: true,
      nextCharOffset: 5000,
      charOffset: 0,
      charLength: 5000,
      totalChars: 15000,
    });
    expect(hints.some(h => h.includes('charOffset=5000'))).toBe(true);
  });

  it('omits navigation hint when hasMore is false', () => {
    const hints = generatePaginationHints({
      currentPage: 2,
      totalPages: 2,
      hasMore: false,
      charOffset: 5000,
      charLength: 5000,
    });
    expect(hints.every(h => !h.includes('charOffset'))).toBe(true);
  });

  it('omits navigation hint when nextCharOffset is undefined', () => {
    const hints = generatePaginationHints({
      currentPage: 1,
      totalPages: 2,
      hasMore: true,
      charOffset: 0,
      charLength: 1000,
    });
    expect(hints.every(h => !h.includes('charOffset'))).toBe(true);
  });

  it('adds token warning when estimatedTokens > 50000', () => {
    const hints = generatePaginationHints({
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      charOffset: 0,
      charLength: 0,
      estimatedTokens: 55000,
    });
    expect(hints.some(h => h.includes('exceeds typical context'))).toBe(true);
  });

  it('adds approaching-limit warning when estimatedTokens is 30001–50000', () => {
    const hints = generatePaginationHints({
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      charOffset: 0,
      charLength: 0,
      estimatedTokens: 35000,
    });
    expect(hints.some(h => h.includes('approaching context limit'))).toBe(true);
  });

  it('adds no token warning when estimatedTokens <= 30000', () => {
    const hints = generatePaginationHints({
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      charOffset: 0,
      charLength: 0,
      estimatedTokens: 25000,
    });
    expect(hints.every(h => !h.includes('tokens'))).toBe(true);
  });

  it('suppresses token warnings when enableWarnings is false', () => {
    const hints = generatePaginationHints(
      {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        charOffset: 0,
        charLength: 0,
        estimatedTokens: 60000,
      },
      { enableWarnings: false }
    );
    expect(hints.every(h => !h.includes('tokens'))).toBe(true);
  });

  it('prepends custom hints', () => {
    const hints = generatePaginationHints(
      {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        charOffset: 0,
        charLength: 0,
      },
      { customHints: ['Use minify to reduce size'] }
    );
    expect(hints).toContain('Use minify to reduce size');
  });
});

// ---------------------------------------------------------------------------
// generateStructurePaginationHints
// ---------------------------------------------------------------------------

const STUB_CTX = {
  owner: 'facebook',
  repo: 'react',
  branch: 'main',
  path: '',
  depth: 2,
  pageFiles: 50,
  pageFolders: 10,
  allFiles: 200,
  allFolders: 30,
};

describe('generateStructurePaginationHints', () => {
  it('returns empty array when hasMore is false', () => {
    const hints = generateStructurePaginationHints(
      { currentPage: 1, totalPages: 1, hasMore: false, entriesPerPage: 100, totalEntries: 5 },
      STUB_CTX
    );
    expect(hints).toHaveLength(0);
  });

  it('returns a next-page hint when hasMore is true', () => {
    const hints = generateStructurePaginationHints(
      {
        currentPage: 2,
        totalPages: 5,
        hasMore: true,
        nextPage: 3,
        entriesPerPage: 100,
        totalEntries: 500,
      },
      STUB_CTX
    );
    expect(hints.length).toBeGreaterThan(0);
    expect(hints[0]).toContain('page=3');
  });

  it('includes current/total page info', () => {
    const hints = generateStructurePaginationHints(
      {
        currentPage: 1,
        totalPages: 4,
        hasMore: true,
        nextPage: 2,
        entriesPerPage: 100,
        totalEntries: 400,
      },
      STUB_CTX
    );
    expect(hints[0]).toMatch(/1\/4/);
  });
});
