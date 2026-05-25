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

type Pagination = {
  charOffset: number;
  charLength: number;
  totalChars: number;
  hasMore: boolean;
  currentPage: number;
  totalPages: number;
};

type PerQueryPagination = Pagination & { id: string };

type Warning =
  | {
      kind: 'match-value-truncated';
      groupId: string;
      path: string;
      fullValueLength: number;
      truncatedAt: number;
      recovery: string;
    }
  | {
      kind: 'content-truncated';
      groupId: string;
      path: string;
      fullContentLength: number;
      truncatedAt: number;
      recovery: string;
    };

type FlatResponse = {
  results: Array<{
    id: string;
    owner: string;
    repo: string;
    matches: Array<{ path: string; value?: string }>;
  }>;
  outputPagination?: PerQueryPagination[];
  responsePagination?: Pagination;
  hints?: string[];
  warnings?: Warning[];
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

describe('GitHub Search Code Tool - Char-Level Pagination', () => {
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

  describe('No pagination metadata when response fits', () => {
    it('omits outputPagination and responsePagination for small responses', async () => {
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
      expect(data.outputPagination).toBeUndefined();
      expect(data.responsePagination).toBeUndefined();
      expect(data.results).toHaveLength(1);
    });

    it('omits pagination when no charLength/responseCharLength is supplied even for big responses', async () => {
      const huge = 'X'.repeat(20_000);
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [makeItem('owner/repo', 'src/big.ts', huge)],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [{ keywordsToSearch: ['big'], owner: 'owner', repo: 'repo' }],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.outputPagination).toBeUndefined();
      expect(data.responsePagination).toBeUndefined();
      expect(data.results[0]?.matches[0]?.value?.length).toBe(huge.length);
    });
  });

  describe('Query-level outputPagination (charLength / charOffset)', () => {
    function setupPaginatedFixture() {
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
    }

    it('slices results to fit charLength and emits outputPagination with hasMore=true', async () => {
      setupPaginatedFixture();

      const result = await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
        queries: [
          {
            keywordsToSearch: ['x'],
            owner: 'owner',
            repo: 'repo',
            charLength: 120,
          },
        ],
      });

      const data = result.structuredContent as FlatResponse;
      expect(data.outputPagination).toBeDefined();
      expect(data.outputPagination).toHaveLength(1);
      const page0 = data.outputPagination![0]!;
      expect(page0.charOffset).toBe(0);
      // charLength reports the actually-consumed bytes so callers can use
      // nextOffset = charOffset + charLength to advance.
      expect(page0.charLength).toBeGreaterThan(0);
      expect(page0.charLength).toBeLessThanOrEqual(160);
      expect(page0.hasMore).toBe(true);
      // At least one but fewer than all five matches
      const matchCount = data.results[0]?.matches.length ?? 0;
      expect(matchCount).toBeGreaterThan(0);
      expect(matchCount).toBeLessThan(5);
    });

    it('continues with charOffset to return remaining content', async () => {
      setupPaginatedFixture();

      const first = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['x'],
              owner: 'owner',
              repo: 'repo',
              charLength: 120,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      const firstPage = first.outputPagination![0]!;
      const nextOffset = firstPage.charOffset + firstPage.charLength;

      const second = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['x'],
              owner: 'owner',
              repo: 'repo',
              charLength: 120,
              charOffset: nextOffset,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(second.outputPagination![0]!.charOffset).toBe(nextOffset);
      const firstPaths = first.results[0]?.matches.map(m => m.path) ?? [];
      const secondPaths = second.results[0]?.matches.map(m => m.path) ?? [];
      // Pages do not overlap
      expect(secondPaths.some(p => firstPaths.includes(p))).toBe(false);
    });

    it('sets hasMore=false on the final page', async () => {
      setupPaginatedFixture();

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['x'],
              owner: 'owner',
              repo: 'repo',
              charLength: 10_000,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(data.outputPagination![0]!.hasMore).toBe(false);
      expect(data.results[0]?.matches).toHaveLength(5);
    });

    it('includes a continuation hint when paginated', async () => {
      setupPaginatedFixture();

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['x'],
              owner: 'owner',
              repo: 'repo',
              charLength: 120,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(data.hints?.some(h => h.includes('charOffset'))).toBe(true);
    });
  });

  describe('Bulk responsePagination (responseCharLength / responseCharOffset)', () => {
    function setupTwoQueries() {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/one', 'src/one.ts', 'body-one')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/two', 'src/two.ts', 'body-two')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });
    }

    it('slices merged groups across queries to fit responseCharLength', async () => {
      setupTwoQueries();

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            { keywordsToSearch: ['a'], owner: 'owner', repo: 'one' },
            { keywordsToSearch: ['b'], owner: 'owner', repo: 'two' },
          ],
          responseCharLength: 120,
        })
      ).structuredContent as FlatResponse;

      expect(data.responsePagination).toBeDefined();
      expect(data.responsePagination!.charLength).toBeGreaterThan(0);
      expect(data.responsePagination!.hasMore).toBe(true);
      // First page returns the first merged group only
      expect(data.results).toHaveLength(1);
      expect(data.results[0]?.id).toBe('owner/one');
    });

    it('continues with responseCharOffset to return the next group', async () => {
      setupTwoQueries();

      const first = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            { keywordsToSearch: ['a'], owner: 'owner', repo: 'one' },
            { keywordsToSearch: ['b'], owner: 'owner', repo: 'two' },
          ],
          responseCharLength: 120,
        })
      ).structuredContent as FlatResponse;

      // Re-prime mocks (each callTool consumes one mockResolvedValueOnce).
      setupTwoQueries();

      const nextOffset =
        first.responsePagination!.charOffset +
        first.responsePagination!.charLength;

      const second = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            { keywordsToSearch: ['a'], owner: 'owner', repo: 'one' },
            { keywordsToSearch: ['b'], owner: 'owner', repo: 'two' },
          ],
          responseCharLength: 120,
          responseCharOffset: nextOffset,
        })
      ).structuredContent as FlatResponse;

      expect(second.results.map(r => r.id)).not.toContain('owner/one');
      expect(second.results.map(r => r.id)).toContain('owner/two');
    });

    it('emits a continuation hint for responsePagination', async () => {
      setupTwoQueries();

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            { keywordsToSearch: ['a'], owner: 'owner', repo: 'one' },
            { keywordsToSearch: ['b'], owner: 'owner', repo: 'two' },
          ],
          responseCharLength: 120,
        })
      ).structuredContent as FlatResponse;

      expect(data.hints?.some(h => h.includes('responseCharOffset'))).toBe(
        true
      );
    });
  });

  describe('Combined output + response pagination', () => {
    it('emits both pagination metadata fields when both knobs are supplied', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: Array.from({ length: 6 }, (_, i) =>
            makeItem('owner/repo', `src/f-${i + 1}.ts`, `payload-${i + 1}`)
          ),
          totalCount: 6,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['x'],
              owner: 'owner',
              repo: 'repo',
              charLength: 200,
            },
          ],
          responseCharLength: 150,
        })
      ).structuredContent as FlatResponse;

      expect(data.outputPagination).toBeDefined();
      expect(data.responsePagination).toBeDefined();
    });
  });

  describe('Edge cases', () => {
    it('returns empty pagination metadata when there are zero matches', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [],
          totalCount: 0,
          pagination: { currentPage: 1, totalPages: 0, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['nothing'],
              owner: 'owner',
              repo: 'repo',
              charLength: 100,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(data.results).toEqual([]);
      expect(data.outputPagination).toBeUndefined();
    });

    it('clamps charOffset past totalChars to the last page', async () => {
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [makeItem('owner/repo', 'a.ts', 'tiny')],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              keywordsToSearch: ['tiny'],
              owner: 'owner',
              repo: 'repo',
              charLength: 50,
              charOffset: 9_999_999,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(data.outputPagination![0]!.hasMore).toBe(false);
    });
  });

  describe('Per-query outputPagination across multiple queries', () => {
    it('honors per-query charLength independently for each query', async () => {
      // Two queries, each hits a separate repo with 4 large files.
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: Array.from({ length: 4 }, (_, i) =>
              makeItem('owner/alpha', `src/a-${i}.ts`, `alpha-body-${i}`)
            ),
            totalCount: 4,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: Array.from({ length: 4 }, (_, i) =>
              makeItem('owner/beta', `src/b-${i}.ts`, `beta-body-${i}`)
            ),
            totalCount: 4,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            {
              id: 'qA',
              keywordsToSearch: ['a'],
              owner: 'owner',
              repo: 'alpha',
              charLength: 80,
            },
            {
              id: 'qB',
              keywordsToSearch: ['b'],
              owner: 'owner',
              repo: 'beta',
              charLength: 10_000, // unlimited
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(data.outputPagination).toHaveLength(2);
      const qA = data.outputPagination!.find(p => p.id === 'qA')!;
      const qB = data.outputPagination!.find(p => p.id === 'qB')!;
      expect(qA.hasMore).toBe(true);
      expect(qB.hasMore).toBe(false);
      const alphaGroup = data.results.find(g => g.repo === 'alpha');
      const betaGroup = data.results.find(g => g.repo === 'beta');
      expect(alphaGroup?.matches.length ?? 0).toBeLessThan(4);
      expect(betaGroup?.matches.length).toBe(4);
    });

    it('omits outputPagination[] entries for queries without charLength/charOffset', async () => {
      mockProvider.searchCode
        .mockResolvedValueOnce({
          data: {
            items: [makeItem('owner/one', 'a.ts', 'tiny-a')],
            totalCount: 1,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        })
        .mockResolvedValueOnce({
          data: {
            items: Array.from({ length: 3 }, (_, i) =>
              makeItem('owner/two', `b-${i}.ts`, `body-${i}`)
            ),
            totalCount: 3,
            pagination: { currentPage: 1, totalPages: 1, hasMore: false },
          },
          status: 200,
          provider: 'github',
        });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [
            // no charLength on first query — should not produce a pagination entry
            { id: 'qA', keywordsToSearch: ['a'], owner: 'owner', repo: 'one' },
            {
              id: 'qB',
              keywordsToSearch: ['b'],
              owner: 'owner',
              repo: 'two',
              charLength: 60,
            },
          ],
        })
      ).structuredContent as FlatResponse;

      expect(data.outputPagination).toHaveLength(1);
      expect(data.outputPagination![0]!.id).toBe('qB');
    });
  });

  describe('Hybrid escape valve for oversized atomic groups', () => {
    it('falls back to match-level slicing when next group > 2x charLength', async () => {
      // One group, one huge match — far larger than the bulk budget.
      const huge = 'X'.repeat(50_000);
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [
            makeItem('owner/giant', 'src/giant.ts', huge),
            makeItem('owner/giant', 'src/extra-a.ts', 'small-a'),
            makeItem('owner/giant', 'src/extra-b.ts', 'small-b'),
          ],
          totalCount: 3,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [{ keywordsToSearch: ['x'], owner: 'owner', repo: 'giant' }],
          responseCharLength: 5_000,
        })
      ).structuredContent as FlatResponse;

      expect(data.responsePagination).toBeDefined();
      // Escape-valve kicked in: the single oversized group is split at the
      // match level, so this page only carries some of its matches.
      const matchCount = data.results[0]?.matches.length ?? 0;
      expect(matchCount).toBeGreaterThan(0);
      expect(matchCount).toBeLessThan(3);
      expect(data.responsePagination!.hasMore).toBe(true);
      // The consumed-chars contract still holds even under the escape valve.
      expect(data.responsePagination!.charLength).toBeLessThanOrEqual(
        2 * 5_000
      );
    });

    it('emits a structured warning when a single match value is truncated', async () => {
      // Single oversized match — triggers the second-level escape valve.
      const huge = 'Y'.repeat(50_000);
      mockProvider.searchCode.mockResolvedValue({
        data: {
          items: [makeItem('owner/giant', 'src/giant.ts', huge)],
          totalCount: 1,
          pagination: { currentPage: 1, totalPages: 1, hasMore: false },
        },
        status: 200,
        provider: 'github',
      });

      const data = (
        await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [{ keywordsToSearch: ['y'], owner: 'owner', repo: 'giant' }],
          responseCharLength: 5_000,
        })
      ).structuredContent as FlatResponse;

      expect(data.warnings).toBeDefined();
      const truncWarning = data.warnings!.find(
        w => w.kind === 'match-value-truncated'
      );
      expect(truncWarning).toBeDefined();
      expect(truncWarning!.groupId).toBe('owner/giant');
      expect(truncWarning!.path).toBe('src/giant.ts');
      expect(truncWarning!.fullValueLength).toBe(50_000);
      expect(truncWarning!.truncatedAt).toBeLessThan(50_000);
      expect(truncWarning!.recovery).toMatch(
        /responseCharLength|larger budget/i
      );
    });
  });
});
