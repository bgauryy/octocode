import { describe, it, expect, beforeEach, vi } from 'vitest';
import { clearAllCache, getCacheStats } from '../../src/utils/http/cache.js';
import { getOctokit } from '../../src/github/client';
import { searchGitHubCodeAPI } from '../../src/github/codeSearch';
import { searchGitHubReposAPI } from '../../src/github/repoSearch';
import { searchGitHubPullRequestsAPI } from '../../src/github/pullRequestSearch';

vi.mock('../../src/github/client');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

const createMockOctokit = () => ({
  rest: {
    search: {
      code: vi.fn().mockResolvedValue({
        data: {
          total_count: 100,
          incomplete_results: false,
          items: [],
        },
        headers: {},
      }),
      repos: vi.fn().mockResolvedValue({
        data: {
          total_count: 100,
          incomplete_results: false,
          items: [],
        },
        headers: {},
      }),
      issuesAndPullRequests: vi.fn().mockResolvedValue({
        data: {
          total_count: 100,
          incomplete_results: false,
          items: [],
        },
      }),
    },
    pulls: {
      list: vi.fn().mockResolvedValue({ data: [] }),
      get: vi.fn().mockResolvedValue({ data: {} }),
      listFiles: vi.fn().mockResolvedValue({ data: [] }),
    },
    repos: {
      getContent: vi.fn().mockResolvedValue({ data: [] }),
      get: vi.fn().mockResolvedValue({ data: { default_branch: 'main' } }),
    },
  },
});

describe('GitHub API Caching - Pagination', () => {
  let mockOctokit: ReturnType<typeof createMockOctokit>;

  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    mockOctokit = createMockOctokit();
    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
    );
  });

  describe('Code Search pagination caching', () => {
    it('should cache page 1 and page 2 separately', async () => {
      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 1,
      });

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 2,
      });

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(2);

      const stats = getCacheStats();
      expect(stats.misses).toBe(2);
      expect(stats.sets).toBe(2);
    });

    it('should return cached result when same page is requested again', async () => {
      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 1,
      });

      const statsBefore = getCacheStats();

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 1,
      });

      const statsAfter = getCacheStats();

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(1);
      expect(statsAfter.hits).toBe(statsBefore.hits + 1);
    });
  });

  describe('Repository Search pagination caching', () => {
    it('should cache page 1 and page 2 separately', async () => {
      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
      });

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 2,
      });

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(2);

      expect(mockOctokit.rest.search.repos).toHaveBeenNthCalledWith(
        1,
        expect.objectContaining({ page: 1 })
      );
      expect(mockOctokit.rest.search.repos).toHaveBeenNthCalledWith(
        2,
        expect.objectContaining({ page: 2 })
      );
    });

    it('should cache same page with different context fields', async () => {
      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
        mainResearchGoal: 'Find repos',
        researchGoal: 'Goal 1',
        reasoning: 'Reason 1',
      });

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
        mainResearchGoal: 'Different goal',
        researchGoal: 'Different research',
        reasoning: 'Different reason',
      });

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(1);
    });
  });

  describe('Pull Request Search pagination caching', () => {
    it('should cache page 1 and page 2 separately', async () => {
      await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        limit: 5,
        page: 1,
      });

      await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        limit: 5,
        page: 2,
      });

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenCalledTimes(2);

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenNthCalledWith(1, expect.objectContaining({ page: 1 }));
      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenNthCalledWith(2, expect.objectContaining({ page: 2 }));
    });

    it('should return cached result when same page is requested again', async () => {
      await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        limit: 5,
        page: 1,
      });

      await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        limit: 5,
        page: 1,
      });

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenCalledTimes(1);
    });

    it('should cache same page with same params', async () => {
      await searchGitHubPullRequestsAPI({
        query: 'fix',
        limit: 5,
        page: 1,
      });

      await searchGitHubPullRequestsAPI({
        query: 'fix',
        limit: 5,
        page: 1,
      });

      expect(
        mockOctokit.rest.search.issuesAndPullRequests
      ).toHaveBeenCalledTimes(1);
    });
  });

  describe('Cache isolation with pagination', () => {
    it('should not share cache between different searches even with same page', async () => {
      await searchGitHubCodeAPI({
        keywordsToSearch: ['react'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 1,
      });

      await searchGitHubCodeAPI({
        keywordsToSearch: ['vue'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 1,
      });

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(2);
    });

    it('should not share cache between code and repo search', async () => {
      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        owner: 'facebook',
        repo: 'react',
        limit: 10,
        page: 1,
      });

      await searchGitHubReposAPI({
        keywordsToSearch: ['test'],
        owner: 'facebook',
        limit: 10,
        page: 1,
      });

      expect(mockOctokit.rest.search.code).toHaveBeenCalledTimes(1);
      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(1);
    });
  });

  describe('Cache stats accuracy', () => {
    it('should track cache hits and misses correctly', async () => {
      const initialStats = getCacheStats();

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        page: 1,
      });

      const afterFirstCall = getCacheStats();
      expect(afterFirstCall.misses).toBe(initialStats.misses + 1);
      expect(afterFirstCall.sets).toBe(initialStats.sets + 1);

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        page: 1,
      });

      const afterSecondCall = getCacheStats();
      expect(afterSecondCall.hits).toBe(afterFirstCall.hits + 1);
      expect(afterSecondCall.misses).toBe(afterFirstCall.misses);
      expect(afterSecondCall.sets).toBe(afterFirstCall.sets);
    });

    it('should only cache successful responses', async () => {
      mockOctokit.rest.search.repos.mockRejectedValueOnce(
        new Error('API Error')
      );

      try {
        await searchGitHubReposAPI({
          keywordsToSearch: ['react'],
          page: 1,
        });
      } catch {
        void 0;
      }

      mockOctokit.rest.search.repos.mockResolvedValueOnce({
        data: {
          total_count: 0,
          incomplete_results: false,
          items: [],
        },
        headers: {},
      });

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        page: 1,
      });

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledTimes(2);
    });
  });
});
