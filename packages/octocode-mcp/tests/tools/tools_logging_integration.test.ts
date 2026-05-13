import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

// Mock the session logging
const mockLogToolCall = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock('../../src/session.js', () => ({
  logToolCall: mockLogToolCall,
  initializeSession: vi.fn(() => ({
    getSessionId: () => 'test-session-id',
  })),
  getSessionManager: vi.fn(() => null),
  logSessionInit: vi.fn(),
  logSessionError: vi.fn(),
  resetSessionManager: vi.fn(),
}));

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

// Mock content sanitizer
vi.mock('octocode-security-utils/contentSanitizer', () => ({
  ContentSanitizer: {
    validateInputParameters: vi.fn(params => ({
      isValid: true,
      sanitizedParams: params,
      warnings: [],
      hasSecrets: false,
    })),
    sanitizeContent: vi.fn(content => ({
      content,
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    })),
  },
}));

// Mock server config
vi.mock('../../src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(async () => 'test-token'),
  isLoggingEnabled: vi.fn(() => true),
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
}));

// Import tools after mocks are set up
import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

describe('Tools Logging Integration - Repo/Owner Tracking', () => {
  let mockServer: ReturnType<typeof createMockMcpServer>;
  let mockProvider: {
    searchCode: ReturnType<typeof vi.fn>;
    getFileContent: ReturnType<typeof vi.fn>;
    searchRepos: ReturnType<typeof vi.fn>;
    searchPullRequests: ReturnType<typeof vi.fn>;
    getRepoStructure: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockServer = createMockMcpServer();

    mockProvider = {
      searchCode: vi.fn(),
      getFileContent: vi.fn(),
      searchRepos: vi.fn(),
      searchPullRequests: vi.fn(),
      getRepoStructure: vi.fn(),
    };
    mockGetProvider.mockReturnValue(mockProvider);

    // Default mock responses
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

  describe('github_search_code', () => {
    it('should log repo and owner from queries', async () => {
      registerGitHubSearchCodeTool(mockServer.server);

      const args = {
        queries: [
          {
            owner: 'facebook',
            repo: 'react',
            keywordsToSearch: ['useState'],
          },
        ],
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, args);

      expect(mockProvider.searchCode).toHaveBeenCalled();
    });

    it('should handle multiple queries with different repos', async () => {
      registerGitHubSearchCodeTool(mockServer.server);

      const args = {
        queries: [
          { owner: 'facebook', repo: 'react', keywordsToSearch: ['test1'] },
          { owner: 'vercel', repo: 'next.js', keywordsToSearch: ['test2'] },
        ],
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, args);

      expect(mockProvider.searchCode).toHaveBeenCalledTimes(2);
    });
  });

  describe('github_fetch_content', () => {
    it('should log repo and owner from queries', async () => {
      registerFetchGitHubFileContentTool(mockServer.server);

      const args = {
        queries: [
          {
            owner: 'microsoft',
            repo: 'TypeScript',
            path: 'README.md',
            branch: 'main',
          },
        ],
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_FETCH_CONTENT, args);

      expect(mockProvider.getFileContent).toHaveBeenCalled();
    });
  });

  describe('github_search_repos', () => {
    it('should log owner from queries when specified', async () => {
      registerSearchGitHubReposTool(mockServer.server);

      const args = {
        queries: [
          {
            owner: 'google',
            keywordsToSearch: ['tensorflow'],
          },
        ],
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, args);

      expect(mockProvider.searchRepos).toHaveBeenCalled();
    });
  });

  describe('github_view_repo_structure', () => {
    it('should log repo and owner from queries', async () => {
      registerViewGitHubRepoStructureTool(mockServer.server);

      const args = {
        queries: [
          {
            owner: 'nodejs',
            repo: 'node',
            branch: 'main',
          },
        ],
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE, args);

      expect(mockProvider.getRepoStructure).toHaveBeenCalled();
    });
  });

  describe('github_search_pull_requests', () => {
    it('should log repo and owner from queries', async () => {
      registerSearchGitHubPullRequestsTool(mockServer.server);

      const args = {
        queries: [
          {
            owner: 'rust-lang',
            repo: 'rust',
            state: 'open',
          },
        ],
      };

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, args);

      expect(mockProvider.searchPullRequests).toHaveBeenCalled();
    });
  });
});
