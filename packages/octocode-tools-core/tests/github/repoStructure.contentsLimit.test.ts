import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';
import { BaseGitHubSearchQuerySchema } from '@octocodeai/octocode-core/schema';
import { getRepoStructure } from '../../src/providers/github/githubStructure.js';
import { exploreRepositoryStructure } from '../../src/tools/github_view_repo_structure/execution.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';

const mocks = vi.hoisted(() => ({
  contents: vi.fn(),
  tree: vi.fn(),
  treesEnabled: false,
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({ rest: { repos: { getContent: mocks.contents } } }),
  resolveDefaultBranch: async () => 'main',
  resolveCacheAuthFingerprint: async () => 'contents-limit-test',
}));
vi.mock('../../src/utils/http/cache/conditional.js', () => ({
  withDataCacheConditional: async (
    _key: string,
    operation: (input: object) => Promise<{ value: unknown }>
  ) => (await operation({})).value,
}));
vi.mock('../../src/github/repoStructureTree.js', () => ({
  isGitStructureTreesEnabled: () => mocks.treesEnabled,
  fetchStructureViaGitTree: mocks.tree,
}));

const entries = (count: number) =>
  Array.from({ length: count }, (_, i) => ({
    path: `file-${String(i).padStart(4, '0')}.ts`,
    name: `file-${String(i).padStart(4, '0')}.ts`,
    type: 'file',
    size: 1,
  }));

async function execute(query: Record<string, unknown>) {
  const data = await exploreRepositoryStructure(
    query as never,
    {} as never,
    () => ({ provider: { getRepoStructure } }) as never
  );
  const finalized = buildGitHubSearchFinalizer()({
    queries: [query],
    results: [{ index: 0, data }],
    config: { toolName: 'ghSearch' },
  } as never);
  return (
    finalized.structuredContent as {
      results: Array<{
        data: {
          structure: Array<{ files: string[] }>;
          isPartial?: boolean;
          terminalLimit?: boolean;
          partialReasons?: string[];
          status?: string;
          statusCode?: number;
          next?: {
            nextPage?: { tool: string; query: Record<string, unknown> };
          };
        };
      }>;
    }
  ).results[0]!.data;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.treesEnabled = false;
  mocks.contents.mockResolvedValue({ data: entries(1000), headers: {} });
  mocks.tree.mockRejectedValue(new Error('tree unavailable'));
});

describe('Contents directory completeness', () => {
  it.each(['direct', 'recursive', 'tree fallback'])(
    'preserves all reachable pages with a terminal diagnostic after failed recovery: %s',
    async mode => {
      mocks.treesEnabled = mode === 'tree fallback';
      let query: Record<string, unknown> = {
        operation: 'tree',
        owner: 'owner',
        repo: 'repo',
        branch: 'main',
        maxDepth: mode === 'direct' ? 1 : 2,
        pageSize: 100,
      };
      const received: string[] = [];
      for (let page = 1; page <= 10; page++) {
        const result = await execute(query);
        expect(result).toMatchObject({
          isPartial: true,
          terminalLimit: true,
          partialReasons: ['providerContentsLimit'],
          providerLimit: {
            reason: 'providerContentsLimit',
            maxEntriesPerDirectory: 1000,
            completeness: 'unknown',
          },
        });
        received.push(...result.structure.flatMap(entry => entry.files));
        const next = result.next?.nextPage;
        if (page === 10) {
          expect(next).toBeUndefined();
        } else {
          expect(next?.tool).toBe('ghSearch');
          expect(
            BaseGitHubSearchQuerySchema.safeParse(next!.query).success
          ).toBe(true);
          query = next!.query;
        }
      }
      expect(received).toEqual(entries(1000).map(entry => entry.name));
    }
  );

  it('recovers the full directory from a complete tree without increasing depth', async () => {
    mocks.tree.mockResolvedValue({
      items: entries(1001),
      truncated: false,
      rawResponseChars: 10,
    });
    let query: Record<string, unknown> = {
      operation: 'tree',
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      pageSize: 100,
    };
    const received: string[] = [];
    for (let page = 1; page <= 11; page++) {
      const result = await execute(query);
      expect(result.isPartial).toBeUndefined();
      expect(result.terminalLimit).toBeUndefined();
      received.push(...result.structure.flatMap(entry => entry.files));
      if (page === 11) expect(result.next?.nextPage).toBeUndefined();
      else {
        query = result.next!.nextPage!.query;
        expect(BaseGitHubSearchQuerySchema.safeParse(query).success).toBe(true);
      }
    }
    expect(received).toEqual(entries(1001).map(entry => entry.name));
    expect(mocks.tree).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ maxDepth: 1, pathPrefix: '' })
    );
  });

  it('retains original Contents entries when tree recovery is truncated', async () => {
    mocks.tree.mockResolvedValue({
      items: [],
      truncated: true,
      rawResponseChars: 10,
    });
    const result = await execute({
      operation: 'tree',
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      pageSize: 100,
    });
    expect(result).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['providerContentsLimit'],
    });
    expect(result.structure.flatMap(entry => entry.files)).toEqual(
      entries(100).map(entry => entry.name)
    );
  });

  it('does not add recovery calls or partial state below the provider bound', async () => {
    mocks.contents.mockResolvedValue({ data: entries(999), headers: {} });
    const result = await execute({
      operation: 'tree',
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      pageSize: 100,
    });
    expect(result.isPartial).toBeUndefined();
    expect(mocks.tree).not.toHaveBeenCalled();
  });

  it.each([401, 403, 429])(
    'propagates recovery HTTP %i instead of returning partial success',
    async status => {
      mocks.tree.mockRejectedValue(
        new RequestError('denied', status, {
          request: {
            method: 'GET',
            url: 'https://api.github.com/tree',
            headers: {},
          },
        })
      );
      const result = await execute({
        operation: 'tree',
        owner: 'owner',
        repo: 'repo',
        branch: 'main',
        pageSize: 100,
      });
      expect(result).toMatchObject({ status: 'error', statusCode: status });
      expect(result.isPartial).toBeUndefined();
      expect(result.structure).toBeUndefined();
    }
  );
});
