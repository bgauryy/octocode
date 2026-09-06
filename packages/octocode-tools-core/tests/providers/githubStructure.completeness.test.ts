import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getRepoStructure } from '../../src/providers/github/githubStructure.js';
import { exploreRepositoryStructure } from '../../src/tools/github_view_repo_structure/execution.js';
import { GitHubSearchQuerySchema } from '../../src/toolContract/input/resources/tools/ghSearch.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';
import { cache } from '../../src/utils/http/cache/store.js';

const mocks = vi.hoisted(() => ({
  tree: vi.fn(),
  contributors: vi.fn(),
  branches: vi.fn(),
  tags: vi.fn(),
  languages: vi.fn(),
}));
vi.mock('../../src/github/repoStructure/fetchOrchestration.js', () => ({
  viewGitHubRepositoryStructureAPI: mocks.tree,
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      repos: {
        listContributors: mocks.contributors,
        listBranches: mocks.branches,
        listTags: mocks.tags,
        listLanguages: mocks.languages,
      },
    },
  }),
  resolveCacheAuthFingerprint: async (auth?: { token?: string }) =>
    auth?.token ?? 'structure-test',
}));
vi.mock('../../src/utils/http/cache/diskStore.js', () => ({
  readDiskCache: async () => undefined,
  writeDiskCache: async () => {},
}));

async function execute(query: Record<string, unknown>) {
  const { operation: _operation, pageSize, ...internal } = query;
  return exploreRepositoryStructure(
    { ...internal, itemsPerPage: pageSize } as never,
    {} as never,
    () => ({ provider: { getRepoStructure } }) as never
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  cache.flushAll();
  mocks.tree.mockResolvedValue({
    owner: 'owner',
    repo: 'repo',
    branch: 'main',
    path: 'src',
    structure: { '.': { files: ['index.ts'], folders: [] } },
    summary: { totalFiles: 1, totalFolders: 0, truncated: false },
  });
});

describe('GitHub tree provider to public result completeness', () => {
  it('reuses language metadata across tree pages and keeps auth scopes separate', async () => {
    mocks.languages.mockResolvedValue({
      data: { TypeScript: 42 },
      headers: {},
    });
    const query = {
      projectId: 'owner/repo',
      ref: 'main',
      includeLanguages: true,
    };
    await getRepoStructure({ ...query, page: 1 });
    await getRepoStructure({ ...query, page: 2 });
    expect(mocks.languages).toHaveBeenCalledTimes(1);
    await getRepoStructure(query, { token: 'different' } as never);
    expect(mocks.languages).toHaveBeenCalledTimes(2);
  });

  it('exposes and executes a language retry instead of silently losing requested metadata', async () => {
    mocks.languages.mockRejectedValueOnce(new Error('temporary failure'));
    const result = await execute({
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      include: ['languages'],
    });
    expect(result).toMatchObject({
      isPartial: true,
      partialReasons: ['metadataFetchFailed'],
    });
    const retry = (
      result.next as Record<
        string,
        { tool: string; query: Record<string, unknown> }
      >
    ).languages!;
    expect(retry.tool).toBe('ghSearch');
    expect(GitHubSearchQuerySchema.safeParse(retry.query).success).toBe(true);
    mocks.languages.mockResolvedValue({
      data: { TypeScript: 42 },
      headers: {},
    });
    expect(await execute(retry.query)).toMatchObject({
      languages: { TypeScript: 42 },
    });
    expect(mocks.languages).toHaveBeenCalledTimes(2);
  });

  it('reuses one metadata page across tree pages and isolates auth and metadata page keys', async () => {
    mocks.tags.mockResolvedValue({
      data: [{ name: 'one', commit: { sha: 'sha' } }],
      headers: {},
    });
    const query = { projectId: 'owner/repo', ref: 'main', includeTags: true };
    await getRepoStructure({ ...query, page: 1 });
    await getRepoStructure({ ...query, page: 2 });
    expect(mocks.tags).toHaveBeenCalledTimes(1);
    await getRepoStructure({ ...query, metadataPage: 2 });
    expect(mocks.tags).toHaveBeenCalledTimes(2);
    await getRepoStructure(query, { token: 'other-auth@enterprise' } as never);
    expect(mocks.tags).toHaveBeenCalledTimes(3);
  });

  it('accounts for metadata bytes in raw source metrics', async () => {
    const value = await mocks.tree();
    mocks.tree.mockResolvedValue({ ...value, rawResponseChars: 20 });
    const tags = [{ name: 'one', commit: { sha: 'sha' } }];
    mocks.tags.mockResolvedValue({ data: tags, headers: {} });
    const result = await getRepoStructure({
      projectId: 'owner/repo',
      ref: 'main',
      includeTags: true,
    });
    expect(result.rawResponseChars).toBe(20 + JSON.stringify(tags).length);
  });
  it('does not label returned metadata empty when the tree page has no visible files', async () => {
    const value = await mocks.tree();
    mocks.tree.mockResolvedValue({ ...value, structure: {} });
    mocks.tags.mockResolvedValue({
      data: [{ name: 'one', commit: { sha: 'sha' } }],
      headers: {},
    });
    const result = await execute({
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      include: ['tags'],
    });
    expect(result.tags).toEqual([{ name: 'one', sha: 'sha' }]);
    expect(result.status).toBeUndefined();
  });
  it.each(['providerTreeTruncated', 'partialTreeFailures'])(
    'preserves %s through the full mapping',
    async reason => {
      const value = await mocks.tree();
      mocks.tree.mockResolvedValue({
        ...value,
        summary: {
          ...value.summary,
          incompleteTree: reason === 'providerTreeTruncated',
        },
        isPartial: true,
        terminalLimit: true,
        partialReasons: [reason],
      });
      const result = await execute({
        owner: 'owner',
        repo: 'repo',
        branch: 'main',
      });
      expect(result).toMatchObject({
        isPartial: true,
        terminalLimit: true,
        partialReasons: [reason],
      });
      if (reason === 'providerTreeTruncated')
        expect(result.summary).toMatchObject({ incompleteTree: true });
    }
  );

  it.each([
    ['contributors', 30, 65],
    ['branches', 100, 205],
    ['tags', 50, 105],
  ] as const)(
    'executes %s continuations to cover the entire fixture',
    async (kind, perPage, count) => {
      const fixture = Array.from({ length: count }, (_, i) =>
        kind === 'contributors'
          ? { login: `user-${i}`, contributions: count - i }
          : { name: `name-${i}`, commit: { sha: `sha-${i}` } }
      );
      mocks[kind].mockImplementation(
        async ({
          page = 1,
          per_page,
        }: {
          page?: number;
          per_page: number;
        }) => ({
          data: fixture.slice((page - 1) * per_page, page * per_page),
          headers:
            page * per_page < count
              ? {
                  link: `<https://api.github.com/next?page=${page + 1}>; rel="next"`,
                }
              : {},
        })
      );
      let query: Record<string, unknown> = {
        operation: 'tree',
        owner: 'owner',
        repo: 'repo',
        branch: 'main',
        path: 'src',
        maxDepth: 2,
        page: 3,
        pageSize: 10,
        include: [kind],
      };
      const received: unknown[] = [];
      for (let n = 0; n < 4; n++) {
        const result = await execute(query);
        received.push(...(result[kind] as unknown[]));
        expect(
          result[`total${kind[0]!.toUpperCase()}${kind.slice(1)}`]
        ).toBeUndefined();
        expect(result.metadataPagination).toMatchObject({
          [kind]: {
            currentPage: n + 1,
            perPage,
            returned: Math.min(perPage, count - n * perPage),
          },
        });
        const next = (
          result.next as Record<
            string,
            { tool: string; query: Record<string, unknown> }
          >
        )[kind];
        if (!next) break;
        expect(next.tool).toBe('ghSearch');
        expect(result.isPartial).toBe(true);
        expect(next.query).toMatchObject({
          page: 3,
          pageSize: 10,
          path: 'src',
          maxDepth: 2,
          include: [kind],
          metadataPage: n + 2,
        });
        expect(
          GitHubSearchQuerySchema.safeParse({
            ...next.query,
            goal: 'test',
            reasoning: 'test',
          }).success
        ).toBe(true);
        query = next.query;
      }
      expect(received).toHaveLength(count);
      expect(new Set(received.map(item => JSON.stringify(item))).size).toBe(
        count
      );
      expect(mocks[kind]).toHaveBeenCalledTimes(3);
    }
  );

  it('uses Link semantics even for a short metadata page', async () => {
    mocks.tags.mockResolvedValue({
      data: [{ name: 'one', commit: { sha: 'sha' } }],
      headers: { link: '<https://api.github.com/tags?page=2>; rel="next"' },
    });
    const result = await execute({
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      include: ['tags'],
    });
    expect(result.next).toMatchObject({ tags: { query: { metadataPage: 2 } } });
  });

  it('returns a same-page retry on metadata failure without claiming an empty list', async () => {
    mocks.branches.mockRejectedValueOnce(new Error('temporary failure'));
    const result = await execute({
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      include: ['branches'],
      metadataPage: 2,
    });
    expect(result).toMatchObject({
      isPartial: true,
      partialReasons: ['metadataFetchFailed'],
      metadataPagination: { branches: { failed: true, currentPage: 2 } },
    });
    expect(result.branches).toBeUndefined();
    const retry = (
      result.next as Record<string, { query: Record<string, unknown> }>
    ).branches!;
    expect(retry.query.metadataPage).toBe(2);
    mocks.branches.mockResolvedValue({
      data: [{ name: 'recovered' }],
      headers: {},
    });
    expect(await execute(retry.query)).toMatchObject({
      branches: ['recovered'],
    });
  });

  it('marks a terminal limit instead of emitting an invalid metadata page', async () => {
    mocks.tags.mockResolvedValue({
      data: [],
      headers: { link: '<https://api.github.com/tags?page=1001>; rel="next"' },
    });
    const result = await execute({
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      include: ['tags'],
      metadataPage: 1000,
    });
    expect(result).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['metadataPageLimit'],
    });
    expect((result.next as Record<string, unknown>).tags).toBeUndefined();
  });

  it('preserves independent metadata continuation and failed-tree retry through the public finalizer', async () => {
    const value = await mocks.tree();
    mocks.tree.mockResolvedValue({
      ...value,
      isPartial: true,
      partialReasons: ['partialTreeFailures'],
    });
    mocks.tags.mockResolvedValue({
      data: [{ name: 'one', commit: { sha: 'sha' } }],
      headers: { link: '<https://api.github.com/tags?page=2>; rel="next"' },
    });
    const query = {
      operation: 'tree',
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      path: 'src',
      include: ['tags'],
    };
    const data = await execute(query);
    const final = buildGitHubSearchFinalizer()({
      queries: [query],
      results: [{ index: 0, data }],
      config: { toolName: 'ghSearch' },
    } as never);
    const output = final.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    };
    const result = output.results[0]!.data;
    expect(result).toMatchObject({
      isPartial: true,
      metadataPagination: { tags: { hasMore: true } },
      partialReasons: ['partialTreeFailures', 'metadataPagination'],
    });
    expect(result.next.tags.query.metadataPage).toBe(2);
    expect(result.next.retry.query.page).toBe(1);
    for (const key of ['tags', 'retry']) {
      expect(
        GitHubSearchQuerySchema.safeParse({
          ...result.next[key].query,
          goal: 'test',
          reasoning: 'test',
        }).success
      ).toBe(true);
    }
  });
});
