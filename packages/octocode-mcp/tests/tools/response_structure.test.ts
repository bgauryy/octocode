import { getTextContent } from '../utils/testHelpers.js';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetServerConfig = vi.hoisted(() => vi.fn());
const mockGetGitHubToken = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn(async (_key: string, fn: () => unknown) => {
    return await fn();
  }),
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  initialize: vi.fn(),
  getServerConfig: mockGetServerConfig,
  isLoggingEnabled: vi.fn(() => false),
  getGitHubToken: mockGetGitHubToken,
  getActiveProviderConfig: vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  })),
}));

const mockLogToolCall = vi.hoisted(() => vi.fn());
vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logToolCall: mockLogToolCall,
  getSessionManager: vi.fn(() => null),
  SessionManager: vi.fn(),
  logSessionError: vi.fn(),
  logSessionInit: vi.fn(),
  initializeSession: vi.fn(),
  resetSessionManager: vi.fn(),
}));

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from '../../src/tools/github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from '../../src/tools/github_search_repos/github_search_repos.js';
import { registerViewGitHubRepoStructureTool } from '../../src/tools/github_view_repo_structure/github_view_repo_structure.js';
import { registerSearchGitHubPullRequestsTool } from '../../src/tools/github_search_pull_requests/github_search_pull_requests.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

describe('Response Structure Tests - All Tools', () => {
  let mockServer: MockMcpServer;
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

    mockGetServerConfig.mockReturnValue({
      version: '1.0.0',
      timeout: 30000,
      maxRetries: 3,
      loggingEnabled: false,
    });

    mockGetGitHubToken.mockResolvedValue('test-token');

    registerGitHubSearchCodeTool(mockServer.server);
    registerFetchGitHubFileContentTool(mockServer.server);
    registerSearchGitHubReposTool(mockServer.server);
    registerViewGitHubRepoStructureTool(mockServer.server);
    registerSearchGitHubPullRequestsTool(mockServer.server);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('github_search_code', () => {
    it('should return hasResults for matching results', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/index.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [{ context: 'const test = 1;', positions: [] }],
              url: '',
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywords: ['test'], owner: 'test', repo: 'repo' }],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: test/repo');
      expect(responseText).toContain('src/index.ts');
    });

    it('should return empty for no results', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywords: ['nonexistent'], owner: 'test', repo: 'repo' }],
      });

      expect(result.isError).toBe(false);
      const structured = result.structuredContent as { results: unknown[] };
      expect(structured.results).toEqual([]);
    });

    it('should return error for API failure', async () => {
      mockProvider.searchCode.mockResolvedValue({
        error: 'Not found',
        status: 404,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywords: ['test'], owner: 'bad', repo: 'repo' }],
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });

  describe('github_fetch_content', () => {
    it('should return hasResults for found file', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        data: {
          path: 'README.md',
          content: '# Hello World',
          encoding: 'utf-8',
          size: 13,
          ref: 'main',
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            { owner: 'test', repo: 'repo', path: 'README.md', branch: 'main' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('README.md');
      expect(responseText).toContain('# Hello World');
    });

    it('should return error for missing file', async () => {
      mockProvider.getFileContent.mockResolvedValue({
        error: 'File not found',
        status: 404,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              path: 'missing.txt',
              branch: 'main',
            },
          ],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });

  describe('github_search_repos', () => {
    it('should return hasResults for matching repos', async () => {
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
          queries: [{ keywords: ['react'] }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).not.toContain('status: hasResults');
      expect(responseText).not.toContain('status: empty');
      expect(responseText).not.toContain('status: error');
    });

    it('should return empty for no matching repos', async () => {
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
          queries: [{ keywords: ['nonexistent123456'] }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('empty');
    });
  });

  describe('github_view_repo_structure', () => {
    it('should return hasResults for existing repo', async () => {
      mockProvider.getRepoStructure.mockResolvedValue({
        data: {
          projectPath: 'test/repo',
          branch: 'main',
          path: '',
          structure: {
            '.': {
              files: ['README.md', 'package.json'],
              folders: ['src'],
            },
          },
          summary: {
            totalFiles: 2,
            totalFolders: 1,
            truncated: false,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        {
          queries: [{ owner: 'test', repo: 'repo', branch: 'main' }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('README.md');
    });

    it('should return error for non-existent repo', async () => {
      mockProvider.getRepoStructure.mockResolvedValue({
        error: 'Repository not found',
        status: 404,
        provider: 'github',
      });

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        {
          queries: [{ owner: 'nonexistent', repo: 'repo', branch: 'main' }],
        }
      );

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });

  describe('github_search_pull_requests', () => {
    it('should return hasResults for matching PRs', async () => {
      mockProvider.searchPullRequests.mockResolvedValue({
        data: {
          items: [
            {
              id: 123,
              number: 123,
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
              head: { ref: 'feature', sha: 'abc123' },
              base: { ref: 'main', sha: 'def456' },
              body: 'Description',
              comments: 0,
              reviewComments: 0,
              additions: 10,
              deletions: 5,
              changedFiles: 2,
              url: 'https://github.com/test/repo/pull/123',
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
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('Test PR');
    });

    it('should return empty for no matching PRs', async () => {
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
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('empty');
    });
  });
});
