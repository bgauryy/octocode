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

type FlatResponse = {
  results: Array<{
    id: string;
    owner: string;
    repo: string;
    matches: Array<{ path: string; value?: string }>;
  }>;
  perQueryPagination?: PerQueryPagination[];
  responsePagination?: Pagination;
  hints?: string[];
  // githubSearchCode no longer emits any warnings (truncation removed); kept
  // as `unknown[]` only so the `toBeUndefined()` guards stay type-clean.
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
    it('omits perQueryPagination and responsePagination for small responses', async () => {
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
      expect(data.perQueryPagination).toBeUndefined();
      expect(data.responsePagination).toBeUndefined();
      expect(data.results).toHaveLength(1);
    });

    it('auto-paginates big responses at the default limit even without an explicit charLength', async () => {
      // A single 20K-char match far exceeds the single output limit (2000).
      // With no explicit pagination knobs the response must STILL be bounded:
      // the unified engine auto-paginates at getOutputCharLimit(), clips the
      // oversized match (with a structured warning + recovery), and exposes a
      // responseCharOffset cursor — never emitting the full 20K body whole.
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
      // Auto-paginated: responsePagination is present with more to fetch.
      expect(data.responsePagination).toBeDefined();
      expect(data.responsePagination!.hasMore).toBe(true);
      // The 20K body was NOT emitted whole — it was windowed to fit the budget.
      const emitted = data.results[0]?.matches[0]?.value?.length ?? 0;
      expect(emitted).toBeLessThan(huge.length);
      // No data is lost and NO truncation marker/warning: the remainder is
      // reachable purely by advancing the responseCharOffset cursor.
      expect(data.warnings).toBeUndefined();
      expect(data.results[0]?.matches[0]?.value).not.toMatch(
        /\[(truncated|clipped)\]/i
      );
      expect(data.hints?.some(h => h.includes('responseCharOffset'))).toBe(
        true
      );
    });
  });

  describe('Query-level perQueryPagination (charLength / charOffset)', () => {
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

    it('slices results to fit charLength and emits perQueryPagination with hasMore=true', async () => {
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
      expect(data.perQueryPagination).toBeDefined();
      expect(data.perQueryPagination).toHaveLength(1);
      const page0 = data.perQueryPagination![0]!;
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

      const firstPage = first.perQueryPagination![0]!;
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

      expect(second.perQueryPagination![0]!.charOffset).toBe(nextOffset);
      const firstPaths = first.results[0]?.matches.map(m => m.path) ?? [];
      const secondPaths = second.results[0]?.matches.map(m => m.path) ?? [];
      // No file is skipped across the page boundary: the union covers all 5,
      // and the second page resumes at or after where the first ended. (A
      // boundary path MAY repeat when a single value is split across pages —
      // that is correct intra-item continuation, not overlap/duplication.)
      const union = new Set([...firstPaths, ...secondPaths]);
      expect(union.size).toBe(5);
      const lastFirst = firstPaths[firstPaths.length - 1];
      expect(secondPaths[0]! >= lastFirst!).toBe(true);
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

      expect(data.perQueryPagination![0]!.hasMore).toBe(false);
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

      expect(data.perQueryPagination).toBeDefined();
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
      expect(data.perQueryPagination).toBeUndefined();
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

      expect(data.perQueryPagination![0]!.hasMore).toBe(false);
    });
  });

  describe('Per-query perQueryPagination across multiple queries', () => {
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

      expect(data.perQueryPagination).toHaveLength(2);
      const qA = data.perQueryPagination!.find(p => p.id === 'qA')!;
      const qB = data.perQueryPagination!.find(p => p.id === 'qB')!;
      expect(qA.hasMore).toBe(true);
      expect(qB.hasMore).toBe(false);
      const alphaGroup = data.results.find(g => g.repo === 'alpha');
      const betaGroup = data.results.find(g => g.repo === 'beta');
      expect(alphaGroup?.matches.length ?? 0).toBeLessThan(4);
      expect(betaGroup?.matches.length).toBe(4);
    });

    it('omits perQueryPagination[] entries for queries without charLength/charOffset', async () => {
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

      expect(data.perQueryPagination).toHaveLength(1);
      expect(data.perQueryPagination![0]!.id).toBe('qB');
    });
  });

  describe('Oversized values are windowed by char pagination, never truncated', () => {
    it('keeps the page within budget and exposes a continuation cursor', async () => {
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
      expect(data.responsePagination!.hasMore).toBe(true);
      // Deterministic bound: the page never exceeds the requested budget by
      // more than a single group wrapper — far tighter than the old ≤2× cap,
      // and certainly never the full 50K.
      expect(data.responsePagination!.charLength).toBeLessThanOrEqual(5_200);
      // NO truncation warnings exist anymore — oversized data is paginated.
      expect(data.warnings).toBeUndefined();
    });

    it('slices an oversized value without a marker and reassembles losslessly across pages', async () => {
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

      const call = (responseCharOffset?: number) =>
        mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_CODE, {
          queries: [{ keywordsToSearch: ['y'], owner: 'owner', repo: 'giant' }],
          responseCharLength: 5_000,
          ...(responseCharOffset !== undefined ? { responseCharOffset } : {}),
        });

      const first = (await call()).structuredContent as FlatResponse;
      const firstValue = first.results[0]?.matches[0]?.value ?? '';
      // No marker, no truncation warning — just a slice plus a cursor.
      expect(firstValue).not.toMatch(/\[(truncated|clipped)\]/i);
      expect(first.warnings).toBeUndefined();
      expect(first.responsePagination!.hasMore).toBe(true);
      expect(first.hints?.some(h => h.includes('responseCharOffset'))).toBe(
        true
      );

      // Walk the responseCharOffset cursor to the end and reassemble.
      let assembled = '';
      let offset = 0;
      for (let i = 0; i < 40; i++) {
        const page = (await call(offset)).structuredContent as FlatResponse;
        assembled += page.results[0]?.matches[0]?.value ?? '';
        const p = page.responsePagination!;
        if (!p.hasMore) break;
        offset = p.charOffset + p.charLength;
      }
      // Every 'Y' is recovered — nothing dropped.
      expect(assembled.length).toBeGreaterThanOrEqual(huge.length);
      expect(/^Y+$/.test(assembled)).toBe(true);
    });
  });
});
