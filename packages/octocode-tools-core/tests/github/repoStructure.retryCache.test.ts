import { beforeEach, describe, expect, it, vi } from 'vitest';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure/fetchOrchestration.js';

const mocks = vi.hoisted(() => ({
  cache: new Map<string, unknown>(),
  recursive: vi.fn(),
  resolve: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({}),
  resolveCacheAuthFingerprint: async () => 'retry-cache-fixture',
}));
vi.mock('../../src/utils/http/cache/conditional.js', () => ({
  withDataCacheConditional: async (
    key: string,
    operation: () => Promise<{ value: unknown }>,
    options: { shouldCache: (value: unknown) => boolean }
  ) => {
    if (mocks.cache.has(key)) return mocks.cache.get(key);
    const { value } = await operation({} as never);
    if (options.shouldCache(value)) mocks.cache.set(key, value);
    return value;
  },
}));
vi.mock('../../src/github/repoStructure/contentResolution.js', () => ({
  resolveContentWithBranchFallback: mocks.resolve,
  mapApiItems: () => [],
}));
vi.mock('../../src/github/repoStructureTree.js', () => ({
  isGitStructureTreesEnabled: () => false,
}));
vi.mock('../../src/github/repoStructureRecursive.js', () => ({
  fetchDirectoryContentsRecursivelyAPI: mocks.recursive,
  getRecursiveFetchFailureCount: (items: unknown[]) =>
    items.length === 1 ? 1 : 0,
}));

beforeEach(() => {
  mocks.cache.clear();
  vi.clearAllMocks();
});

describe('partial tree retries bypass incomplete cached data', () => {
  it('separates size-enriched and plain tree cache entries', async () => {
    mocks.resolve.mockResolvedValue({ data: [], workingBranch: 'main' });
    mocks.recursive.mockResolvedValue([
      { path: 'index.ts', name: 'index.ts', type: 'file', size: 10 },
      { path: 'other.ts', name: 'other.ts', type: 'file', size: 20 },
    ]);
    const query = {
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      maxDepth: 2,
      page: 1,
      itemsPerPage: 100,
    };
    const plain = await viewGitHubRepositoryStructureAPI(query as never);
    expect(plain).not.toHaveProperty('fileSizeMap');
    const sized = await viewGitHubRepositoryStructureAPI({
      ...query,
      includeSizes: true,
    } as never);
    expect(sized).toMatchObject({
      fileSizeMap: { '.': { 'index.ts': 10, 'other.ts': 20 } },
    });
    await viewGitHubRepositoryStructureAPI({
      ...query,
      includeSizes: true,
    } as never);
    expect(mocks.recursive).toHaveBeenCalledTimes(2);
  });
  it('re-executes a failed subtree on the same-page retry and caches the recovered tree', async () => {
    mocks.resolve.mockResolvedValue({ data: [], workingBranch: 'main' });
    const first = { path: 'index.ts', name: 'index.ts', type: 'file' };
    const second = { path: 'recovered.ts', name: 'recovered.ts', type: 'file' };
    mocks.recursive
      .mockResolvedValueOnce([first])
      .mockResolvedValue([first, second]);
    const query = {
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      maxDepth: 2,
      page: 1,
      itemsPerPage: 100,
    };
    const incomplete = await viewGitHubRepositoryStructureAPI(query as never);
    expect(incomplete).toMatchObject({
      isPartial: true,
      partialReasons: ['partialTreeFailures'],
    });
    const recovered = await viewGitHubRepositoryStructureAPI(query as never);
    expect(recovered).toMatchObject({
      structure: { '.': { files: ['index.ts', 'recovered.ts'] } },
    });
    expect(recovered).not.toHaveProperty('isPartial');
    await viewGitHubRepositoryStructureAPI(query as never);
    expect(mocks.recursive).toHaveBeenCalledTimes(2);
  });
});
