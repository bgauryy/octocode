import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(),
  withCache: vi.fn(),
}));

vi.mock('../../../octocode-tools-core/src/tools/utils/tokenManager.js', () => ({
  getGitHubToken: mockGetGitHubToken,
}));

const mockGetActiveProviderConfig = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getGitHubToken: mockGetGitHubToken,
  getActiveProviderConfig: mockGetActiveProviderConfig,
  getServerConfig: vi.fn(() => ({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
  })),
}));

import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

function createMockPRProviderResponse(overrides: Record<string, unknown> = {}) {
  return {
    data: {
      items: [
        {
          id: 456,
          number: 456,
          title: 'Test PR',
          state: 'open',
          draft: false,
          merged: false,
          createdAt: '2023-01-01T00:00:00Z',
          updatedAt: '2023-01-01T00:00:00Z',
          closedAt: null,
          mergedAt: null,
          author: { login: 'testuser', id: '1' },
          assignees: [],
          labels: [],
          head: { ref: 'feature-branch', sha: 'abc123' },
          base: { ref: 'main', sha: 'def456' },
          body: 'Test PR description',
          comments: 0,
          reviewComments: 0,
          additions: 10,
          deletions: 5,
          changedFiles: 2,
          url: 'https://github.com/test/repo/pull/456',
          repository: { id: '1', name: 'test/repo', url: '' },
        },
      ],
      totalCount: 1,
      pagination: { currentPage: 1, totalPages: 1, hasMore: false },
      ...overrides,
    },
    status: 200,
    provider: 'github',
  };
}

describe('GitHub Search Pull Requests Tool', () => {
  let mockServer: MockMcpServer;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    mockGetActiveProviderConfig.mockReturnValue({
      provider: 'github',
      baseUrl: undefined,
      token: 'mock-token',
    });
    mockServer = createMockMcpServer();

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    registerSearchGitHubPullRequestsTool(mockServer.server);
    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
    mockGetGitHubToken.mockResolvedValue('test-token');
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('Callback invocation', () => {
    it('should invoke callback when registered with callback', async () => {
      const callback = vi.fn().mockResolvedValue(undefined);
      mockServer.cleanup();
      registerSearchGitHubPullRequestsTool(mockServer.server, callback);
      mockGetProvider.mockReturnValue(mockProvider);
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
      });

      expect(callback).toHaveBeenCalledWith(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        expect.arrayContaining([
          expect.objectContaining({ owner: 'test', repo: 'repo' }),
        ])
      );
    });
  });

  describe('Basic Search', () => {
    it('should search for pull requests and return results', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              state: 'open',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Test PR');
    });

    it('should handle no results', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              state: 'open',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('empty');
    });
  });

  describe('Filters', () => {
    it('should filter by state', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              state: 'closed',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should filter by author', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              author: 'testuser',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle merged filter', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              state: 'merged',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('PR Number lookup', () => {
    it('should fetch specific PR by number', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 456,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('456');
    });
  });

  describe('Bulk queries', () => {
    it('should handle multiple queries', async () => {
      mockProvider.searchPullRequests
        .mockResolvedValueOnce(createMockPRProviderResponse())
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                id: 789,
                number: 789,
                title: 'Second PR',
                state: 'closed',
                merged: true,
                draft: false,
                createdAt: '2023-01-02T00:00:00Z',
                updatedAt: '2023-01-02T00:00:00Z',
                closedAt: '2023-01-03T00:00:00Z',
                mergedAt: '2023-01-03T00:00:00Z',
                author: { login: 'user2', id: '2' },
                assignees: [],
                labels: [],
                head: { ref: 'fix-branch', sha: 'ghi789' },
                base: { ref: 'main', sha: 'jkl012' },
                body: 'Fix description',
                comments: 2,
                reviewComments: 1,
                additions: 5,
                deletions: 3,
                changedFiles: 1,
                url: 'https://github.com/test/repo/pull/789',
                repository: { id: '1', name: 'test/repo', url: '' },
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            { owner: 'test', repo: 'repo', state: 'open' },
            { owner: 'test', repo: 'repo', state: 'closed' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Test PR');
      expect(responseText).toContain('Second PR');
    });
  });

  describe('Error handling', () => {
    it('should handle API errors', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        error: 'Not found',
        status: 404,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'nonexistent',
              repo: 'repo',
            },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });

    it('should handle provider exceptions', async () => {
      mockProvider.searchPullRequests.mockRejectedValue(
        new Error('Network error')
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
            },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });

  describe('Pagination', () => {
    it('should handle paginated results', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [createMockPRProviderResponse().data.items[0]],
          totalCount: 50,
          pagination: { currentPage: 1, totalPages: 5, hasMore: true },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              itemsPerPage: 10,
              page: 1,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should forward page parameter to the provider', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse({
          pagination: { currentPage: 3, totalPages: 5, hasMore: true },
        })
      );

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            state: 'open',
            page: 3,
          },
        ],
      });

      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      expect(providerQuery).toBeDefined();
      expect(providerQuery.page).toBe(3);
      expect(providerQuery.limit).toBeGreaterThan(0);
    });

    it('should include page in provider query when explicitly set', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            state: 'open',
            page: 1,
          },
        ],
      });

      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      expect(providerQuery).toBeDefined();
      expect(providerQuery.page).toBe(1);
    });
    it('should forward different page values correctly across bulk queries', async () => {
      mockProvider.searchPullRequests
        .mockResolvedValueOnce(
          createMockPRProviderResponse({
            pagination: { currentPage: 1, totalPages: 3, hasMore: true },
          })
        )
        .mockResolvedValueOnce(
          createMockPRProviderResponse({
            pagination: { currentPage: 2, totalPages: 3, hasMore: true },
          })
        );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            { owner: 'test', repo: 'repo', state: 'open', page: 1 },
            { owner: 'test', repo: 'repo', state: 'open', page: 2 },
          ],
        }
      );

      expect(result.isError).toBe(false);
      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(2);

      const firstQuery = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      const secondQuery = mockProvider.searchPullRequests.mock.calls[1]?.[0];
      expect(firstQuery.page).toBe(1);
      expect(secondQuery.page).toBe(2);
    });
  });

  describe('No valid params (execution branch)', () => {
    it('should return error when query has no valid search params', async () => {
      const { searchMultipleGitHubPullRequests } =
        await import('../../../octocode-tools-core/src/tools/github_search_pull_requests/execution.js');

      const result = await searchMultipleGitHubPullRequests({
        queries: [
          {
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
            state: 'open',
            draft: false,
          },
        ],
        authInfo: undefined,
        sessionId: undefined,
      });

      const text = getTextContent(result.content);
      expect(text).toContain('At least one valid search parameter');
    });
  });

  describe('Large file change PRs', () => {
    it('should include file change hints when PR has >30 file changes', async () => {
      const largeFileChanges = Array.from({ length: 35 }, (_, i) => ({
        path: `src/file${i}.ts`,
        additions: 1,
        deletions: 0,
      }));
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [
            {
              ...createMockPRProviderResponse().data.items[0],
              fileChanges: largeFileChanges,
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('35+ file changes');
    });
  });

  describe('Owner-only search (no repo)', () => {
    it('should pass owner to provider when only owner is specified', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [
          {
            owner: 'organization-private',
            state: 'open',
          },
        ],
      });

      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      expect(providerQuery.owner).toBe('organization-private');
    });

    it('should return results when searching by owner only', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'organization-private',
              author: 'developer',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Test PR');
    });
  });

  describe('Query text search', () => {
    it('should search by query string', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createMockPRProviderResponse()
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              query: 'fix bug',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('Content selector output', () => {
    it('keeps broad search lean without per-PR next menus (Metadata mode hint covers escalation)', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [
            {
              ...createMockPRProviderResponse().data.items[0],
              body: 'A'.repeat(1200),
              fileChanges: [
                {
                  path: 'src/a.ts',
                  status: 'modified',
                  additions: 1,
                  deletions: 1,
                  patch: 'HUGE PATCH SHOULD NOT SHOW',
                },
              ],
              comments: [
                {
                  id: '1',
                  author: 'reviewer',
                  body: 'COMMENT SHOULD NOT SHOW',
                  createdAt: '2024-01-01T00:00:00Z',
                  updatedAt: '2024-01-01T00:00:00Z',
                  commentType: 'discussion',
                },
              ],
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              query: 'fix bug',
              content: {
                comments: { discussion: true },
                patches: { mode: 'all' },
              },
            },
          ],
        }
      );

      const text = getTextContent(result.content);
      expect(text).not.toContain('getBody');
      expect(text).not.toContain('getChangedFiles');
      expect(text).not.toContain('Patches not included');
      expect(text).not.toContain('Comments not included');
      expect(text).toContain('Broad PR search returns metadata only');
      expect(text).toContain('Metadata mode:');
      expect(text).not.toContain('HUGE PATCH SHOULD NOT SHOW');
      expect(text).not.toContain('COMMENT SHOULD NOT SHOW');
    });

    it('returns selected direct PR content; exhausted pagination blocks are pruned at render', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [
            {
              ...createMockPRProviderResponse().data.items[0],
              body: 'Body text',
              fileChanges: [
                {
                  path: 'src/a.ts',
                  status: 'modified',
                  additions: 1,
                  deletions: 1,
                  patch: 'patch-a',
                },
                {
                  path: 'src/b.ts',
                  status: 'modified',
                  additions: 2,
                  deletions: 0,
                  patch: 'patch-b',
                },
              ],
              comments: [
                {
                  id: '1',
                  author: 'reviewer',
                  body: 'Inline comment',
                  createdAt: '2024-01-01T00:00:00Z',
                  updatedAt: '2024-01-01T00:00:00Z',
                  commentType: 'review_inline',
                  path: 'src/a.ts',
                  line: 10,
                },
              ],
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 456,
              itemsPerPage: 1,
              content: {
                body: true,
                changedFiles: true,
                patches: { mode: 'selected', files: ['src/a.ts'] },
                comments: {
                  reviewInline: true,
                  discussion: false,
                  file: 'src/a.ts',
                },
              },
            },
          ],
        }
      );

      const text = getTextContent(result.content);
      expect(text).toContain('body: Body text');
      expect(text).toContain('path: src/a.ts');
      expect(text).toContain('patch: patch-a');
      expect(text).not.toContain('patch-b');
      expect(text).not.toContain('filePagination');
      expect(text).not.toContain('commentPagination');
    });
  });
});
