import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubPullRequestsAPI } from '../../../octocode-tools-core/src/github/pullRequestSearch.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('Pull Request Search - Pagination', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  const createMockOctokit = (mocks: {
    issuesAndPullRequests?: ReturnType<typeof vi.fn>;
    pullsList?: ReturnType<typeof vi.fn>;
    pullsGet?: ReturnType<typeof vi.fn>;
    pullsListFiles?: ReturnType<typeof vi.fn>;
  }) => ({
    rest: {
      search: {
        issuesAndPullRequests:
          mocks.issuesAndPullRequests ||
          vi.fn().mockResolvedValue({ data: { items: [], total_count: 0 } }),
      },
      pulls: {
        list: mocks.pullsList || vi.fn().mockResolvedValue({ data: [] }),
        get: mocks.pullsGet || vi.fn().mockResolvedValue({ data: {} }),
        listFiles:
          mocks.pullsListFiles || vi.fn().mockResolvedValue({ data: [] }),
      },
    },
  });

  const createMockSearchResponse = (totalCount: number, itemCount: number) => ({
    data: {
      total_count: totalCount,
      incomplete_results: false,
      items: Array.from({ length: itemCount }, (_, i) => ({
        number: i + 1,
        title: `PR ${i + 1}`,
        state: 'open',
        user: { login: `user${i}` },
        html_url: `https://github.com/owner/repo/pull/${i + 1}`,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
        labels: [],
        pull_request: {
          url: `https://api.github.com/repos/owner/repo/pulls/${i + 1}`,
        },
      })),
    },
  });

  const createMockRESTResponse = (itemCount: number) => ({
    data: Array.from({ length: itemCount }, (_, i) => ({
      number: i + 1,
      title: `PR ${i + 1}`,
      state: 'open',
      user: { login: `user${i}` },
      html_url: `https://github.com/owner/repo/pull/${i + 1}`,
      created_at: '2024-01-01T00:00:00Z',
      updated_at: '2024-01-01T00:00:00Z',
      labels: [],
      head: { ref: 'feature', sha: 'abc123' },
      base: { ref: 'main', sha: 'def456' },
    })),
  });

  describe('GitHub Search API pagination', () => {
    describe('page parameter', () => {
      it('should pass page parameter to GitHub API (default page=1)', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(50, 5));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 5,
        });

        expect(searchMock).toHaveBeenCalledWith(
          expect.objectContaining({
            page: 1,
            per_page: 5,
          })
        );
      });

      it('should pass custom page parameter to GitHub API', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(50, 5));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 5,
          page: 3,
        });

        expect(searchMock).toHaveBeenCalledWith(
          expect.objectContaining({
            page: 3,
            per_page: 5,
          })
        );
      });
    });

    describe('pagination metadata in response', () => {
      it('should include pagination info with currentPage', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(100, 5));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const result = await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 5,
          page: 2,
        });

        expect(result.pagination).toBeDefined();
        expect(result.pagination?.currentPage).toBe(2);
      });

      it('should calculate totalPages correctly', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(95, 10));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const result = await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 10,
          page: 1,
        });

        expect(result.pagination?.totalPages).toBe(10);
        expect(result.pagination?.totalMatches).toBe(95);
        expect(result.pagination?.reportedTotalMatches).toBe(95);
        expect(result.pagination?.reachableTotalMatches).toBe(95);
        expect(result.pagination?.totalMatchesKind).toBe('reported');
      });

      it('should cap totalPages at 10 (GitHub 1000 result limit)', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(5000, 10));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const result = await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 100,
          page: 1,
        });

        expect(result.pagination?.totalPages).toBe(10);
        expect(result.pagination?.totalMatches).toBe(1000);
        expect(result.pagination?.reportedTotalMatches).toBe(5000);
        expect(result.pagination?.reachableTotalMatches).toBe(1000);
        expect(result.pagination?.totalMatchesCapped).toBe(true);
      });

      it('should set hasMore=true when more pages exist', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(50, 5));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const result = await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 5,
          page: 1,
        });

        expect(result.pagination?.hasMore).toBe(true);
        expect(result.pagination?.totalPages).toBe(10);
      });

      it('should set hasMore=false on last page', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(50, 5));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const result = await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 5,
          page: 10,
        });

        expect(result.pagination?.hasMore).toBe(false);
        expect(result.pagination?.currentPage).toBe(10);
        expect(result.pagination?.totalPages).toBe(10);
      });

      it('should include perPage in pagination info', async () => {
        const searchMock = vi
          .fn()
          .mockResolvedValue(createMockSearchResponse(100, 7));

        vi.mocked(getOctokit).mockResolvedValue(
          createMockOctokit({
            issuesAndPullRequests: searchMock,
          }) as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const result = await searchGitHubPullRequestsAPI({
          query: 'fix bug',
          limit: 7,
          page: 1,
        });

        expect(result.pagination?.perPage).toBe(7);
      });
    });
  });

  describe('REST API pagination', () => {
    it('should pass page parameter to REST API', async () => {
      const pullsListMock = vi
        .fn()
        .mockResolvedValue(createMockRESTResponse(5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          pullsList: pullsListMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      await searchGitHubPullRequestsAPI({
        owner: 'facebook',
        repo: 'react',
        state: 'open',
        limit: 5,
        page: 2,
      });

      expect(pullsListMock).toHaveBeenCalledWith(
        expect.objectContaining({
          page: 2,
          per_page: 5,
        })
      );
    });

    it('should estimate hasMore based on results length', async () => {
      const pullsListMock = vi
        .fn()
        .mockResolvedValue(createMockRESTResponse(5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          pullsList: pullsListMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await searchGitHubPullRequestsAPI({
        owner: 'facebook',
        repo: 'react',
        state: 'open',
        limit: 5,
        page: 1,
      });

      expect(result.pagination?.hasMore).toBe(true);
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination?.totalMatches).toBe(6);
      expect(result.pagination?.reachableTotalMatches).toBe(5);
      expect(result.pagination?.totalMatchesKind).toBe('lowerBound');
    });

    it('should set hasMore=false when results are less than limit', async () => {
      const pullsListMock = vi
        .fn()
        .mockResolvedValue(createMockRESTResponse(3));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          pullsList: pullsListMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await searchGitHubPullRequestsAPI({
        owner: 'facebook',
        repo: 'react',
        state: 'open',
        limit: 5,
        page: 1,
      });

      expect(result.pagination?.hasMore).toBe(false);
      expect(result.pagination?.totalMatches).toBe(3);
      expect(result.pagination?.reachableTotalMatches).toBe(3);
      expect(result.pagination?.totalMatchesKind).toBe('exact');
    });
  });

  describe('caching with pagination', () => {
    it('should cache different pages separately', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(100, 5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

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

      expect(searchMock).toHaveBeenCalledTimes(2);
    });

    it('should return cached result for same page', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(100, 5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

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

      expect(searchMock).toHaveBeenCalledTimes(1);
    });

    it('should not cache context fields (mainResearchGoal, researchGoal, reasoning)', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(100, 5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

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

      expect(searchMock).toHaveBeenCalledTimes(1);
    });
  });

  describe('edge cases', () => {
    it('should handle single page of results', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(3, 3));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        limit: 5,
        page: 1,
      });

      expect(result.pagination?.totalPages).toBe(1);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should handle zero results', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(0, 0));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await searchGitHubPullRequestsAPI({
        query: 'nonexistent',
        limit: 5,
        page: 1,
      });

      expect(result.pagination?.totalPages).toBe(0);
      expect(result.pagination?.totalMatches).toBe(0);
      expect(result.pagination?.hasMore).toBe(false);
    });

    it('should use default limit when not specified', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(100, 30));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        page: 1,
      });

      expect(result.pagination?.perPage).toBe(30);
      expect(searchMock).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 30,
        })
      );
    });

    it('should cap per_page at 100', async () => {
      const searchMock = vi
        .fn()
        .mockResolvedValue(createMockSearchResponse(100, 100));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit({
          issuesAndPullRequests: searchMock,
        }) as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      await searchGitHubPullRequestsAPI({
        query: 'fix bug',
        limit: 150,
        page: 1,
      });

      expect(searchMock).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 100,
        })
      );
    });
  });
});
