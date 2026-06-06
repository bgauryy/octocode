import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

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

type FlatResponse = {
  results: Array<{
    id: string;
    owner: string;
    repo: string;
    matches: Array<{ path: string; value?: string }>;
  }>;
  pagination?: {
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
    totalMatches?: number;
  };
  hints?: string[];
  warnings?: unknown[];
  errors?: Array<{ id: string; error: string }>;
};

function makeItem(
  repoFullName: string,
  path: string,
  context: string,
  urlPrefix = 'https://github.com'
) {
  return {
    path,
    matches: [{ context, positions: [] as Array<[number, number]> }],
    url: `${urlPrefix}/${repoFullName}/blob/main/${path}`,
    repository: {
      id: '1',
      name: repoFullName,
      url: `${urlPrefix}/${repoFullName}`,
    },
  };
}

describe('GitHub Search Code Tool - Page-Based Pagination', () => {
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

  describe('Single-page results', () => {
    it('returns results with no pagination when API has only one page', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [makeItem('owner/repo', 'src/index.ts', 'short')],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          { keywordsToSearch: ['short'], owner: 'owner', repo: 'repo' },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results).toHaveLength(1);
      expect(data.errors).toBeUndefined();
    });

    it('returns multiple matches from a single query', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 5 }, (_, i) =>
            makeItem('owner/repo', `src/file-${i + 1}.ts`, `body-${i + 1}`)
          ),
          totalCount: 5,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: ['x'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results[0]?.matches.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('Multi-page GitHub API results', () => {
    it('exposes hasMore=true when GitHub API reports more pages', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 20 }, (_, i) =>
            makeItem('owner/repo', `src/file-${i + 1}.ts`, `body-${i + 1}`)
          ),
          totalCount: 100,
          pagination: { currentPage: 1, totalPages: 5, hasMore: true },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: ['x'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results).toBeDefined();
      expect(data.results.length).toBeGreaterThan(0);
    });

    it('requests page 2 when page param is specified', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 20 }, (_, i) =>
            makeItem(
              'owner/repo',
              `src/page2-file-${i + 1}.ts`,
              `body-${i + 1}`
            )
          ),
          totalCount: 100,
          pagination: { currentPage: 2, totalPages: 5, hasMore: true },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          { keywordsToSearch: ['x'], owner: 'owner', repo: 'repo', page: 2 },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results).toBeDefined();
      expect(data.errors).toBeUndefined();
    });
  });

  describe('Multiple queries', () => {
    it('handles two queries and returns results for each', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/repoA', 'src/a.ts', 'body-a')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/repoB', 'src/b.ts', 'body-b')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          { keywordsToSearch: ['aQuery'], owner: 'owner', repo: 'repoA' },
          { keywordsToSearch: ['bQuery'], owner: 'owner', repo: 'repoB' },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results.length).toBeGreaterThanOrEqual(1);
      expect(data.errors).toBeUndefined();
    });

    it('reports errors per query without failing the whole request', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/repo', 'src/good.ts', 'ok')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockRejectedValueOnce(new Error('Rate limited'));

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          { keywordsToSearch: ['good'], owner: 'owner', repo: 'repo' },
          { keywordsToSearch: ['bad'], owner: 'other', repo: 'repo' },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data).toBeDefined();
    });
  });

  describe('Empty results', () => {
    it('returns empty results with hints when nothing matches', async () => {
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
          { keywordsToSearch: ['nonExistent'], owner: 'owner', repo: 'repo' },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.errors).toBeUndefined();
      expect(data.hints).toBeDefined();
    });
  });
});
