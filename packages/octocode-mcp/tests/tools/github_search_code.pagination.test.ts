import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';

const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
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
  })),
}));

import { registerGitHubSearchCodeTool } from '../../src/tools/github_search_code/github_search_code.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

type CodeFile = {
  id: string;
  queryId?: string;
  owner: string;
  repo: string;
  path: string;
  matches: Array<{ value?: string }>;
};

type FlatResponse = {
  results: Array<{
    id: string;
    data?: { files?: CodeFile[] };
  }>;
  pagination?: {
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
    totalMatches?: number;
  };
  hints?: string[];
  warnings?: unknown[];
  errors?: Array<{ id: string; error: string; hints?: string[] }>;
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
        queries: [{ keywords: ['short'], owner: 'owner', repo: 'repo' }],
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
        queries: [{ keywords: ['x'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      const files = data.results[0]?.data?.files ?? [];
      expect(files.length).toBeGreaterThanOrEqual(1);
      expect(files[0]?.matches.length).toBeGreaterThanOrEqual(1);
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
        queries: [{ keywords: ['x'], owner: 'owner', repo: 'repo' }],
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
        queries: [{ keywords: ['x'], owner: 'owner', repo: 'repo', page: 2 }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results).toBeDefined();
      expect(data.errors).toBeUndefined();
    });

    it('hints when GitHub caps reachable results below totalMatches', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 20 }, (_, i) =>
            makeItem('owner/repo', `src/file-${i + 1}.ts`, `body-${i + 1}`)
          ),
          totalCount: 446,
          pagination: {
            currentPage: 1,
            totalPages: 10,
            entriesPerPage: 20,
            hasMore: true,
            totalMatches: 446,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywords: ['x'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      const capHint = data.hints?.find(h =>
        h.includes('GitHub caps code-search at 200 results')
      );
      expect(capHint).toBeDefined();
      expect(capHint).toContain('246 of 446 reported matches are unreachable');
      expect(capHint).toContain('narrow with path/extension/filename');
    });

    it('emits no cap hint when all reported matches are reachable', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 20 }, (_, i) =>
            makeItem('owner/repo', `src/file-${i + 1}.ts`, `body-${i + 1}`)
          ),
          totalCount: 60,
          pagination: {
            currentPage: 1,
            totalPages: 3,
            entriesPerPage: 20,
            hasMore: true,
            totalMatches: 60,
          },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywords: ['x'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.hints?.some(h => h.includes('caps code-search'))).toBeFalsy();
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
          { keywords: ['aQuery'], owner: 'owner', repo: 'repoA' },
          { keywords: ['bQuery'], owner: 'owner', repo: 'repoB' },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.results.length).toBeGreaterThanOrEqual(1);
      const files = data.results[0]?.data?.files ?? [];
      expect(files.map(file => file.queryId)).toEqual(['q1', 'q2']);
      expect(data.errors).toBeUndefined();
    });

    it('keeps same-repository matches separated by queryId', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/repo', 'src/a.ts', 'body-a')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/repo', 'src/b.ts', 'body-b')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            id: 'first',
            keywords: ['a'],
            owner: 'owner',
            repo: 'repo',
          },
          {
            id: 'second',
            keywords: ['b'],
            owner: 'owner',
            repo: 'repo',
          },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      const files = data.results[0]?.data?.files ?? [];
      expect(files).toHaveLength(2);
      expect(files.map(file => file.queryId)).toEqual(['first', 'second']);
      expect(files.map(file => file.path)).toEqual(['src/a.ts', 'src/b.ts']);
    });

    it('rejects repo without owner before calling the provider', async () => {
      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ id: 'missing-owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(mockProvider.searchCode).not.toHaveBeenCalled();
      expect(result.isError).toBe(true);
      expect(data.errors?.[0]).toMatchObject({
        id: 'missing-owner',
        error: expect.stringContaining('Repository scope requires owner'),
      });
      expect(data.errors?.[0]?.hints?.[0]).toContain('owner=');
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
          { keywords: ['good'], owner: 'owner', repo: 'repo' },
          { keywords: ['bad'], owner: 'other', repo: 'repo' },
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
        queries: [{ keywords: ['nonExistent'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.errors).toBeUndefined();
      expect(data.hints).toBeDefined();
    });
  });
});
