/**
 * Tests for githubSearchPullRequests tool handler pagination fix
 * Verifies that pagination object and hints are correctly passed to the response
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createSuccessResult } from '../../src/tools/utils.js';

vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('githubSearchPullRequests Tool Handler - Pagination Fix', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('pagination hints generation', () => {
    it('should generate pagination hints when pagination data is present', () => {
      const query = {
        mainResearchGoal: 'Find PRs',
        researchGoal: 'Testing pagination',
        reasoning: 'Verify pagination hints',
      };

      const pagination = {
        currentPage: 1,
        totalPages: 10,
        perPage: 5,
        totalMatches: 50,
        hasMore: true,
      };

      const pullRequests = [{ number: 1 }, { number: 2 }];

      // Generate pagination hints (simulating what the tool handler does)
      const paginationHints: string[] = [];
      if (pagination) {
        const { currentPage, totalPages, totalMatches, hasMore } = pagination;
        paginationHints.push(
          `Page ${currentPage}/${totalPages} (showing ${pullRequests.length} of ${totalMatches} PRs)`
        );
        if (hasMore) {
          paginationHints.push(`Next: page=${currentPage + 1}`);
        }
        if (currentPage > 1) {
          paginationHints.push(`Previous: page=${currentPage - 1}`);
        }
        if (!hasMore) {
          paginationHints.push('Final page');
        }
        if (totalPages > 2) {
          paginationHints.push(
            `Jump to: page=1 (first) or page=${totalPages} (last)`
          );
        }
      }

      const result = createSuccessResult(
        query,
        {
          owner: 'facebook',
          repo: 'react',
          pull_requests: pullRequests,
          total_count: 50,
          pagination,
        },
        true,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints: paginationHints }
      );

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
      expect(result.pagination).toEqual(pagination);
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Page 1/10 (showing 2 of 50 PRs)');
      expect(result.hints).toContain('Next: page=2');
      expect(result.hints).toContain(
        'Jump to: page=1 (first) or page=10 (last)'
      );
    });

    it('should include "Previous" hint when not on first page', () => {
      const query = {
        researchGoal: 'Testing pagination',
        reasoning: 'Verify previous hint',
      };

      const pagination = {
        currentPage: 3,
        totalPages: 10,
        perPage: 5,
        totalMatches: 50,
        hasMore: true,
      };

      const pullRequests = [{ number: 1 }];

      const paginationHints: string[] = [];
      const { currentPage, totalPages, totalMatches, hasMore } = pagination;
      paginationHints.push(
        `Page ${currentPage}/${totalPages} (showing ${pullRequests.length} of ${totalMatches} PRs)`
      );
      if (hasMore) {
        paginationHints.push(`Next: page=${currentPage + 1}`);
      }
      if (currentPage > 1) {
        paginationHints.push(`Previous: page=${currentPage - 1}`);
      }

      const result = createSuccessResult(
        query,
        { pull_requests: pullRequests, pagination },
        true,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints: paginationHints }
      );

      expect(result.hints).toContain('Previous: page=2');
      expect(result.hints).toContain('Next: page=4');
    });

    it('should include "Final page" hint when on last page', () => {
      const query = {
        researchGoal: 'Testing pagination',
        reasoning: 'Verify final page hint',
      };

      const pagination = {
        currentPage: 10,
        totalPages: 10,
        perPage: 5,
        totalMatches: 50,
        hasMore: false,
      };

      const pullRequests = [{ number: 1 }];

      const paginationHints: string[] = [];
      const { currentPage, totalPages, totalMatches, hasMore } = pagination;
      paginationHints.push(
        `Page ${currentPage}/${totalPages} (showing ${pullRequests.length} of ${totalMatches} PRs)`
      );
      if (hasMore) {
        paginationHints.push(`Next: page=${currentPage + 1}`);
      }
      if (!hasMore) {
        paginationHints.push('Final page');
      }

      const result = createSuccessResult(
        query,
        { pull_requests: pullRequests, pagination },
        true,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints: paginationHints }
      );

      expect(result.hints).toContain('Final page');
      expect(result.hints).not.toContain('Next: page=11');
    });

    it('should not include jump hint when totalPages <= 2', () => {
      const query = {
        researchGoal: 'Testing pagination',
        reasoning: 'Verify no jump hint for small results',
      };

      const pagination = {
        currentPage: 1,
        totalPages: 2,
        perPage: 5,
        totalMatches: 10,
        hasMore: true,
      };

      const pullRequests = [{ number: 1 }];

      const paginationHints: string[] = [];
      const { currentPage, totalPages, totalMatches, hasMore } = pagination;
      paginationHints.push(
        `Page ${currentPage}/${totalPages} (showing ${pullRequests.length} of ${totalMatches} PRs)`
      );
      if (hasMore) {
        paginationHints.push(`Next: page=${currentPage + 1}`);
      }
      if (totalPages > 2) {
        paginationHints.push(
          `Jump to: page=1 (first) or page=${totalPages} (last)`
        );
      }

      const result = createSuccessResult(
        query,
        { pull_requests: pullRequests, pagination },
        true,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints: paginationHints }
      );

      // Verify no jump hint is present
      const hasJumpHint = result.hints?.some((h: string) =>
        h.includes('Jump to:')
      );
      expect(hasJumpHint).toBeFalsy();
    });

    it('should have no pagination hints when pagination is undefined', () => {
      const query = {
        researchGoal: 'Testing no pagination',
        reasoning: 'Verify empty hints',
      };

      const pullRequests = [{ number: 1 }];

      // No pagination hints when pagination is undefined
      const paginationHints: string[] = [];

      const result = createSuccessResult(
        query,
        { pull_requests: pullRequests, total_count: 1 },
        true,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints: paginationHints }
      );

      // Hints should be undefined when empty array is passed
      expect(result.hints).toBeUndefined();
    });
  });

  describe('pagination object in result', () => {
    it('should include pagination object in the result data', () => {
      const query = {
        researchGoal: 'Testing pagination object',
        reasoning: 'Verify pagination is included',
      };

      const pagination = {
        currentPage: 1,
        totalPages: 5,
        perPage: 10,
        totalMatches: 50,
        hasMore: true,
      };

      const result = createSuccessResult(
        query,
        {
          pull_requests: [],
          total_count: 50,
          pagination,
        },
        false,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints: [] }
      );

      expect(result.pagination).toBeDefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination?.totalPages).toBe(5);
      expect(result.pagination?.perPage).toBe(10);
      expect(result.pagination?.totalMatches).toBe(50);
      expect(result.pagination?.hasMore).toBe(true);
    });
  });
});
