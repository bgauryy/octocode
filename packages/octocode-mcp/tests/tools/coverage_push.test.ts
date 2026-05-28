/**
 * Targeted tests to push branch coverage from 91.97% to 92%+
 * Tests specific uncovered branches in multiple small files
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

import { hints as codeSearchHints } from '../../src/tools/github_search_code/hints.js';

describe('github_search_code hints - error branches', () => {
  it('should generate rate limit hint with retryAfter', () => {
    const result = codeSearchHints.error({
      isRateLimited: true,
      retryAfter: 30,
    });
    expect(result.some(h => h?.includes('Retry after 30s'))).toBe(true);
  });

  it('should generate rate limit hint without retryAfter', () => {
    const result = codeSearchHints.error({ isRateLimited: true });
    expect(result.some(h => h?.includes('Rate limited'))).toBe(true);
  });

  it('should generate 401 auth hint', () => {
    const result = codeSearchHints.error({ status: 401 });
    expect(result.some(h => h?.includes('GITHUB_TOKEN'))).toBe(true);
  });

  it('should generate 403 permission hint when not rate limited', () => {
    const result = codeSearchHints.error({
      status: 403,
      isRateLimited: false,
    });
    expect(result.some(h => h?.includes('repo'))).toBe(true);
  });

  it('should not generate 403 hint when rate limited', () => {
    const result = codeSearchHints.error({
      status: 403,
      isRateLimited: true,
    });
    expect(result.some(h => h?.includes('Permission denied'))).toBe(false);
  });
});

import { extractMatchingLines } from '../../src/tools/local_fetch_content/contentExtractor.js';

describe('contentExtractor - empty matches branch', () => {
  it('should return empty result when no matches found (line 61)', () => {
    const lines = ['line1', 'line2', 'line3'];
    // Search for a string that doesn't exist
    const result = extractMatchingLines(
      lines,
      'nonexistent_string_xyz',
      5, // contextLines
      false, // isRegex
      false // caseSensitive
    );
    expect(result.matchCount).toBe(0);
    expect(result.lines).toEqual([]);
    expect(result.matchRanges).toEqual([]);
  });
});

import { fetchDirectoryContentsRecursivelyAPI } from '../../src/github/repoStructureRecursive.js';

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
}));

import { getOctokit } from '../../src/github/client.js';

describe('repoStructureRecursive - error branches', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should return empty array when octokit getContent fails (line 22/73)', async () => {
    vi.mocked(getOctokit).mockResolvedValue({
      rest: {
        repos: {
          getContent: vi.fn().mockRejectedValue(new Error('API error')),
        },
      },
    } as any);

    // The function catches errors and returns empty array
    const mockOctokit = await getOctokit();
    const result = await fetchDirectoryContentsRecursivelyAPI(
      mockOctokit!,
      'owner',
      'repo',
      'main',
      'src',
      1,
      1,
      new Set()
    );
    expect(result).toEqual([]);
  });
});
