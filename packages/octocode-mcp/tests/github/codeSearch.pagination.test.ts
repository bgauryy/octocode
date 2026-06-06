import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('Code Search - Pagination', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  const createMockOctokit = (searchCodeMock: ReturnType<typeof vi.fn>) => ({
    rest: {
      search: {
        code: searchCodeMock,
      },
    },
  });

  const createMockResponse = (totalCount: number, itemCount: number) => ({
    data: {
      total_count: totalCount,
      items: Array.from({ length: itemCount }, (_, i) => ({
        name: `file${i}.ts`,
        path: `src/file${i}.ts`,
        repository: {
          full_name: 'test/repo',
          url: 'repo_url',
          owner: { login: 'test' },
        },
        url: 'file_url',
        html_url: 'html_url',
        sha: `sha${i}`,
      })),
      incomplete_results: false,
    },
    headers: {},
  });

  describe('page parameter', () => {
    it('should pass page parameter to GitHub API (default page=1)', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 10,
      });

      expect(searchCodeMock).toHaveBeenCalledWith(
        expect.objectContaining({
          page: 1,
          per_page: 10,
        })
      );
    });

    it('should pass custom page parameter to GitHub API', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 10,
        page: 3,
      });

      expect(searchCodeMock).toHaveBeenCalledWith(
        expect.objectContaining({
          page: 3,
          per_page: 10,
        })
      );
    });
  });

  describe('pagination metadata in response', () => {
    it('should include pagination info with currentPage', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(95, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(5000, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 25));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 10,
        page: 1,
      });

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 10,
        page: 2,
      });

      expect(searchCodeMock).toHaveBeenCalledTimes(2);
    });

    it('should return cached result for same page', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 10,
        page: 1,
      });

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 10,
        page: 1,
      });

      expect(searchCodeMock).toHaveBeenCalledTimes(1);
    });
  });

  describe('edge cases', () => {
    it('should handle single page of results', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(5, 5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(0, 0));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
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
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 30));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      const result = await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.perPage
      ).toBe(30);
      expect(searchCodeMock).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 30,
        })
      );
    });

    it('should cap per_page at 100', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(100, 100));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as ReturnType<
          typeof getOctokit
        >
      );

      await searchGitHubCodeAPI({
        keywordsToSearch: ['test'],
        limit: 150,
        page: 1,
      });

      expect(searchCodeMock).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 100,
        })
      );
    });
  });
});
