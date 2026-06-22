import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { getTextContent } from '../utils/testHelpers.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

const mockWithCache = vi.hoisted(() => vi.fn());
const mockGenerateCacheKey = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withCache: mockWithCache,
  clearAllCache: vi.fn(),
}));

vi.mock('../../../octocode-tools-core/src/tools/utils/tokenManager.js', () => ({
  getGitHubToken: mockGetGitHubToken,
}));

vi.mock('../../../octocode-tools-core/src/github/client.js');

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
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
  })),
}));

import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

describe('GitHub Pull Requests Tool - Branch Coverage', () => {
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

  describe('Per-query validation in execution', () => {
    it('should return error when no valid search params provided', async () => {
      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              state: 'open',
            },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('At least one valid search parameter');
      expect(mockProvider.searchPullRequests).not.toHaveBeenCalled();
    });

    it('should gracefully degrade: valid query succeeds while invalid fails', async () => {
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
            { owner: 'test', repo: 'repo', keywordsToSearch: ['valid'] },
            { state: 'open' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('At least one valid search parameter');
      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
    });
  });

  describe('Execution Error Handling', () => {
    describe('Try/catch error handling (line 131)', () => {
      it('should catch and handle errors thrown during execution', async () => {
        mockProvider.searchPullRequests.mockRejectedValue(
          new Error('Unexpected provider error')
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

        expect(result.isError).toBe(true);
        const responseText = getTextContent(result.content);
        expect(responseText).toContain('error');
      });

      it('should handle errors when getProvider throws', async () => {
        mockGetProvider.mockImplementation(() => {
          throw new Error('Provider initialization failed');
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

        expect(result.isError).toBe(true);
        const responseText = getTextContent(result.content);
        expect(responseText).toContain('error');
      });
    });
  });

  describe('PR Content Fetcher Error Handling', () => {
    describe('fetchCommitFilesAPI catch block (line 234)', () => {
      it('should handle getCommit errors gracefully by returning null', async () => {
        const { transformPullRequestItemFromREST } =
          await import('../../../octocode-tools-core/src/github/prContentFetcher.js');

        const getCommitMock = vi
          .fn()
          .mockRejectedValue(new Error('Failed to fetch commit'));

        const mockOctokit = {
          rest: {
            pulls: {
              listCommits: vi.fn().mockResolvedValue({
                data: [
                  {
                    sha: 'commit-sha-1',
                    commit: {
                      message: 'Test commit',
                      author: {
                        name: 'Test Author',
                        date: '2023-01-01T00:00:00Z',
                      },
                    },
                  },
                ],
              }),
            },
            repos: {
              getCommit: getCommitMock,
            },
          },
        };

        vi.mocked(getOctokit).mockResolvedValue(
          mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
        );

        const prItem = {
          number: 123,
          title: 'Test PR',
          state: 'open',
          draft: false,
          user: { login: 'testuser' },
          created_at: '2023-01-01T00:00:00Z',
          updated_at: '2023-01-01T00:00:00Z',
          closed_at: null,
          merged_at: null,
          html_url: 'https://github.com/test/repo/pull/123',
          head: { ref: 'feature', sha: 'head-sha' },
          base: { ref: 'main', sha: 'base-sha' },
          additions: 10,
          deletions: 5,
          changed_files: 2,
        };

        const result = await transformPullRequestItemFromREST(
          prItem as any,
          {
            owner: 'test',
            repo: 'repo',
            content: { commits: { list: true, includeFiles: true } },
          },
          mockOctokit as any,
          undefined
        );

        expect(getCommitMock).toHaveBeenCalled();
        expect(getCommitMock).toHaveBeenCalledWith({
          owner: 'test',
          repo: 'repo',
          ref: 'commit-sha-1',
        });

        expect(result).toBeDefined();
      });
    });
  });
});
