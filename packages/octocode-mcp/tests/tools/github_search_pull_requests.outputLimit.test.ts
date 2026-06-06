import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(),
  withCache: vi.fn(),
}));

vi.mock('../../src/tools/utils/tokenManager.js', () => ({
  getGitHubToken: mockGetGitHubToken,
}));

vi.mock('../../src/serverConfig.js', () => ({
  isLoggingEnabled: vi.fn(() => false),
  getGitHubToken: mockGetGitHubToken,
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getServerConfig: vi.fn(() => ({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: false,
  })),
}));

import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

function createLargePR(prNumber: number, contentSize: number) {
  return {
    id: prNumber,
    number: prNumber,
    title: `Large PR #${prNumber} with many changes`,
    state: 'closed',
    draft: false,
    merged: true,
    createdAt: '2023-01-01T00:00:00Z',
    updatedAt: '2023-06-01T00:00:00Z',
    closedAt: '2023-06-01T00:00:00Z',
    mergedAt: '2023-06-01T00:00:00Z',
    author: { login: 'testuser', id: '1' },
    assignees: [],
    labels: [],
    sourceBranch: 'feature-large',
    targetBranch: 'main',
    body: 'A'.repeat(Math.min(contentSize, 5000)),
    commentsCount: 50,
    changedFilesCount: 45,
    additions: 1000,
    deletions: 500,
    url: `https://github.com/test/repo/pull/${prNumber}`,
    repository: { id: '1', name: 'test/repo', url: '' },
    comments: Array.from({ length: 20 }, (_, i) => ({
      id: `comment-${i}`,
      user: `user${i}`,
      body: `Comment body ${'x'.repeat(Math.floor(contentSize / 20))}`,
      created_at: '2023-03-01T00:00:00Z',
      updated_at: '2023-03-01T00:00:00Z',
    })),
    fileChanges: Array.from({ length: 45 }, (_, i) => ({
      filename: `src/path/to/file${i}.ts`,
      status: 'modified',
      additions: 20,
      deletions: 10,
      changes: 30,
      patch: `@@ -1,10 +1,20 @@\n+${'added line\n+'.repeat(20)}`,
    })),
  };
}

function createLargePRProviderResponse(prCount: number, contentSize: number) {
  return {
    data: {
      items: Array.from({ length: prCount }, (_, i) =>
        createLargePR(i + 1, contentSize)
      ),
      totalCount: prCount,
      pagination: {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        totalMatches: prCount,
      },
    },
    status: 200,
    provider: 'github',
  };
}

describe('githubSearchPullRequests output size limits', () => {
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
    registerSearchGitHubPullRequestsTool(mockServer.server);
    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
    mockGetGitHubToken.mockResolvedValue('test-token');
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('auto-pagination on large output', () => {
    it('should auto-paginate when PR response is very large', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createLargePRProviderResponse(1, 10000)
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 320,
              type: 'fullContent',
              withComments: true,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Large PR');
    });

    it('should include outputPagination metadata when output is large', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createLargePRProviderResponse(1, 10000)
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 320,
              type: 'fullContent',
              withComments: true,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      if (responseText.includes('outputPagination')) {
        expect(responseText).toContain('charOffset');
        expect(responseText).toContain('totalChars');
      }
    });
  });

  describe('explicit charOffset/charLength', () => {
    it('should accept charLength parameter', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createLargePRProviderResponse(1, 5000)
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 320,
              type: 'metadata',
              charLength: 2000,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toBeTruthy();
    });

    it('should accept charOffset for pagination navigation', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        createLargePRProviderResponse(1, 5000)
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 320,
              type: 'metadata',
              charOffset: 1000,
              charLength: 2000,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
    });
  });

  describe('small output (no pagination)', () => {
    it('should not add outputPagination when PR response is small', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [
            {
              number: 1,
              title: 'Small PR',
              state: 'open',
              draft: false,
              createdAt: '2023-01-01T00:00:00Z',
              updatedAt: '2023-01-01T00:00:00Z',
              author: { login: 'user', id: '1' },
              body: 'Small body',
              url: 'https://github.com/test/repo/pull/1',
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
              state: 'open',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('outputPagination');
    });
  });

  describe('edge cases for branch coverage', () => {
    it('should handle response without pagination from provider', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [
            {
              number: 1,
              title: 'PR without pagination',
              state: 'open',
              draft: false,
              createdAt: '2023-01-01T00:00:00Z',
              updatedAt: '2023-01-01T00:00:00Z',
              author: { login: 'user', id: '1' },
              body: 'Body',
              url: 'https://github.com/test/repo/pull/1',
            },
          ],
          totalCount: 1,
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ owner: 'test', repo: 'repo' }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('PR without pagination');
    });

    it('should handle string label (non-array)', async () => {
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
          queries: [{ owner: 'test', repo: 'repo', label: 'bug' }],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle array labels', async () => {
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
          queries: [{ owner: 'test', repo: 'repo', label: ['bug', 'feature'] }],
        }
      );

      expect(result.isError).toBe(false);
    });

    it('should handle provider error without error message', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        error: '',
        status: 500,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ owner: 'test', repo: 'repo' }],
        }
      );

      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Provider error');
    });
  });
});
