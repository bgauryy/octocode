/**
 * Branch-coverage tests for
 * src/tools/github_search_pull_requests/execution.ts
 *
 * Targets previously-uncovered branches:
 *  - 86,88        concise verbosity caps limit to CONCISE_PR_LIMIT (limit > 3)
 *  - 94,95,97,99  type coercion under concise (explicit non-metadata type,
 *                 omitted type, prNumber+explicit type opt-out)
 *  - 102          partialContentMetadata deletion under concise coercion
 *  - 120          hasValidParams via query.query.trim()
 *  - 197,199      large-file detection via changedFilesCount vs fileChanges
 *  - 211,213      Math.max maxFiles via changedFilesCount vs fileChanges
 *  - 313,324      applyGithubSearchPullRequestsVerbosity concise branch
 *  - 333,336      applyGithubSearchPullRequestsVerbosity compact branch
 */
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
import { applyGithubSearchPullRequestsVerbosity } from '../../src/tools/github_search_pull_requests/execution.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';

function basePR(overrides: Record<string, unknown> = {}) {
  return {
    id: 456,
    number: 456,
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
    head: { ref: 'feature-branch', sha: 'abc123' },
    base: { ref: 'main', sha: 'def456' },
    body: 'Test PR description',
    comments: 0,
    reviewComments: 0,
    additions: 10,
    deletions: 5,
    changedFiles: 2,
    url: 'https://github.com/test/repo/pull/456',
    repository: { id: '1', name: 'test/repo', url: '' },
    ...overrides,
  };
}

function providerResponse(items: Array<Record<string, unknown>>) {
  return {
    data: {
      items,
      totalCount: items.length,
      pagination: {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        totalMatches: items.length,
      },
    },
    status: 200,
    provider: 'github',
  };
}

describe('github_search_pull_requests execution — branch coverage', () => {
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

  describe('concise verbosity pre-flight caps (lines 86,88,94,95,97,99,102)', () => {
    it('caps limit to CONCISE_PR_LIMIT and coerces explicit non-metadata type, dropping partialContentMetadata', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR()])
      );
      const query = {
        owner: 'test',
        repo: 'repo',
        state: 'open',
        itemsPerPage: 50,
        type: 'fullContent',
        partialContentMetadata: { foo: 'bar' },
        verbosity: 'concise',
      };

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [query],
        }
      );

      const text = getTextContent(result.content);
      expect(text).toContain("type coerced to 'metadata' under concise");
      expect(query).toEqual({
        owner: 'test',
        repo: 'repo',
        state: 'open',
        itemsPerPage: 50,
        type: 'fullContent',
        partialContentMetadata: { foo: 'bar' },
        verbosity: 'concise',
      });

      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
      const q = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      // limit capped to 3
      expect(q.limit).toBe(3);
    });

    it('coerces omitted type to metadata under concise (no explicit type)', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR()])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              state: 'open',
              verbosity: 'concise',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
    });

    it('does NOT coerce type when prNumber + explicit type are both given under concise', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 456 })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            {
              owner: 'test',
              repo: 'repo',
              prNumber: 456,
              type: 'fullContent',
              verbosity: 'concise',
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
    });

    it('does not cap when concise limit is already within CONCISE_PR_LIMIT', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR()])
      );

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            state: 'open',
            itemsPerPage: 2,
            verbosity: 'concise',
          },
        ],
      });

      const q = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      expect(q.limit).toBe(2);
    });
  });

  describe('hasValidParams via query.query.trim() (line 120)', () => {
    it('accepts a query string as a valid search parameter', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR()])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ query: 'fix bug', owner: 'test', repo: 'repo' }],
        }
      );

      expect(result.isError).toBe(false);
      const text = getTextContent(result.content);
      expect(text).toContain('Test PR');
    });
  });

  describe('large-file detection (lines 197,199,211,213)', () => {
    it('uses changedFilesCount when it is a number', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 11, changedFilesCount: 42 })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const text = getTextContent(result.content);
      expect(text).toContain('42+ file changes');
      expect(text).toContain('#11');
    });

    it('falls back to fileChanges array length when changedFilesCount is absent', async () => {
      const fileChanges = Array.from({ length: 37 }, (_, i) => ({
        filename: `src/file${i}.ts`,
        status: 'modified',
        additions: 1,
        deletions: 0,
      }));
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 22, fileChanges })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const text = getTextContent(result.content);
      expect(text).toContain('37+ file changes');
      expect(text).toContain('#22');
    });

    it('does not flag small PRs (<=30 changes)', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 33, changedFilesCount: 3 })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const text = getTextContent(result.content);
      expect(text).not.toContain('file changes.');
    });

    it('keeps a lightweight file list (paths+counts, no patch) in metadata mode (default)', async () => {
      const fileChanges = Array.from({ length: 5 }, (_, i) => ({
        filename: `src/file${i}.ts`,
        status: 'modified',
        additions: 1,
        deletions: 0,
        patch: '@@ -1 +1 @@\n-a\n+b',
      }));
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 44, fileChanges })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          // type omitted ⇒ metadata (triage) default.
          queries: [{ owner: 'test', repo: 'repo', state: 'open' }],
        }
      );

      expect(result.isError).toBe(false);
      const prs = (
        (result.structuredContent as Record<string, unknown>)?.results as Array<
          Record<string, unknown>
        >
      )?.flatMap(
        q =>
          ((q.data as Record<string, unknown>)?.pull_requests as Array<
            Record<string, unknown>
          >) ?? []
      );
      expect(prs?.length).toBe(1);
      // Metadata keeps the file list (so "which files?" needs no second call)
      // but strips patches.
      const metaFileChanges = prs?.[0]?.fileChanges as
        | Array<Record<string, unknown>>
        | undefined;
      expect(metaFileChanges).toHaveLength(5);
      expect(metaFileChanges?.every(f => f.patch === undefined)).toBe(true);
      expect(prs?.[0]?.changedFilesCount).toBe(5);
      expect(getTextContent(result.content)).toContain(
        'Metadata mode: file lists include paths + counts only'
      );
    });

    it('keeps the file list when type="fullContent"', async () => {
      const fileChanges = Array.from({ length: 5 }, (_, i) => ({
        filename: `src/file${i}.ts`,
        status: 'modified',
        additions: 1,
        deletions: 0,
      }));
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 45, fileChanges })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [
            { owner: 'test', repo: 'repo', state: 'open', type: 'fullContent' },
          ],
        }
      );

      expect(result.isError).toBe(false);
      const prs = (
        (result.structuredContent as Record<string, unknown>)?.results as Array<
          Record<string, unknown>
        >
      )?.flatMap(
        q =>
          ((q.data as Record<string, unknown>)?.pull_requests as Array<
            Record<string, unknown>
          >) ?? []
      );
      expect(Array.isArray(prs?.[0]?.fileChanges)).toBe(true);
    });

    it('omits the pagination block on a prNumber lookup (#A3b)', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 410 })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        { queries: [{ owner: 'test', repo: 'repo', prNumber: 410 }] }
      );

      const data = (
        (result.structuredContent as Record<string, unknown>)?.results as Array<
          Record<string, unknown>
        >
      )?.[0]?.data as Record<string, unknown> | undefined;
      expect(data?.pull_requests).toBeDefined();
      expect(data?.pagination).toBeUndefined();
    });

    it('keeps the pagination block on a normal search', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR({ number: 1 }), basePR({ number: 2 })])
      );

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        { queries: [{ owner: 'test', repo: 'repo', state: 'open' }] }
      );

      const data = (
        (result.structuredContent as Record<string, unknown>)?.results as Array<
          Record<string, unknown>
        >
      )?.[0]?.data as Record<string, unknown> | undefined;
      expect(data?.pagination).toBeDefined();
    });
  });
});

/**
 * Targets the `fileChanges`-array fallback arms (lines 199 + 213) inside the
 * large-file detection / Math.max. The real provider mapper always backfills a
 * numeric `changedFilesCount`, so those arms are only reachable when a mapped
 * PR exposes `fileChanges` but no `changedFilesCount`. We mock the mapper to
 * return exactly that shape and drive the execution function directly.
 */
describe('large-file detection — fileChanges fallback arm (199,213)', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetModules();
  });

  it('computes maxFiles from fileChanges length when changedFilesCount is absent', async () => {
    const search = vi.fn();

    vi.doMock('../../src/tools/providerExecution.js', () => ({
      createProviderExecutionContext: vi.fn(() => ({
        provider: { searchPullRequests: search },
      })),
      createLazyProviderContext: vi.fn(() =>
        vi.fn(() => ({ provider: { searchPullRequests: search } }))
      ),
      executeProviderOperation: vi.fn(async () => ({
        ok: true,
        response: { data: {}, rawResponseChars: 0 },
      })),
    }));

    const fileChanges = Array.from({ length: 44 }, (_, i) => ({
      filename: `src/f${i}.ts`,
    }));

    vi.doMock('../../src/tools/providerMappers.js', async () => {
      const actual = await vi.importActual<
        typeof import('../../src/tools/providerMappers.js')
      >('../../src/tools/providerMappers.js');
      return {
        ...actual,
        mapPullRequestProviderResultData: vi.fn(() => ({
          // PR has fileChanges but intentionally no changedFilesCount
          pullRequests: [{ number: 77, fileChanges }],
          resultData: {
            pull_requests: [{ number: 77, fileChanges }],
            total_count: 1,
          },
          pagination: undefined,
        })),
      };
    });

    const { searchMultipleGitHubPullRequests } =
      await import('../../src/tools/github_search_pull_requests/execution.js');

    const result = await searchMultipleGitHubPullRequests({
      queries: [
        {
          owner: 'test',
          repo: 'repo',
          state: 'open',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
      authInfo: undefined,
      sessionId: undefined,
    } as never);

    const first = result.content?.[0];
    const text =
      first && 'text' in first && typeof first.text === 'string'
        ? first.text
        : '';
    expect(text).toContain('44+ file changes');
    expect(text).toContain('#77');
  });
});

describe('applyGithubSearchPullRequestsVerbosity — direct (lines 313,324,333,336)', () => {
  const baseInput = {
    data: { pull_requests: [], total_count: 2 } as Record<string, unknown>,
    pullRequests: [
      { number: 101, title: 'A', state: 'open', merged: false },
      { number: 102, title: 'B', state: 'closed', merged: true },
    ] as Array<Record<string, unknown>>,
    extraHints: ['some hint'],
  };

  it('concise projects PRs to identity fields and prepends a summary (313,324)', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      { ...baseInput, extraHints: ['h1'] },
      { verbosity: 'concise' } as never
    );

    const prs = out.data.pull_requests as Array<Record<string, unknown>>;
    expect(prs).toHaveLength(2);
    expect(Object.keys(prs[0]).sort()).toEqual(
      ['merged', 'number', 'state', 'title'].sort()
    );
    expect(out.extraHints[0]).toBe('2 PRs (top: #101)');
    expect(out.extraHints).toContain('h1');
  });

  it('concise summary uses "?" when the first PR has no number (324)', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      {
        data: {},
        pullRequests: [{ title: 'no-number' }],
        extraHints: [],
      },
      { verbosity: 'concise' } as never
    );

    expect(out.extraHints[0]).toBe('1 PRs (top: #?)');
  });

  it('compact trims advisory hints and keeps data intact (333,336)', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      {
        data: { pull_requests: [{ number: 1 }] },
        pullRequests: [{ number: 1 }],
        extraHints: [
          'Page 1/2 (showing 1 of 2 PRs)',
          'PR archaeology: use prNumber',
          'withComments adds tokens',
          'another data hint',
        ],
      },
      { verbosity: 'compact' } as never
    );

    expect(out.data).toEqual({ pull_requests: [{ number: 1 }] });
    // advisory entries (archaeology / withComments) trimmed; cap of 2 applied
    expect(out.extraHints.length).toBeLessThanOrEqual(2);
    expect(out.extraHints).toContain('Page 1/2 (showing 1 of 2 PRs)');
    expect(
      out.extraHints.some(h => h.toLowerCase().includes('archaeology'))
    ).toBe(false);
  });

  it('compact returns [] when compactTrimHints yields nothing', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      { data: { x: 1 }, pullRequests: [], extraHints: [] },
      { verbosity: 'compact' } as never
    );
    expect(out.extraHints).toEqual([]);
  });

  it('basic / omitted verbosity passes hints and data through unchanged', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      { data: { x: 1 }, pullRequests: [{ number: 1 }], extraHints: ['keep'] },
      {} as never
    );
    expect(out.data).toEqual({ x: 1 });
    expect(out.extraHints).toEqual(['keep']);
  });
});
