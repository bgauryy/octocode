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
import { mapPullRequestProviderResultData } from '../../src/tools/providerMappers.js';
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

  describe('verbosity pre-flight behavior (lines 86,88,94,95,97,99,102)', () => {
    it('verbose=false is a no-op — no limit cap, no type coercion', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR()])
      );
      const query = {
        owner: 'test',
        repo: 'repo',
        state: 'open',
        type: 'fullContent',
        partialContentMetadata: { foo: 'bar' },
        verbose: false,
      };

      const result = await mockServer.callTool(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        {
          queries: [query],
        }
      );

      expect(result.isError).toBe(false);
      const text = getTextContent(result.content);
      expect(text).not.toContain("type coerced to 'metadata' under concise");

      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
      const q = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      expect(q.limit).toBeGreaterThan(0);
    });

    it('verbose:false with omitted type — succeeds without type coercion', async () => {
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
              verbose: false,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
    });

    it('verbose:false with prNumber + explicit type — succeeds', async () => {
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
              verbose: false,
            },
          ],
        }
      );

      expect(result.isError).toBe(false);
      expect(mockProvider.searchPullRequests).toHaveBeenCalledTimes(1);
    });

    it('does not cap when verbose=false with page=1', async () => {
      mockProvider.searchPullRequests.mockResolvedValue(
        providerResponse([basePR()])
      );

      await mockServer.callTool(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
        queries: [
          {
            owner: 'test',
            repo: 'repo',
            state: 'open',
            page: 1,
            verbose: false,
          },
        ],
      });

      const q = mockProvider.searchPullRequests.mock.calls[0]?.[0];
      expect(q.limit).toBeGreaterThan(0);
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

    it('omits fileChanges entirely in metadata mode (type omitted = default)', async () => {
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
      expect(prs?.[0]).not.toHaveProperty('fileChanges');
      expect(prs?.[0]?.changedFilesCount).toBe(5);
      expect(getTextContent(result.content)).toContain('Metadata mode');
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

describe('applyGithubSearchPullRequestsVerbosity — direct', () => {
  const basePRs = [
    {
      number: 101,
      title: 'A',
      state: 'open',
      merged: false,
      createdAt: '2024-01-01',
    },
    {
      number: 102,
      title: 'B',
      state: 'closed',
      merged: true,
      updatedAt: '2024-01-02',
    },
  ] as Array<Record<string, unknown>>;

  it('verbose=false (default) strips metadata fields from PRs', () => {
    const input = {
      data: { total_count: 2 } as Record<string, unknown>,
      pullRequests: basePRs,
      extraHints: ['h1'],
    };
    const out = applyGithubSearchPullRequestsVerbosity(input, {} as never);

    const prs = out.data.pull_requests as Array<Record<string, unknown>>;
    expect(prs).toBeDefined();
    expect(prs[0]).not.toHaveProperty('createdAt');
    expect(prs[0]).toHaveProperty('number');
    expect(out.extraHints).toEqual(['h1']);
  });

  it('verbose=true passes PRs and data through unchanged with all metadata', () => {
    const input = {
      data: { total_count: 2 } as Record<string, unknown>,
      pullRequests: basePRs,
      extraHints: ['h1'],
    };
    const out = applyGithubSearchPullRequestsVerbosity(input, {
      verbose: true,
    } as never);

    expect(out.data).toEqual({ total_count: 2 });
    expect(out.extraHints).toEqual(['h1']);
  });

  it('with no-number PR — strips metadata but keeps core fields', () => {
    const input = {
      data: {},
      pullRequests: [{ title: 'no-number', createdAt: '2024-01-01' }],
      extraHints: [],
    };
    const out = applyGithubSearchPullRequestsVerbosity(input, {} as never);

    const prs = out.data.pull_requests as Array<Record<string, unknown>>;
    expect(prs[0]).not.toHaveProperty('createdAt');
    expect(prs[0]).toHaveProperty('title');
    expect(out.extraHints).toEqual([]);
  });

  it('advisory hints always preserved regardless of verbosity', () => {
    const allHints = [
      'Page 1/2 (showing 1 of 2 PRs)',
      'PR archaeology: use prNumber',
      'withComments adds tokens',
      'another data hint',
    ];
    const out = applyGithubSearchPullRequestsVerbosity(
      {
        data: { pull_requests: [{ number: 1 }] },
        pullRequests: [{ number: 1 }],
        extraHints: [...allHints],
      },
      {} as never
    );

    expect(out.extraHints).toEqual(allHints);
    expect(out.extraHints.length).toBe(4);
    expect(out.extraHints).toContain('Page 1/2 (showing 1 of 2 PRs)');
    expect(
      out.extraHints.some(h => h.toLowerCase().includes('archaeology'))
    ).toBe(true);
  });

  it('empty pullRequests returns empty pull_requests array', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      { data: { x: 1 }, pullRequests: [], extraHints: [] },
      {} as never
    );
    expect(out.extraHints).toEqual([]);
    const prs = out.data.pull_requests as Array<unknown>;
    expect(prs).toEqual([]);
  });

  it('verbose=true with hints and data passes through unchanged', () => {
    const out = applyGithubSearchPullRequestsVerbosity(
      { data: { x: 1 }, pullRequests: [{ number: 1 }], extraHints: ['keep'] },
      { verbose: true } as never
    );
    expect(out.data).toEqual({ x: 1 });
    expect(out.extraHints).toEqual(['keep']);
  });
});

function makeProviderData(fileChanges: Array<Record<string, unknown>>) {
  return {
    items: [
      {
        number: 42,
        title: 'Test PR',
        state: 'open',
        draft: false,
        merged: false,
        author: { login: 'user', id: '1' },
        assignees: [],
        labels: [],
        head: { ref: 'feature', sha: 'abc' },
        base: { ref: 'main', sha: 'def' },
        body: null,
        comments: [],
        createdAt: '2024-01-01',
        updatedAt: '2024-01-01',
        closedAt: null,
        mergedAt: null,
        commentsCount: 0,
        changedFilesCount: fileChanges.length,
        additions: 5,
        deletions: 2,
        url: 'https://github.com/t/r/pull/42',
        fileChanges,
      },
    ],
    totalCount: 1,
    pagination: undefined,
  } as never;
}

describe('mapPullRequestProviderResultData — metadata mode (includeFileChanges=false)', () => {
  const fileChanges = [
    { filename: 'src/a.ts', additions: 5, deletions: 2, patch: 'diff...' },
    { filename: 'src/b.ts', additions: 1, deletions: 0, patch: 'diff...' },
  ];

  it('type=metadata (includeFileChanges=false) omits fileChanges from each PR', () => {
    const { pullRequests } = mapPullRequestProviderResultData(
      makeProviderData(fileChanges),
      { includeFileChanges: false }
    );
    for (const pr of pullRequests) {
      expect(pr).not.toHaveProperty('fileChanges');
    }
  });

  it('type=metadata keeps changedFilesCount', () => {
    const { pullRequests } = mapPullRequestProviderResultData(
      makeProviderData(fileChanges),
      { includeFileChanges: false }
    );
    expect(pullRequests[0]).toHaveProperty('changedFilesCount');
    expect((pullRequests[0] as Record<string, unknown>).changedFilesCount).toBe(
      fileChanges.length
    );
  });

  it('type=partialContent (includeFileChanges=true) includes fileChanges', () => {
    const { pullRequests } = mapPullRequestProviderResultData(
      makeProviderData(fileChanges),
      { includeFileChanges: true }
    );
    expect(pullRequests[0]).toHaveProperty('fileChanges');
  });
});
