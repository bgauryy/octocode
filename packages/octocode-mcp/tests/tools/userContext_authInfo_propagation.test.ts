import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockMcpServer,
  type MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn(async (_key: string, fn: () => unknown) => fn()),
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
}));

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

describe('Provider Integration - ALL TOOLS', () => {
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
    vi.clearAllMocks();

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    mockProvider.searchCode.mockResolvedValue({
      data: {
        items: [],
        totalCount: 0,
        pagination: { currentPage: 1, totalPages: 0, hasMore: false },
      },
      status: 200,
      provider: 'github',
    });
    mockProvider.getFileContent.mockResolvedValue({
      data: {
        path: 'test.js',
        content: 'test',
        encoding: 'utf-8',
        size: 4,
        ref: 'main',
      },
      status: 200,
      provider: 'github',
    });
    mockProvider.searchRepos.mockResolvedValue({
      data: {
        items: [],
        totalCount: 0,
        pagination: { currentPage: 1, totalPages: 0, hasMore: false },
      },
      status: 200,
      provider: 'github',
    });
    mockProvider.getRepoStructure.mockResolvedValue({
      data: {
        entries: [],
        path: '',
        repository: { id: '1', name: 'test/repo', url: '' },
        branch: 'main',
        truncated: false,
      },
      status: 200,
      provider: 'github',
    });
    mockProvider.searchPullRequests.mockResolvedValue({
      data: {
        items: [],
        totalCount: 0,
        pagination: { currentPage: 1, totalPages: 0, hasMore: false },
      },
      status: 200,
      provider: 'github',
    });
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('1. github_search_code → provider.searchCode', () => {
    it('should call provider searchCode', async () => {
      registerGitHubSearchCodeTool(mockServer.server);

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywords: ['test'] }],
      });

      expect(mockGetProvider).toHaveBeenCalled();
      expect(mockProvider.searchCode).toHaveBeenCalled();
    });
  });

  describe('2. github_fetch_content → provider.getFileContent', () => {
    it('should call provider getFileContent', async () => {
      registerFetchGitHubFileContentTool(mockServer.server);

      await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            path: 'test.js',
            branch: 'main',
          },
        ],
      });

      expect(mockGetProvider).toHaveBeenCalled();
      expect(mockProvider.getFileContent).toHaveBeenCalled();
    });
  });

  describe('3. github_search_repos → provider.searchRepos', () => {
    it('should call provider searchRepos', async () => {
      registerSearchGitHubReposTool(mockServer.server);

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
        queries: [{ keywords: ['test'] }],
      });

      expect(mockGetProvider).toHaveBeenCalled();
      expect(mockProvider.searchRepos).toHaveBeenCalled();
    });
  });

  describe('4. github_view_repo_structure → provider.getRepoStructure', () => {
    it('should call provider getRepoStructure', async () => {
      registerViewGitHubRepoStructureTool(mockServer.server);

      await mockServer.callTool(TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            branch: 'main',
          },
        ],
      });

      expect(mockGetProvider).toHaveBeenCalled();
      expect(mockProvider.getRepoStructure).toHaveBeenCalled();
    });
  });

  describe('5. github_search_pull_requests → provider.searchPullRequests', () => {
    it('should call provider searchPullRequests', async () => {
      registerSearchGitHubPullRequestsTool(mockServer.server);

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
          },
        ],
      });

      expect(mockGetProvider).toHaveBeenCalled();
      expect(mockProvider.searchPullRequests).toHaveBeenCalled();
    });
  });
});
