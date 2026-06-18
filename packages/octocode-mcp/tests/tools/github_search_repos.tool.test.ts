import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getGitHubToken: vi.fn(() => Promise.resolve('mock-token')),
  getServerConfig: vi.fn(() => ({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: false,
  })),
}));

import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

describe('GitHub Search Repos Tool - Comprehensive Status Tests', () => {
  let mockServer: MockMcpServer;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    mockServer = createMockMcpServer();

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
    registerSearchGitHubReposTool(mockServer.server);
  });

  afterEach(() => {
    mockServer.cleanup();
  });

  describe('Status: hasResults', () => {
    it('rejects an empty repository search before calling the provider', async () => {
      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [{}],
        }
      );

      const responseText = getTextContent(result.content);
      expect(responseText).toContain(
        'At least one repository search term or filter is required'
      );
      expect(mockProvider.searchRepos).not.toHaveBeenCalled();
    });

    it('should return hasResults status when API returns repositories', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'react',
              fullPath: 'facebook/react',
              description: 'A declarative JavaScript library',
              url: 'https://github.com/facebook/react',
              stars: 200000,
              forks: 40000,
              language: 'JavaScript',
              topics: ['javascript', 'react'],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
            {
              id: '2',
              name: 'next.js',
              fullPath: 'vercel/next.js',
              description: 'The React Framework',
              url: 'https://github.com/vercel/next.js',
              stars: 100000,
              forks: 20000,
              language: 'JavaScript',
              topics: ['nextjs', 'react'],
              createdAt: '2024-01-14',
              updatedAt: '2024-01-14',
              pushedAt: '2024-01-14',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 2,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              keywords: ['react'],
              limit: 2,
            },
          ],
        }
      );

      const responseText = getTextContent(result.content);

      expect(result.isError).toBe(false);
      expect(responseText).not.toContain('status: hasResults');
      expect(responseText).toContain('facebook');
      expect(responseText).toContain('react');
      expect(responseText).toContain('vercel');

      type RepoDetail = {
        owner?: string;
        repo?: string;
        stars?: number;
        forks?: number;
      };
      const repos = result.structuredContent as {
        results?: Array<{ data?: { repositories?: RepoDetail[] } }>;
      };
      const repoItems = repos.results?.[0]?.data?.repositories ?? [];
      expect(`${repoItems[0]?.owner}/${repoItems[0]?.repo}`).toBe(
        'facebook/react'
      );
      expect(repoItems[0]?.stars).toBe(200000);
      expect(repoItems[0]?.forks).toBe(40000);
      expect(`${repoItems[1]?.owner}/${repoItems[1]?.repo}`).toBe(
        'vercel/next.js'
      );
    });

    it('should handle single repository result', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'TypeScript',
              fullPath: 'microsoft/TypeScript',
              description: 'TypeScript language',
              url: 'https://github.com/microsoft/TypeScript',
              stars: 90000,
              forks: 12000,
              language: 'TypeScript',
              topics: ['typescript'],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [{ keywords: ['typescript'] }],
        }
      );

      const responseText = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(responseText).toContain('microsoft/TypeScript');
    });
  });

  describe('Keywords-only empty hint (hasKeywords && !hasResults && !hasTopics)', () => {
    it('should include keywordsEmpty hint when keywords search returns no results', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              keywords: ['veryobscurekeyword123'],
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('empty');
    });
  });

  describe('Status: empty', () => {
    it('should return empty status when no repositories found', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              keywords: ['veryrandomnonexistent123'],
            },
          ],
        }
      );

      const responseText = getTextContent(result.content);
      expect(result.isError).toBe(false);
      expect(responseText).toContain('empty');
    });
  });

  describe('Status: error', () => {
    it('should return error status when API fails', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        error: 'Rate limit exceeded',
        status: 403,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [{ keywords: ['test'] }],
        }
      );

      const responseText = getTextContent(result.content);
      expect(result.isError).toBe(true);
      expect(responseText).toContain('error');
    });
  });

  describe('Filters', () => {
    it('should handle stars filter', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'repo',
              fullPath: 'popular/repo',
              description: 'Popular repo',
              url: 'https://github.com/popular/repo',
              stars: 50000,
              forks: 5000,
              language: 'JavaScript',
              topics: [],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              keywords: ['popular'],
              stars: '>10000',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle owner filter', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'react',
              fullPath: 'facebook/react',
              description: 'React',
              url: 'https://github.com/facebook/react',
              stars: 200000,
              forks: 40000,
              language: 'JavaScript',
              topics: [],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              owner: 'facebook',
              keywords: ['react'],
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('facebook');
    });

    it('should handle topics filter', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'cli',
              fullPath: 'awesome/cli',
              description: 'CLI tool',
              url: 'https://github.com/awesome/cli',
              stars: 5000,
              forks: 500,
              language: 'TypeScript',
              topics: ['cli', 'typescript'],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              topicsToSearch: ['cli', 'typescript'],
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('Bulk queries', () => {
    it('should handle multiple queries', async () => {
      mockProvider.searchRepos
        .mockResolvedValueOnce({
          data: {
            repositories: [
              {
                id: '1',
                name: 'react',
                fullPath: 'facebook/react',
                description: 'React',
                url: 'https://github.com/facebook/react',
                stars: 200000,
                forks: 40000,
                language: 'JavaScript',
                topics: [],
                createdAt: '2024-01-15',
                updatedAt: '2024-01-15',
                pushedAt: '2024-01-15',
                defaultBranch: 'main',
                isPrivate: false,
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            repositories: [
              {
                id: '2',
                name: 'vue',
                fullPath: 'vuejs/vue',
                description: 'Vue',
                url: 'https://github.com/vuejs/vue',
                stars: 180000,
                forks: 30000,
                language: 'JavaScript',
                topics: [],
                createdAt: '2024-01-15',
                updatedAt: '2024-01-15',
                pushedAt: '2024-01-15',
                defaultBranch: 'main',
                isPrivate: false,
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [{ keywords: ['react'] }, { keywords: ['vue'] }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('react');
      expect(responseText).toContain('vue');
    });
  });

  describe('Pagination', () => {
    it('should handle paginated results', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'repo',
              fullPath: 'test/repo',
              description: 'Test',
              url: 'https://github.com/test/repo',
              stars: 100,
              forks: 10,
              language: 'JavaScript',
              topics: [],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 100,
          pagination: { currentPage: 2, totalPages: 10, hasMore: true },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              keywords: ['test'],
              page: 2,
              limit: 10,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('returns each repository as a complete one-liner — never truncates mid-item', async () => {
      const topics = Array.from({ length: 5 }, (_, index) => `topic-${index}`);
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: Array.from({ length: 6 }, (_, i) => ({
            id: `${i}`,
            name: `repo-${i}`,
            fullPath: `test/repo-${i}`,
            description: 'Test repository',
            url: `https://github.com/test/repo-${i}`,
            stars: 100,
            forks: 10,
            language: 'TypeScript',
            topics,
            createdAt: '2024-01-15',
            updatedAt: '2024-01-15',
            pushedAt: '2024-01-15',
            defaultBranch: 'main',
            isPrivate: false,
          })),
          totalCount: 6,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const firstResult = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        { queries: [{ keywords: ['repo'], concise: true }] }
      );

      const firstStructured = firstResult.structuredContent as {
        results: Array<{ data: { repositories?: string[] } }>;
      };
      const firstData = firstStructured.results[0]!.data;

      expect(firstData.repositories?.length ?? 0).toBeGreaterThan(0);
      for (const line of firstData.repositories ?? []) {
        expect(typeof line).toBe('string');
        expect(line).toMatch(/^test\/repo-\d+/);
      }
    });
  });

  describe('Owner-only search (TC-7)', () => {
    it('should allow owner-only search without keywords or topics', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'react',
              fullPath: 'facebook/react',
              description: 'React library',
              url: 'https://github.com/facebook/react',
              stars: 200000,
              forks: 40000,
              language: 'JavaScript',
              topics: [],
              createdAt: '2024-01-15',
              updatedAt: '2024-01-15',
              pushedAt: '2024-01-15',
              defaultBranch: 'main',
              isPrivate: false,
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              owner: 'facebook',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('facebook');
    });

    it('should still reject queries with no owner, keywords, or topics', async () => {
      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              stars: '>1000',
            },
          ],
        }
      );

      expect(result.isError).toBe(true);
    });
  });

  describe('Stars range passthrough (TC-20, TC-22)', () => {
    it('should pass stars range filter correctly to provider', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [
          {
            keywords: ['react'],
            stars: '100..500',
          },
        ],
      });

      expect(mockProvider.searchRepos).toHaveBeenCalled();
      const providerCall = mockProvider.searchRepos.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(providerCall.stars || providerCall.minStars).toBeDefined();
      if (providerCall.stars) {
        expect(providerCall.stars).toBe('100..500');
      }
    });

    it('should pass >=1000 stars filter correctly', async () => {
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [
          {
            keywords: ['react'],
            stars: '>=1000',
          },
        ],
      });

      expect(mockProvider.searchRepos).toHaveBeenCalled();
      const providerCall = mockProvider.searchRepos.mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      if (providerCall.stars) {
        expect(providerCall.stars).toBe('>=1000');
      }
    });
  });

  describe('Exception handling', () => {
    it('should handle provider exceptions', async () => {
      mockProvider.searchRepos.mockRejectedValue(new Error('Network error'));

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [{ keywords: ['test'] }],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });
});
