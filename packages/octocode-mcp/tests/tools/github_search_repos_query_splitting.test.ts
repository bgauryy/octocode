import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import type { GitHubReposSearchQuery } from '@octocodeai/octocode-core';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
}));

describe('GitHub Search Repositories Query Splitting', () => {
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    // Mock successful API response
    mockProvider.searchRepos.mockResolvedValue({
      data: {
        repositories: [
          {
            id: '1',
            name: 'repo',
            fullPath: 'test/repo',
            description: 'Test repository',
            url: 'https://github.com/test/repo',
            stars: 100,
            forks: 10,
            language: 'JavaScript',
            topics: [],
            createdAt: '01/01/2020',
            updatedAt: '01/01/2024',
            pushedAt: '01/01/2024',
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
  });

  describe('Query Splitting Logic', () => {
    it('should split queries with both topicsToSearch and keywordsToSearch into separate queries', async () => {
      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const originalQuery: GitHubReposSearchQuery = {
        id: 'split_topics_keywords',
        reasoning: 'Test query with both search types',
        topicsToSearch: ['computer-vision', 'deep-learning'],
        keywordsToSearch: ['whale', 'detection'],
        limit: 10,
        sort: 'stars',
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [originalQuery],
      });

      // Should have been called twice - once for topics, once for keywords
      expect(mockProvider.searchRepos).toHaveBeenCalledTimes(2);
    });

    it('should NOT split queries with only topicsToSearch', async () => {
      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const originalQuery: GitHubReposSearchQuery = {
        id: 'topics_only',
        reasoning: 'Test query with only topics',
        topicsToSearch: ['computer-vision', 'deep-learning'],
        limit: 10,
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [originalQuery],
      });

      expect(mockProvider.searchRepos).toHaveBeenCalledTimes(1);
    });

    it('should NOT split queries with only keywordsToSearch', async () => {
      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const originalQuery: GitHubReposSearchQuery = {
        id: 'keywords_only',
        reasoning: 'Test query with only keywords',
        keywordsToSearch: ['whale', 'detection'],
        limit: 10,
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [originalQuery],
      });

      expect(mockProvider.searchRepos).toHaveBeenCalledTimes(1);
    });
  });

  describe('Response Deduplication', () => {
    it('should deduplicate results from split queries', async () => {
      // Both queries return the same repo
      mockProvider.searchRepos.mockResolvedValue({
        data: {
          repositories: [
            {
              id: '1',
              name: 'repo',
              fullPath: 'duplicate/repo',
              description: 'Duplicate repo',
              url: 'https://github.com/duplicate/repo',
              stars: 100,
              forks: 10,
              language: 'Python',
              topics: ['computer-vision'],
              createdAt: '01/01/2020',
              updatedAt: '01/01/2024',
              pushedAt: '01/01/2024',
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

      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              id: 'dedup_merged_result',
              reasoning: 'Test dedup',
              topicsToSearch: ['computer-vision'],
              keywordsToSearch: ['whale'],
              limit: 10,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "dedup_merged_result"');
      expect(
        (responseText.match(/id: "dedup_merged_result"/g) || []).length
      ).toBe(1);
      expect(responseText).toContain('duplicate/repo');
    });

    it('exposes merged pagination (as an upper bound) when both searches succeed', async () => {
      mockProvider.searchRepos
        .mockResolvedValueOnce({
          data: {
            repositories: [
              {
                id: '1',
                name: 'topic-repo',
                fullPath: 'topic/repo',
                description: 'Topic result',
                url: 'https://github.com/topic/repo',
                stars: 100,
                forks: 10,
                language: 'TypeScript',
                topics: ['topic'],
                createdAt: '01/01/2020',
                updatedAt: '01/01/2024',
                pushedAt: '01/01/2024',
                defaultBranch: 'main',
                isPrivate: false,
              },
            ],
            totalCount: 1,
            pagination: {
              currentPage: 1,
              totalPages: 2,
              hasMore: true,
              entriesPerPage: 10,
              totalMatches: 15,
            },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            repositories: [
              {
                id: '2',
                name: 'keyword-repo',
                fullPath: 'keyword/repo',
                description: 'Keyword result',
                url: 'https://github.com/keyword/repo',
                stars: 50,
                forks: 5,
                language: 'TypeScript',
                topics: [],
                createdAt: '01/01/2020',
                updatedAt: '01/01/2024',
                pushedAt: '01/01/2024',
                defaultBranch: 'main',
                isPrivate: false,
              },
            ],
            totalCount: 1,
            pagination: {
              currentPage: 1,
              totalPages: 3,
              hasMore: true,
              entriesPerPage: 10,
              totalMatches: 25,
            },
          },
          status: 200,
          provider: 'github',
        });

      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              id: 'merged_success',
              topicsToSearch: ['topic'],
              keywordsToSearch: ['keyword'],
            },
          ],
        }
      );

      const responseText = getTextContent(result.content);
      // The combined search is now PAGINABLE — no "omitted" dead-end.
      expect(responseText).not.toContain('pagination is omitted');
      // Structured pagination present so the agent can fetch the next page.
      expect(responseText).toContain('pagination:');
      // hasMore = either variant; next page guidance present.
      expect(responseText).toContain('fetch page 2');
      // totalMatches is the summed upper bound (15 + 25), disclosed as such.
      expect(responseText).toContain('upper bound');

      const structured = result.structuredContent as {
        results?: Array<{
          data?: { pagination?: { hasMore?: boolean; totalMatches?: number } };
        }>;
      };
      const pg = structured.results?.[0]?.data?.pagination;
      expect(pg?.hasMore).toBe(true);
      expect(pg?.totalMatches).toBe(40);
    });

    it('ranks merged split-query repositories by explicit relevance', async () => {
      mockProvider.searchRepos
        .mockResolvedValueOnce({
          data: {
            repositories: [
              {
                id: '1',
                name: 'general',
                fullPath: 'topic/general',
                description: 'General utility',
                url: 'https://github.com/topic/general',
                stars: 500,
                forks: 10,
                language: 'TypeScript',
                topics: ['topic'],
                createdAt: '01/01/2020',
                updatedAt: '01/01/2024',
                pushedAt: '01/01/2024',
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
                name: 'keyword',
                fullPath: 'keyword/keyword',
                description: 'Keyword-focused result',
                url: 'https://github.com/keyword/keyword-engine',
                stars: 50,
                forks: 5,
                language: 'TypeScript',
                topics: [],
                createdAt: '01/01/2020',
                updatedAt: '01/01/2024',
                pushedAt: '01/01/2024',
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

      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              id: 'ranked_merge',
              topicsToSearch: ['topic'],
              keywordsToSearch: ['keyword'],
            },
          ],
        }
      );

      const structured = result.structuredContent as {
        results?: Array<{
          data?: {
            repositories?: Array<{ owner: string; repo: string }>;
          };
        }>;
      };
      expect(structured.results?.[0]?.data?.repositories?.[0]).toMatchObject({
        owner: 'keyword',
        repo: 'keyword',
      });
    });
  });

  describe('Error Handling', () => {
    it('should handle partial failures in split queries', async () => {
      mockProvider.searchRepos
        .mockResolvedValueOnce({
          data: {
            repositories: [
              {
                id: '1',
                name: 'repo',
                fullPath: 'success/repo',
                description: 'Success',
                url: 'https://github.com/success/repo',
                stars: 100,
                forks: 10,
                language: 'Python',
                topics: [],
                createdAt: '01/01/2020',
                updatedAt: '01/01/2024',
                pushedAt: '01/01/2024',
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
          error: 'Rate limit exceeded',
          status: 403,
          provider: 'github',
        });

      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              id: 'partial_failure_query',
              reasoning: 'Test partial failure',
              topicsToSearch: ['topic1'],
              keywordsToSearch: ['keyword1'],
              limit: 10,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "partial_failure_query"');
      expect(
        (responseText.match(/id: "partial_failure_query"/g) || []).length
      ).toBe(1);
      expect(responseText).toContain('success/repo');
      expect(responseText).toContain(
        'Keyword search failed: Rate limit exceeded'
      );
      expect(responseText).toContain(
        'Only topics search succeeded; pagination reflects that subset.'
      );
    });

    it('returns the first normalized provider failure when all split variants fail', async () => {
      mockProvider.searchRepos
        .mockResolvedValueOnce({
          error: 'Rate limit exceeded',
          status: 429,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          error: 'Secondary failure',
          status: 500,
          provider: 'github',
        });

      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        {
          queries: [
            {
              id: 'all_failures',
              topicsToSearch: ['topic1'],
              keywordsToSearch: ['keyword1'],
            },
          ],
        }
      );

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('status: "error"');
      expect(responseText).toContain('Rate limit exceeded');
      expect(responseText).not.toContain('Secondary failure');
    });
  });

  describe('Filter Preservation', () => {
    it('should preserve all filters in split queries', async () => {
      const mockServer = createMockMcpServer();
      registerSearchGitHubReposTool(mockServer.server);

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [
          {
            id: 'filter_preservation_query',
            reasoning: 'Test filter preservation',
            topicsToSearch: ['topic1'],
            keywordsToSearch: ['keyword1'],
            stars: '>1000',
            sort: 'stars',
            limit: 5,
          },
        ],
      });

      expect(mockProvider.searchRepos).toHaveBeenCalledTimes(2);
    });
  });
});
