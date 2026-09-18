import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';

const mocks = vi.hoisted(() => ({
  cacheKeys: [] as string[],
  generateCacheKey: vi.fn((_prefix: string, params: Record<string, unknown>) =>
    JSON.stringify(params)
  ),
  searchReposApi: vi.fn(async () => ({
    data: { items: [], total_count: 0 },
    headers: {},
  })),
  listForOrg: vi.fn(async () => ({ data: [] })),
  listForUser: vi.fn(async () => ({ data: [] })),
  getOctokit: vi.fn(async () => ({
    rest: {
      search: {
        repos: mocks.searchReposApi,
      },
      repos: {
        listForOrg: mocks.listForOrg,
        listForUser: mocks.listForUser,
      },
    },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'test-auth'),
  provider: {
    type: 'github' as const,
    capabilities: {
      cloneRepo: true,
      requiresScopedCodeSearch: false,
      supportsMergedState: true,
      supportsMultiTopicSearch: true,
    },
    searchCode: vi.fn(),
    getFileContent: vi.fn(),
    searchRepos: vi.fn(),
    searchPullRequests: vi.fn(),
    getRepoStructure: vi.fn(),
    resolveDefaultBranch: vi.fn(async () => 'main'),
  },
  withDataCache: vi.fn(
    async (key: string, producer: () => Promise<unknown>) => {
      mocks.cacheKeys.push(key);
      return producer();
    }
  ),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mocks.getOctokit,
  resolveCacheAuthFingerprint: mocks.resolveCacheAuthFingerprint,
}));

vi.mock('../../src/utils/http/cache/key.js', () => ({
  generateCacheKey: mocks.generateCacheKey,
}));
vi.mock('../../src/utils/http/cache/dataCache.js', () => ({
  withDataCache: mocks.withDataCache,
}));

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: () => mocks.provider,
}));

import { cleanup, initialize } from '../../src/serverConfig.js';
import { searchGitHubReposAPI } from '../../src/github/repoSearch.js';
import { executeGitHubSearch } from '../../src/tools/github_search/execution.js';
import { GitHubSearchQuerySchema } from '@octocodeai/octocode-core/schema';

function parseQuery(query: Record<string, unknown>) {
  return GitHubSearchQuerySchema.parse({
    reasoning: 'Exercise unified GitHub search execution parity.',
    debug: true,
    ...query,
  });
}

const pagination = {
  currentPage: 1,
  totalPages: 2,
  hasMore: true,
  nextPage: 2,
  entriesPerPage: 20,
  totalMatches: 21,
};

function providerResponse(data: unknown) {
  return Promise.resolve({
    provider: 'github' as const,
    status: 200,
    rawResponseChars: JSON.stringify(data).length,
    data,
  });
}

function rows(result: { structuredContent?: unknown }): Record<string, any>[] {
  return (
    (result.structuredContent as { results?: Record<string, any>[] }).results ??
    []
  );
}

beforeAll(async () => {
  await initialize();
});

afterAll(() => {
  cleanup();
});

describe('ghSearch repository cache parity', () => {
  beforeEach(() => {
    mocks.cacheKeys.length = 0;
    vi.clearAllMocks();
  });

  it.each([
    ['size', '100..2000', '2001..4000'],
    ['created', '>=2024-01-01', '>=2025-01-01'],
  ] as const)(
    '%s remains query-local in the raw provider cache',
    async (field, a, b) => {
      const base = { keywords: ['needle'] };
      await searchGitHubReposAPI({ ...base, [field]: a });
      await searchGitHubReposAPI({ ...base, [field]: b });

      expect(mocks.cacheKeys).toHaveLength(2);
      expect(mocks.cacheKeys[0]).not.toBe(mocks.cacheKeys[1]);
      expect(mocks.generateCacheKey.mock.calls[0]?.[1]).toMatchObject({
        [field]: a,
      });
      expect(mocks.generateCacheKey.mock.calls[1]?.[1]).toMatchObject({
        [field]: b,
      });
    }
  );

  it.each([
    ['stars', '>100'],
    ['forks', '>10'],
    ['goodFirstIssues', '>2'],
    ['updated', '>=2024-01-01'],
    ['created', '>=2024-01-01'],
    ['size', '100..2000'],
    ['language', 'typescript'],
    ['match', ['name']],
    ['license', 'mit'],
    ['visibility', 'public'],
    ['archived', true],
    ['sort', 'stars'],
  ] as const)(
    'owner plus %s uses repository search instead of the owner-list shortcut',
    async (field, value) => {
      await searchGitHubReposAPI({ owner: 'bgauryy', [field]: value });

      expect(mocks.searchReposApi).toHaveBeenCalledOnce();
      expect(mocks.listForOrg).not.toHaveBeenCalled();
    }
  );
});

describe('ghSearch recorded-response execution parity', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.provider.searchCode.mockImplementation(() =>
      providerResponse({
        items: [
          {
            path: 'src/index.ts',
            matches: [
              { context: 'export const needle = true;', positions: [[13, 19]] },
            ],
            url: 'https://github.com/recorded/fixture/blob/main/src/index.ts',
            repository: {
              id: 'recorded/fixture',
              name: 'recorded/fixture',
              url: 'https://github.com/recorded/fixture',
            },
          },
        ],
        totalCount: 21,
        pagination,
        repositoryContext: {
          owner: 'recorded',
          repo: 'fixture',
          branch: 'main',
        },
      })
    );
    mocks.provider.searchRepos.mockImplementation(() =>
      providerResponse({
        repositories: [
          {
            id: 'recorded/fixture',
            name: 'fixture',
            fullPath: 'recorded/fixture',
            description: 'Recorded provider fixture',
            url: 'https://github.com/recorded/fixture',
            cloneUrl: 'https://github.com/recorded/fixture.git',
            defaultBranch: 'main',
            stars: 10,
            forks: 1,
            visibility: 'public',
            topics: ['fixture'],
            createdAt: '2024-01-01T00:00:00Z',
            updatedAt: '2024-01-02T00:00:00Z',
            lastActivityAt: '2024-01-02T00:00:00Z',
          },
        ],
        totalCount: 21,
        pagination,
      })
    );
    mocks.provider.getRepoStructure.mockImplementation(() =>
      providerResponse({
        projectPath: 'recorded/fixture',
        branch: 'main',
        defaultBranch: 'main',
        path: 'src',
        structure: { src: { files: ['index.ts'], folders: [] } },
        summary: { totalFiles: 1, totalFolders: 0, truncated: false },
        pagination,
      })
    );
  });

  it('executes every public branch without a legacy translation layer', async () => {
    const code = {
      owner: 'recorded',
      repo: 'fixture',
      keywords: ['needle'],
      match: 'file' as const,
      pageSize: 20,
      page: 1,
    };
    const repositories = {
      keywords: ['fixture'],
      forks: '>=1',
      goodFirstIssues: '>0',
      created: '>=2024-01-01',
      size: '1..5000',
      sort: 'best-match' as const,
      pageSize: 20,
      page: 1,
    };
    const tree = {
      owner: 'recorded',
      repo: 'fixture',
      path: 'src',
      branch: 'main',
      pageSize: 20,
      page: 1,
    };

    const unified = await executeGitHubSearch({
      queries: [
        parseQuery({ operation: 'code', ...code }),
        parseQuery({
          operation: 'repositories',
          ...repositories,
        }),
        parseQuery({ operation: 'tree', ...tree }),
      ],
    });
    const unifiedRows = rows(unified);
    expect(unifiedRows.map(row => row.index)).toEqual([0, 1, 2]);
    expect(unifiedRows.map(row => row.data.operation)).toEqual([
      'code',
      'repositories',
      'tree',
    ]);
    expect(unifiedRows[0]?.data.files[0]).toMatchObject({
      owner: 'recorded',
      repo: 'fixture',
      path: 'src/index.ts',
    });
    expect(unifiedRows[1]?.data.repositories[0]).toMatchObject({
      owner: 'recorded',
      repo: 'fixture',
    });
    expect(unifiedRows[2]?.data.structure[0]).toMatchObject({
      dir: 'src',
      files: ['index.ts'],
    });
    const serialized = JSON.stringify(unifiedRows);
    expect(serialized).toContain('"tool":"ghSearch"');
    expect(serialized).not.toMatch(
      /"tool":"(?:github.code|github.repositories|github.tree)"/
    );
    expect(mocks.provider.searchRepos).toHaveBeenCalledWith(
      expect.objectContaining({
        forks: '>=1',
        goodFirstIssues: '>0',
        created: '>=2024-01-01',
        size: '1..5000',
      })
    );
  });

  it('preserves mixed input ordering and keeps a partial failure non-fatal', async () => {
    mocks.provider.searchCode.mockRejectedValueOnce(
      new Error('recorded failure')
    );
    const result = await executeGitHubSearch({
      queries: [
        parseQuery({
          operation: 'tree',
          owner: 'recorded',
          repo: 'fixture',
        }),
        parseQuery({
          operation: 'code',
          owner: 'recorded',
          repo: 'fixture',
          keywords: ['needle'],
        }),
        parseQuery({
          operation: 'repositories',
          keywords: ['fixture'],
        }),
      ],
    });

    expect(
      rows(result).map(row => [row.index, row.data.operation, row.status])
    ).toEqual([
      [0, 'tree', undefined],
      [1, 'code', 'error'],
      [2, 'repositories', undefined],
    ]);
    expect(result.isError).not.toBe(true);
  });

  it('marks provider-incomplete code search and emits an executable same-page retry', async () => {
    mocks.provider.searchCode.mockImplementationOnce(() =>
      providerResponse({
        items: [],
        totalCount: 0,
        incompleteResults: true,
        pagination: { ...pagination, hasMore: false, nextPage: undefined },
      })
    );
    const query = parseQuery({
      operation: 'code',
      owner: 'recorded',
      repo: 'fixture',
      keywords: ['needle'],
      page: 3,
    });
    const result = await executeGitHubSearch({ queries: [query] });
    const row = rows(result)[0]!;
    expect(row.data).toMatchObject({
      isPartial: true,
      partialReasons: ['providerIncompleteResults'],
    });
    expect(row.data.next.retry).toMatchObject({
      tool: 'ghSearch',
      query: { operation: 'code', page: 3 },
    });
    expect(row.meta.diagnostics?.partial).toBe(true);
    expect(row.meta.diagnostics?.codes ?? []).not.toContain(
      'continuationMissing'
    );
  });

  it('marks an incomplete provider tree as an explicit terminal limit', async () => {
    mocks.provider.getRepoStructure.mockImplementationOnce(() =>
      providerResponse({
        projectPath: 'recorded/fixture',
        branch: 'main',
        path: 'src',
        structure: {},
        summary: {
          totalFiles: 0,
          totalFolders: 0,
          truncated: false,
          incompleteTree: true,
        },
        isPartial: true,
        terminalLimit: true,
        partialReasons: ['providerTreeTruncated'],
        pagination: { ...pagination, hasMore: false, nextPage: undefined },
      })
    );
    const result = await executeGitHubSearch({
      queries: [
        parseQuery({
          operation: 'tree',
          owner: 'recorded',
          repo: 'fixture',
          path: 'src',
        }),
      ],
    });
    const row = rows(result)[0]!;
    expect(row.data).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['providerTreeTruncated'],
    });
    expect(row.meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row.data.next?.retry).toBeUndefined();
  });

  it('marks failed recursive tree branches partial and emits an executable same-page retry', async () => {
    mocks.provider.getRepoStructure.mockImplementationOnce(() =>
      providerResponse({
        projectPath: 'recorded/fixture',
        branch: 'main',
        path: 'src',
        structure: {},
        summary: { totalFiles: 0, totalFolders: 0, truncated: false },
        isPartial: true,
        partialReasons: ['partialTreeFailures'],
        pagination: { ...pagination, hasMore: false, nextPage: undefined },
      })
    );
    const result = await executeGitHubSearch({
      queries: [
        parseQuery({
          operation: 'tree',
          owner: 'recorded',
          repo: 'fixture',
          path: 'src',
          page: 2,
        }),
      ],
    });
    const row = rows(result)[0]!;

    expect(row.data).toMatchObject({
      isPartial: true,
      partialReasons: ['partialTreeFailures'],
    });
    expect(row.data.next.retry).toMatchObject({
      tool: 'ghSearch',
      query: { operation: 'tree', page: 2 },
    });
    expect(row.meta.diagnostics?.partial).toBe(true);
    expect(row.meta.diagnostics?.codes ?? []).not.toContain(
      'continuationMissing'
    );
  });

  it('emits and replays an executable next-page continuation for every operation', async () => {
    const result = await executeGitHubSearch({
      queries: [
        parseQuery({
          operation: 'code',
          owner: 'recorded',
          repo: 'fixture',
          keywords: ['needle'],
          pageSize: 20,
          page: 1,
          goal: 'find the implementation',
          reasoning: 'search the recorded fixture',
        }),
        parseQuery({
          operation: 'repositories',
          keywords: ['fixture'],
          pageSize: 20,
          page: 1,
          goal: 'find repositories',
          reasoning: 'discover candidates',
        }),
        parseQuery({
          operation: 'tree',
          owner: 'recorded',
          repo: 'fixture',
          path: 'src',
          pageSize: 20,
          page: 1,
          goal: 'browse the tree',
          reasoning: 'orient in the repository',
        }),
      ],
    });

    const continuations = rows(result).map(row => row.data.next?.nextPage);
    expect(continuations).toHaveLength(3);
    expect(continuations.map(next => next?.tool)).toEqual([
      'ghSearch',
      'ghSearch',
      'ghSearch',
    ]);
    expect(continuations.map(next => next?.query)).toMatchObject([
      { operation: 'code', page: 2, pageSize: 20 },
      { operation: 'repositories', page: 2, pageSize: 20 },
      { operation: 'tree', page: 2, pageSize: 20 },
    ]);
    for (const next of continuations) {
      expect(next.query).not.toHaveProperty('goal');
      expect(next.query).toMatchObject({
        reasoning: expect.stringMatching(/\S/),
        debug: expect.any(Boolean),
      });
    }

    const replayQueries = continuations.map(next =>
      parseQuery(next.query)
    );
    await expect(
      executeGitHubSearch({ queries: replayQueries })
    ).resolves.toBeDefined();
    expect(mocks.provider.searchCode).toHaveBeenLastCalledWith(
      expect.objectContaining({ page: 2 })
    );
    expect(mocks.provider.searchRepos).toHaveBeenLastCalledWith(
      expect.objectContaining({ page: 2 })
    );
    expect(mocks.provider.getRepoStructure).toHaveBeenLastCalledWith(
      expect.objectContaining({ page: 2 })
    );
  });

  it.each([
    ['code', { owner: 'recorded', repo: 'fixture', keywords: ['needle'] }],
    ['repositories', { keywords: ['fixture'] }],
    ['tree', { owner: 'recorded', repo: 'fixture', path: 'src' }],
  ] as const)(
    'marks an unreachable %s page beyond the public schema ceiling explicitly',
    async (operation, input) => {
      Object.assign(pagination, {
        currentPage: 1000,
        totalPages: 1001,
        nextPage: 1001,
      });
      try {
        const result = await executeGitHubSearch({
          queries: [
            parseQuery({
              operation,
              ...input,
              page: 1000,
              pageSize: 1,
            }),
          ],
        });
        const data = rows(result)[0]!.data;
        expect(data.pagination).toMatchObject({
          hasMore: true,
          continuationUnavailable: {
            reason: 'schemaPageLimit',
            maxPage: 1000,
          },
        });
        expect(data.terminalLimit).toBe(true);
        expect(data.pagination.nextPage).toBeUndefined();
        expect(data.next?.nextPage).toBeUndefined();
        const meta = rows(result)[0]!.meta;
        expect(meta.diagnostics?.codes).toContain('terminalLimitReached');
        expect(meta.diagnostics?.codes).not.toContain('continuationMissing');
      } finally {
        Object.assign(pagination, {
          currentPage: 1,
          totalPages: 2,
          nextPage: 2,
        });
      }
    }
  );
});
