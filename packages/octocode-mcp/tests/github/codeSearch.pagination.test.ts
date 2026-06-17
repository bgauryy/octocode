import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubCodeAPI } from '../../../octocode-tools-core/src/github/codeSearch.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('../../../octocode-tools-core/src/session.js', () => ({
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        limit: 10,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(10);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalMatches
      ).toBe(95);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
          ?.reportedTotalMatches
      ).toBe(95);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
          ?.reachableTotalMatches
      ).toBe(95);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
          ?.totalMatchesKind
      ).toBe('reported');
    });

    it('should cap totalPages at 10 (GitHub 1000 result limit)', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(5000, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        limit: 100,
        page: 1,
      });

      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalPages
      ).toBe(10);
      expect(
        ('data' in result ? result.data : undefined)?.pagination?.totalMatches
      ).toBe(1000);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
          ?.reportedTotalMatches
      ).toBe(5000);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
          ?.reachableTotalMatches
      ).toBe(1000);
      expect(
        ('data' in result ? result.data : undefined)?.pagination
          ?.totalMatchesCapped
      ).toBe(true);
    });

    it('should set hasMore=true when more pages exist', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 10));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      await searchGitHubCodeAPI({
        keywords: ['test'],
        limit: 10,
        page: 1,
      });

      await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      await searchGitHubCodeAPI({
        keywords: ['test'],
        limit: 10,
        page: 1,
      });

      await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
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
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      await searchGitHubCodeAPI({
        keywords: ['test'],
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

  describe('uniqueFileCount in pagination', () => {
    it('should include uniqueFileCount equal to distinct file paths', async () => {
      const searchCodeMock = vi
        .fn()
        .mockResolvedValue(createMockResponse(50, 5));

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        limit: 10,
      });

      expect(result).toBeDefined();
      expect(result?.status).toBe(200);
      if (result?.status === 200) {
        expect(result.data.pagination?.uniqueFileCount).toBe(5);
      }
    });

    it('should deduplicate paths in uniqueFileCount', async () => {
      const duplicateMockResponse = {
        data: {
          total_count: 4,
          items: [
            {
              name: 'a.ts',
              path: 'src/a.ts',
              repository: {
                full_name: 'test/repo',
                url: 'u',
                owner: { login: 'test' },
              },
              url: 'u',
              html_url: 'h',
              sha: 's0',
            },
            {
              name: 'a.ts',
              path: 'src/a.ts',
              repository: {
                full_name: 'test/repo',
                url: 'u',
                owner: { login: 'test' },
              },
              url: 'u',
              html_url: 'h',
              sha: 's1',
            },
            {
              name: 'b.ts',
              path: 'src/b.ts',
              repository: {
                full_name: 'test/repo',
                url: 'u',
                owner: { login: 'test' },
              },
              url: 'u',
              html_url: 'h',
              sha: 's2',
            },
            {
              name: 'b.ts',
              path: 'src/b.ts',
              repository: {
                full_name: 'test/repo',
                url: 'u',
                owner: { login: 'test' },
              },
              url: 'u',
              html_url: 'h',
              sha: 's3',
            },
          ],
          incomplete_results: false,
        },
        headers: {},
      };

      const searchCodeMock = vi.fn().mockResolvedValue(duplicateMockResponse);

      vi.mocked(getOctokit).mockResolvedValue(
        createMockOctokit(searchCodeMock) as unknown as Awaited<
          ReturnType<typeof getOctokit>
        >
      );

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        limit: 10,
      });

      expect(result?.status).toBe(200);
      if (result?.status === 200) {
        expect(result.data.pagination?.uniqueFileCount).toBe(2);
      }
    });
  });
});
