import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createSuccessResult } from '../../../octocode-tools-core/src/tools/utils.js';

describe('ghHistoryResearch Tool Handler - Pagination Fix', () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
        'GITHUB_SEARCH_PULL_REQUESTS'
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
