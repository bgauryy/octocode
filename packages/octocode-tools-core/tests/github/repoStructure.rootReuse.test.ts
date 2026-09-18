import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';
import { GitHubSearchQuerySchema } from '@octocodeai/octocode-core/schema';
import { cache } from '../../src/utils/http/cache/store.js';
import { getRawResponseChars } from '../../src/utils/response/charSavings.js';
import { getRepoStructure } from '../../src/providers/github/githubStructure.js';
import { exploreRepositoryStructure } from '../../src/tools/github_view_repo_structure/execution.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure/fetchOrchestration.js';

const mocks = vi.hoisted(() => ({
  content: vi.fn(),
  branch: vi.fn(),
  tree: vi.fn(),
  defaultBranch: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      repos: { getContent: mocks.content, getBranch: mocks.branch },
      git: { getTree: mocks.tree },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'root-reuse-fixture',
  resolveDefaultBranch: mocks.defaultBranch,
}));
vi.mock('../../src/utils/http/cache/diskStore.js', () => ({
  readDiskCache: async () => undefined,
  writeDiskCache: async () => {},
}));

const branch = 'a'.repeat(40);
const item = (path: string, type = 'file') => ({
  path,
  name: path.split('/').at(-1),
  type,
  size: 7,
  sha: `sha-${path}`,
  _links: { self: `https://api.github.com/fixture/${path}` },
});
const root = [
  item('root.ts'),
  item('src', 'dir'),
  item('linked', 'symlink'),
  item('vendor', 'submodule'),
];
const child = [item('src/one.ts'), item('src/two.ts')];
const payload = (data: unknown) => ({ data, headers: {} });
const treePayload = (items: ReturnType<typeof item>[], truncated = false) =>
  payload({
    tree: items.map(entry => ({
      ...entry,
      type: entry.type === 'dir' ? 'tree' : 'blob',
    })),
    truncated,
  });

beforeEach(() => {
  cache.flushAll();
  vi.resetAllMocks();
  vi.stubEnv('OCTOCODE_GH_STRUCTURE_TREES', '0');
  mocks.defaultBranch.mockResolvedValue('default-fixture');
  mocks.content.mockImplementation(async ({ path }: { path: string }) => {
    if (path === '') return payload(root);
    if (path === 'src') return payload(child);
    throw new Error(`Unexpected Contents request: ${path}`);
  });
  mocks.branch.mockRejectedValue(new Error('tree metadata unavailable'));
});
afterEach(() => vi.unstubAllEnvs());

async function run(query: Record<string, unknown>) {
  const parsed = GitHubSearchQuerySchema.parse(query);
  const data = await exploreRepositoryStructure(
    parsed as never,
    {} as never,
    () => ({ provider: { getRepoStructure } }) as never
  );
  const result = buildGitHubSearchFinalizer()({
    queries: [parsed],
    results: [{ index: 0, data }],
    config: { toolName: 'ghSearch' },
  } as never);
  return {
    rawChars: getRawResponseChars(data),
    public: (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!.data,
  };
}

const query = {
  operation: 'tree',
  owner: 'fixture',
  repo: 'root-reuse',
  branch,
  maxDepth: 2,
  pageSize: 2,
};

async function collect(extra: Record<string, unknown> = {}) {
  let current: Record<string, unknown> = { ...query, ...extra };
  const paths: string[] = [];
  const pages: Awaited<ReturnType<typeof run>>[] = [];
  for (let page = 0; page < 20; page++) {
    const result = await run(current);
    pages.push(result);
    expect(result.public.structure).toBeDefined();
    for (const entry of result.public.structure) {
      for (const name of [...entry.files, ...entry.folders])
        paths.push(entry.dir === '.' ? name : `${entry.dir}/${name}`);
    }
    const next = result.public.next?.nextPage;
    if (!next) return { paths, pages };
    expect(next.tool).toBe('ghSearch');
    expect(GitHubSearchQuerySchema.safeParse(next.query).success).toBe(true);
    expect(next.query).toMatchObject({ branch, maxDepth: 2, page: page + 2 });
    current = next.query;
  }
  throw new Error('Fixture continuation did not terminate');
}

describe('deep Contents root response reuse through the public tree path', () => {
  it('preserves default-branch rate guidance before any deep tree request', async () => {
    vi.stubEnv('OCTOCODE_GH_STRUCTURE_TREES', '1');
    mocks.defaultBranch.mockRejectedValue(
      new RequestError('secondary rate limit', 429, {
        request: {
          method: 'GET',
          url: '/repos/fixture/root-reuse',
          headers: {},
        },
        response: {
          status: 429,
          url: '/repos/fixture/root-reuse',
          headers: {
            'retry-after': '90',
            'x-ratelimit-remaining': '7',
            'x-ratelimit-reset': '2000000000',
          },
          data: { message: 'secondary rate limit' },
        },
      })
    );
    const result = await viewGitHubRepositoryStructureAPI({
      owner: 'fixture',
      repo: 'root-reuse',
      maxDepth: 2,
    } as never);
    expect(result).toMatchObject({
      status: 429,
      retryAfter: 90,
      rateLimitRemaining: 7,
      rateLimitReset: 2000000000000,
    });
    expect(mocks.content).not.toHaveBeenCalled();
    expect(mocks.tree).not.toHaveBeenCalled();
    expect(mocks.branch).not.toHaveBeenCalled();
  });
  it.each(['contents', 'failed-tree'])(
    'loads root once and executes the full page union: %s',
    async mode => {
      vi.stubEnv(
        'OCTOCODE_GH_STRUCTURE_TREES',
        mode === 'contents' ? '0' : '1'
      );
      const result = await collect();
      expect(result.paths.sort()).toEqual([
        'root.ts',
        'src',
        'src/one.ts',
        'src/two.ts',
      ]);
      expect(new Set(result.paths).size).toBe(4);
      expect(result.pages).toHaveLength(2);
      expect(mocks.content.mock.calls.map(([args]) => args.path)).toEqual([
        '',
        'src',
      ]);
      for (const [args] of mocks.content.mock.calls)
        expect(args.ref).toBe(branch);
      for (const page of result.pages) {
        expect(page.public.resolvedBranch).toBe(branch);
        expect(page.rawChars).toBe(
          JSON.stringify(root).length + JSON.stringify(child).length
        );
      }
      expect(result.pages.at(-1)!.public.isPartial).not.toBe(true);
    }
  );

  it('fetches root normally after a truncated tree and preserves the complete available union', async () => {
    vi.stubEnv('OCTOCODE_GH_STRUCTURE_TREES', '1');
    const branchData = { commit: { commit: { tree: { sha: 'root-tree' } } } };
    const treeData = treePayload([item('root.ts')], true);
    mocks.branch.mockResolvedValue(payload(branchData));
    mocks.tree.mockResolvedValue(treeData);
    const result = await collect();
    expect(result.paths.sort()).toEqual([
      'root.ts',
      'src',
      'src/one.ts',
      'src/two.ts',
    ]);
    expect(mocks.content.mock.calls.map(([args]) => args.path)).toEqual([
      '',
      'src',
      '',
      'src',
    ]);
    for (const page of result.pages) {
      expect(page.public.partialReasons).toContain('providerTreeTruncated');
      expect(page.rawChars).toBe(
        [branchData, treeData.data, root, child].reduce(
          (sum, data) => sum + JSON.stringify(data).length,
          0
        )
      );
    }
  });

  it('retains a failed child as partial and executes its emitted retry', async () => {
    mocks.content.mockImplementation(async ({ path }: { path: string }) => {
      if (path === '') return payload(root);
      throw new Error('child temporarily unavailable');
    });
    const first = await run({ ...query, pageSize: 100 });
    expect(first.public.partialReasons).toContain('partialTreeFailures');
    expect(first.rawChars).toBe(JSON.stringify(root).length);
    const retry = first.public.next.retry;
    expect(GitHubSearchQuerySchema.safeParse(retry.query).success).toBe(true);
    mocks.content.mockImplementation(async ({ path }: { path: string }) =>
      payload(path === '' ? root : child)
    );
    const recovered = await run(retry.query);
    expect(recovered.public.isPartial).not.toBe(true);
    expect(mocks.content.mock.calls.map(([args]) => args.path)).toEqual([
      '',
      'src',
      '',
      'src',
    ]);
  });

  it.each(['complete', 'truncated'])(
    'retains root saturation recovery and accounting when the tree is %s',
    async mode => {
      const initial = Array.from({ length: 1000 }, (_, i) =>
        item(`file-${String(i).padStart(4, '0')}.ts`)
      );
      const recovered = [...initial, item('recovered.ts')];
      const branchData = { commit: { commit: { tree: { sha: 'root-tree' } } } };
      const treeData = treePayload(recovered, mode === 'truncated');
      mocks.content.mockResolvedValue(payload(initial));
      mocks.branch.mockResolvedValue(payload(branchData));
      mocks.tree.mockResolvedValue(treeData);
      const result = await collect({ pageSize: 100 });
      const expected = mode === 'complete' ? recovered : initial;
      expect(result.paths.sort()).toEqual(
        expected.map(entry => entry.path).sort()
      );
      const acquisitions = mode === 'complete' ? 1 : 10;
      expect(mocks.content).toHaveBeenCalledTimes(acquisitions);
      expect(mocks.tree).toHaveBeenCalledTimes(acquisitions);
      for (const page of result.pages) {
        expect(page.rawChars).toBe(
          [initial, branchData, treeData.data].reduce(
            (sum, data) => sum + JSON.stringify(data).length,
            0
          )
        );
        if (mode === 'truncated') {
          expect(page.public).toMatchObject({
            isPartial: true,
            terminalLimit: true,
          });
          expect(page.public.partialReasons).toContain('providerContentsLimit');
        } else expect(page.public.terminalLimit).not.toBe(true);
      }
    }
  );

  it.each([401, 403, 429])(
    'propagates child HTTP %i without refetching root',
    async status => {
      mocks.content.mockImplementation(async ({ path }: { path: string }) => {
        if (path === '') return payload(root);
        throw new RequestError('denied', status, {
          request: { method: 'GET', url: '/fixture/src', headers: {} },
        });
      });
      const result = await run(query);
      expect(result.public).toMatchObject({
        status: 'error',
        statusCode: status,
      });
      expect(result.public.structure).toBeUndefined();
      expect(mocks.content.mock.calls.map(([args]) => args.path)).toEqual([
        '',
        'src',
      ]);
    }
  );
});
