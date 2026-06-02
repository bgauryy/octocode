import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
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
  getServerConfig: vi.fn(() => ({
    version: '1.0.0',
    timeout: 30000,
    maxRetries: 3,
    loggingEnabled: false,
  })),
}));

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

describe('GitHub Search Code Tool - Tool Layer Integration', () => {
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

    registerGitHubSearchCodeTool(mockServer.server);
    vi.clearAllMocks();
    mockGetProvider.mockReturnValue(mockProvider);
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('Status: hasResults', () => {
    it('rejects an empty search before calling the provider', async () => {
      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: [] }],
      });

      const responseText = getTextContent(result.content);
      expect(responseText).toContain(
        'At least one search term or scope filter is required'
      );
      expect(mockProvider.searchCode).not.toHaveBeenCalled();
    });

    it('should return hasResults status when API returns items', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/index.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [{ context: 'const test = 1;', positions: [] }],
              url: '',
            },
            {
              path: 'src/utils.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [
                { context: 'export function util() {}', positions: [] },
              ],
              url: '',
            },
          ],
          totalCount: 2,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('id: "test/repo"');
      expect(responseText).toContain('src/index.ts');
      expect(responseText).toContain('src/utils.ts');
    });

    it('should include owner and repo fields in each file result', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/index.ts',
              repository: { id: '1', name: 'facebook/react', url: '' },
              matches: [{ context: 'const test = 1;', positions: [] }],
              url: '',
            },
            {
              path: 'src/utils.ts',
              repository: { id: '2', name: 'vercel/next', url: '' },
              matches: [
                { context: 'export function util() {}', positions: [] },
              ],
              url: '',
            },
          ],
          totalCount: 2,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: ['test'] }],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('owner: "facebook"');
      expect(responseText).toContain('repo: "react"');
      expect(responseText).toContain('owner: "vercel"');
      expect(responseText).toContain('repo: "next"');
    });
  });

  describe('Status: empty', () => {
    it('should return empty results when no items found', async () => {
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
        queries: [
          {
            keywordsToSearch: ['nonexistent'],
            owner: 'test',
            repo: 'repo',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const structured = result.structuredContent as { results: unknown[] };
      expect(structured.results).toEqual([]);
    });
  });

  describe('Status: error', () => {
    it('should return error status when API fails', async () => {
      mockProvider.searchCode.mockResolvedValue({
        error: 'Not found',
        status: 404,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'nonexistent',
            repo: 'repo',
          },
        ],
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });

    it('should handle rate limit error', async () => {
      mockProvider.searchCode.mockResolvedValue({
        error: 'Rate limit exceeded',
        status: 403,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: ['test'] }],
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });

  describe('Bulk queries', () => {
    it('should handle multiple queries', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                path: 'file1.ts',
                repository: { id: '1', name: 'test/repo', url: '' },
                matches: [{ context: 'match1', positions: [] }],
                url: '',
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
            items: [
              {
                path: 'file2.ts',
                repository: { id: '1', name: 'test/repo', url: '' },
                matches: [{ context: 'match2', positions: [] }],
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
        queries: [
          { keywordsToSearch: ['test1'], owner: 'test', repo: 'repo' },
          { keywordsToSearch: ['test2'], owner: 'test', repo: 'repo' },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('file1.ts');
      expect(responseText).toContain('file2.ts');
    });

    it('should handle mixed success and error results', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [
              {
                path: 'success.ts',
                repository: { id: '1', name: 'test/repo', url: '' },
                matches: [],
                url: '',
              },
            ],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          error: 'Not found',
          status: 404,
          provider: 'github',
        });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          { keywordsToSearch: ['test1'], owner: 'test', repo: 'repo' },
          { keywordsToSearch: ['test2'], owner: 'bad', repo: 'repo' },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('success.ts');
      expect(responseText).toContain('error');
    });
  });

  describe('Pagination', () => {
    it('should handle paginated results', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/file.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
              url: '',
            },
          ],
          totalCount: 100,
          pagination: { currentPage: 1, totalPages: 10, hasMore: true },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
            page: 1,
            limit: 10,
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('file.ts');
    });
  });

  describe('Search filters', () => {
    it('should handle extension filter', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/file.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
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
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
            extension: 'ts',
          },
        ],
      });

      expect(result.isError).toBe(false);
    });

    it('should handle filename filter', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/index.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
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
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
            filename: 'index.ts',
          },
        ],
      });

      expect(result.isError).toBe(false);
    });

    it('should handle path filter', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/utils/helper.ts',
              repository: { id: '1', name: 'test/repo', url: '' },
              matches: [],
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
        queries: [
          {
            keywordsToSearch: ['test'],
            owner: 'test',
            repo: 'repo',
            path: 'src/utils',
          },
        ],
      });

      expect(result.isError).toBe(false);
    });
  });

  describe('Owner-only search (no repo)', () => {
    it('should pass owner to provider when only owner is specified', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/widget.ts',
              repository: { id: '1', name: 'wix-private/payments', url: '' },
              matches: [{ context: 'refund widget', positions: [] }],
              url: '',
            },
          ],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['refund', 'widget'],
            owner: 'wix-private',
          },
        ],
      });

      expect(mockProvider.searchCode).toHaveBeenCalledTimes(1);
      const providerQuery = mockProvider.searchCode.mock.calls[0]?.[0];
      expect(providerQuery.owner).toBe('wix-private');
    });

    it('should return results when searching by owner only', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            {
              path: 'src/refund.ts',
              repository: { id: '1', name: 'wix-private/billing', url: '' },
              matches: [{ context: 'process refund', positions: [] }],
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
        queries: [
          {
            keywordsToSearch: ['refund'],
            owner: 'wix-private',
          },
        ],
      });

      expect(result.isError).toBe(false);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('owner: "wix-private"');
      expect(responseText).toContain('src/refund.ts');
    });
  });

  describe('Exception handling', () => {
    it('should handle exception thrown by provider', async () => {
      mockProvider.searchCode.mockRejectedValue(new Error('Network error'));

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: ['test'], owner: 'test', repo: 'repo' }],
      });

      expect(result.isError).toBe(true);
      const responseText = getTextContent(result.content);
      expect(responseText).toContain('error');
    });
  });
});
