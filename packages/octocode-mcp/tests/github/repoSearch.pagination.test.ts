import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubReposAPI } from '../../src/github/repoSearch.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('Repository Search - Pagination', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  const createMockOctokit = (searchReposMock: ReturnType<typeof vi.fn>) => ({
    rest: {
      search: {
        repos: searchReposMock,
      },
    },
  });

  const createMockResponse = (totalCount: number, itemCount: number) => ({
    data: {
      total_count: totalCount,
      items: Array.from({ length: itemCount }, (_, i) => ({
        id: i,
        name: `repo${i}`,
        full_name: `owner/repo${i}`,
        owner: { login: 'owner' },
        html_url: `https://github.com/owner/repo${i}`,
        description: `Description for repo ${i}`,
        stargazers_count: 1000 - i,
        forks_count: 100,
        language: 'TypeScript',
        topics: ['typescript'],
        updated_at: '2024-01-01T00:00:00Z',
        pushed_at: '2024-01-01T00:00:00Z',
        created_at: '2023-01-01T00:00:00Z',
        default_branch: 'main',
        archived: false,
        visibility: 'public',
        license: { spdx_id: 'MIT' },
      })),
      incomplete_results: false,
    },
    headers: {},
  });

  describe('page parameter', () => {
    it('should pass page parameter to GitHub API (default page=1)', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
      });

      expect(searchReposMock).toHaveBeenCalledWith(
        expect.objectContaining({
          page: 1,
          per_page: 10,
        })
      );
    });

    it('should pass custom page parameter to GitHub API', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 3,
      });

      expect(searchReposMock).toHaveBeenCalledWith(
        expect.objectContaining({
          page: 3,
          per_page: 10,
        })
      );
    });
  });

  describe('pagination metadata in response', () => {
    it('should include pagination info with currentPage', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 2,
      });

      expect(result.status).toBe(200);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
      ).toBeDefined();
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.currentPage
      ).toBe(2);
    });

    it('should calculate totalPages correctly', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(95, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(10);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalMatches
      ).toBe(95);
    });

    it('should cap totalPages at 10 (GitHub 1000 result limit)', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(5000, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 100,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(10);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalMatches
      ).toBe(1000);
    });

    it('should set hasMore=true when more pages exist', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.hasMore
      ).toBe(true);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(5);
    });

    it('should set hasMore=false on last page', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 5,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.hasMore
      ).toBe(false);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.currentPage
      ).toBe(5);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(5);
    });

    it('should include perPage in pagination info', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 25));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 25,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.perPage
      ).toBe(25);
    });
  });

  describe('caching with pagination', () => {
    it('should cache different pages separately', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

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

      expect(searchReposMock).toHaveBeenCalledTimes(2);
    });

    it('should return cached result for same page', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
      });

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
      });

      expect(searchReposMock).toHaveBeenCalledTimes(1);
    });

    it('should not cache context fields (mainResearchGoal, researchGoal, reasoning)', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
        mainResearchGoal: 'Goal 1',
        researchGoal: 'Research 1',
        reasoning: 'Reason 1',
      });

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
        mainResearchGoal: 'Different Goal',
        researchGoal: 'Different Research',
        reasoning: 'Different Reason',
      });

      expect(searchReposMock).toHaveBeenCalledTimes(1);
    });
  });

  describe('edge cases', () => {
    it('should handle single page of results', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(5, 5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(1);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.hasMore
      ).toBe(false);
    });

    it('should handle zero results', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(0, 0));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['nonexistent-repo-xyz'],
        limit: 10,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(0);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalMatches
      ).toBe(0);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.hasMore
      ).toBe(false);
    });

    it('should use default limit of 30 when not specified', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 30));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.perPage
      ).toBe(30);
      expect(searchReposMock).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 30,
        })
      );
    });

    it('should cap per_page at 100', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 100));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 150,
        page: 1,
      });

      expect(searchReposMock).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 100,
        })
      );
    });
  });

  describe('sorting with pagination', () => {
    it('should pass sort parameter along with pagination', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 2,
        sort: 'stars',
      });

      expect(searchReposMock).toHaveBeenCalledWith(
        expect.objectContaining({
          page: 2,
          per_page: 10,
          sort: 'stars',
        })
      );
    });

    it('should not pass sort for best-match', async () => {
      const searchReposMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchReposMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubReposAPI({
        keywordsToSearch: ['react'],
        limit: 10,
        page: 1,
        sort: 'best-match',
      });

      expect(searchReposMock).toHaveBeenCalledWith(
        expect.not.objectContaining({
          sort: expect.anything(),
        })
      );
    });
  });
});
