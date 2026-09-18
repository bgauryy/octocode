import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';
import { GitHubSearchQuerySchema } from '@octocodeai/octocode-core/schema';

const mocks = vi.hoisted(() => ({
  content: vi.fn(),
  branch: vi.fn(),
  tree: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      repos: { getContent: mocks.content, getBranch: mocks.branch },
      git: { getTree: mocks.tree },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'contents-limit-heldout',
  resolveDefaultBranch: async () => 'main',
}));
vi.mock('../../src/utils/http/cache/diskStore.js', () => ({
  readDiskCache: async () => undefined,
  writeDiskCache: async () => {},
}));

import { cache } from '../../src/utils/http/cache/store.js';
import { getRepoStructure } from '../../src/providers/github/githubStructure.js';
import { exploreRepositoryStructure } from '../../src/tools/github_view_repo_structure/execution.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';

const item = (path: string, type = 'file') => ({
  path,
  name: path.split('/').at(-1),
  type,
  sha: `sha-${path}`,
  size: 12,
});
const files = (count: number, prefix = '') =>
  Array.from({ length: count }, (_, i) =>
    item(`${prefix}entry-${String(i).padStart(4, '0')}.ts`)
  );
const contents = (data: unknown[]) => ({ data, headers: {} });
const tree = (entries: ReturnType<typeof item>[], truncated = false) => ({
  data: {
    tree: entries.map(entry => ({
      ...entry,
      type: entry.type === 'dir' ? 'tree' : 'blob',
    })),
    truncated,
  },
  headers: {},
});

beforeEach(() => {
  cache.flushAll();
  vi.resetAllMocks();
  vi.stubEnv('OCTOCODE_GH_STRUCTURE_TREES', '1');
  mocks.branch.mockResolvedValue({
    data: { commit: { commit: { tree: { sha: 'root-tree' } } } },
  });
});
afterEach(() => vi.unstubAllEnvs());

async function run(query: Record<string, unknown>) {
  const parsed = GitHubSearchQuerySchema.parse(query);
  const data = await exploreRepositoryStructure(
    parsed as never,
    {} as never,
    () => ({ provider: { getRepoStructure } }) as never
  );
  const finalized = buildGitHubSearchFinalizer()({
    queries: [parsed],
    results: [{ index: 0, data }],
    config: { toolName: 'ghSearch' },
  } as never);
  const row = (
    finalized.structuredContent as {
      results: Array<{ data?: Record<string, any> }>;
    }
  ).results[0]!;
  return { data, public: row.data ?? row };
}

function paths(result: Record<string, any>): string[] {
  const structure = result.structure as Array<{
    dir: string;
    files: string[];
    folders: string[];
  }>;
  return structure.flatMap(entry =>
    [...entry.files, ...entry.folders].map(name =>
      entry.dir === '.' ? name : `${entry.dir}/${name}`
    )
  );
}

async function collect(extra: Record<string, unknown> = {}) {
  let query: Record<string, unknown> = {
    operation: 'tree',
    owner: 'heldout',
    repo: 'contents-limit',
    branch: 'main',
    maxDepth: 1,
    pageSize: 37,
    ...extra,
  };
  const received: string[] = [];
  const pages: Record<string, any>[] = [];
  for (let page = 0; page < 40; page++) {
    const result = (await run(query)).public;
    expect(result.structure).toBeDefined();
    received.push(...paths(result));
    pages.push(result);
    const next = result.next?.nextPage;
    if (!next) return { received, pages };
    expect(next.tool).toBe('ghSearch');
    expect(GitHubSearchQuerySchema.safeParse(next.query).success).toBe(true);
    expect(next.query).toMatchObject({
      operation: 'tree',
      owner: 'heldout',
      repo: 'contents-limit',
      branch: 'main',
      maxDepth: query.maxDepth,
      pageSize: 37,
      ...(query.path ? { path: query.path } : {}),
    });
    query = next.query;
  }
  throw new Error(
    'Executable pagination did not terminate within fixture bound'
  );
}

describe('public tree Contents saturation held-out cases', () => {
  it.each([999, 1000])(
    'distinguishes %i upstream entries before presentation pagination',
    async count => {
      const fixture = files(count);
      mocks.content.mockResolvedValue(contents(fixture));
      mocks.tree.mockResolvedValue(tree(fixture));
      const { received, pages } = await collect();
      expect(received.sort()).toEqual(fixture.map(entry => entry.path).sort());
      expect(new Set(received).size).toBe(count);
      expect(pages).toHaveLength(Math.ceil(count / 37));
      expect(pages.at(-1)?.terminalLimit).not.toBe(true);
      expect(pages.at(-1)?.isPartial).not.toBe(true);
      expect(mocks.tree).toHaveBeenCalledTimes(count === 1000 ? 1 : 0);
      expect(mocks.content).toHaveBeenCalledTimes(1);
    }
  );

  it('detects raw saturation even when discovery filtering hides almost every entry', async () => {
    const initial = [
      item('visible.ts'),
      ...Array.from({ length: 999 }, (_, i) => item(`bundle-${i}.min.js`)),
    ];
    mocks.content.mockResolvedValue(contents(initial));
    mocks.tree.mockResolvedValue(tree([...initial, item('recovered.py')]));
    const { received, pages } = await collect();
    expect(received.sort()).toEqual(['recovered.py', 'visible.ts']);
    expect(mocks.tree).toHaveBeenCalledTimes(1);
    expect(pages.at(-1)?.terminalLimit).not.toBe(true);
  });

  it('resolves a nested Unicode scope and keeps maxDepth one during recovery', async () => {
    const prefix = 'packages/日本語';
    const fixture = files(1001);
    mocks.content.mockImplementation(async ({ path }: { path: string }) => {
      if (path === prefix)
        return contents(
          fixture.slice(0, 1000).map(entry => item(`${prefix}/${entry.path}`))
        );
      if (path === 'packages') return contents([item(prefix, 'dir')]);
      throw new Error(`Unexpected Contents scope: ${path}`);
    });
    mocks.tree.mockResolvedValue(
      tree([...fixture, item('nested', 'dir'), item('nested/too-deep.ts')])
    );
    const { received } = await collect({ path: prefix });
    expect(received.sort()).toEqual(
      [...fixture.map(entry => entry.path), 'nested'].sort()
    );
    expect(new Set(received).size).toBe(1002);
    expect(received).not.toContain('nested/too-deep.ts');
    expect(mocks.branch).not.toHaveBeenCalled();
    expect(mocks.tree).toHaveBeenCalledWith(
      expect.objectContaining({ tree_sha: `sha-${prefix}`, recursive: 'true' })
    );
  });

  it.each(['unavailable', 'truncated'])(
    'keeps available entries and typed terminal state when scoped recovery is %s',
    async mode => {
      const fixture = files(1000, 'src/');
      mocks.content.mockImplementation(async ({ path }: { path: string }) =>
        contents(
          path === 'src'
            ? fixture
            : mode === 'unavailable'
              ? []
              : [item('src', 'dir')]
        )
      );
      mocks.tree.mockResolvedValue(tree([item('entry-0000.ts')], true));
      const { received, pages } = await collect({ path: 'src' });
      expect(new Set(received).size).toBe(1000);
      expect(received.sort()).toEqual(
        files(1000)
          .map(entry => entry.path)
          .sort()
      );
      for (const page of pages) {
        expect(page).toMatchObject({ isPartial: true, terminalLimit: true });
        expect(page.partialReasons).toContain('providerContentsLimit');
        expect(page.providerLimit).toEqual({
          reason: 'providerContentsLimit',
          maxEntriesPerDirectory: 1000,
          completeness: 'unknown',
        });
      }
      if (mode === 'unavailable') expect(mocks.tree).not.toHaveBeenCalled();
    }
  );

  it('recovers a saturated child within the remaining recursive depth budget', async () => {
    vi.stubEnv('OCTOCODE_GH_STRUCTURE_TREES', '0');
    const fixture = files(1001);
    mocks.content.mockImplementation(async ({ path }: { path: string }) => {
      if (path === 'src') return contents([item('src/nested', 'dir')]);
      if (path === 'src/nested')
        return contents(
          fixture.slice(0, 1000).map(entry => item(`src/nested/${entry.path}`))
        );
      throw new Error(`Unexpected Contents scope: ${path}`);
    });
    mocks.tree.mockResolvedValue(
      tree([...fixture, item('child', 'dir'), item('child/too-deep.ts')])
    );
    const { received, pages } = await collect({ path: 'src', maxDepth: 2 });
    expect(received.sort()).toEqual(
      [
        'nested',
        'nested/child',
        ...fixture.map(entry => `nested/${entry.path}`),
      ].sort()
    );
    expect(new Set(received).size).toBe(1003);
    expect(pages.at(-1)?.terminalLimit).not.toBe(true);
    expect(mocks.tree).toHaveBeenCalledTimes(1);
    expect(mocks.tree).toHaveBeenCalledWith(
      expect.objectContaining({ tree_sha: 'sha-src/nested', recursive: 'true' })
    );
  });

  it.each([
    ['contents', 401],
    ['contents', 403],
    ['contents', 429],
    ['recovery', 401],
    ['recovery', 403],
    ['recovery', 429],
  ] as const)(
    'does not turn %s status %i into a successful listing',
    async (stage, status) => {
      const error = new RequestError('held-out access failure', status, {
        request: {
          method: 'GET',
          url: '/repos/heldout/contents-limit/contents',
          headers: {},
        },
        response: {
          status,
          url: '/repos/heldout/contents-limit/contents',
          headers: { 'retry-after': '7', 'x-ratelimit-remaining': '0' },
          data: { message: 'held-out access failure' },
        },
      });
      if (stage === 'contents') mocks.content.mockRejectedValue(error);
      else {
        mocks.content.mockResolvedValue(contents(files(1000)));
        mocks.tree.mockRejectedValue(error);
      }
      const result = await run({
        operation: 'tree',
        owner: 'heldout',
        repo: 'contents-limit',
        branch: 'main',
        maxDepth: 1,
      });
      expect(result.data).toMatchObject({
        status: 'error',
        statusCode: status,
      });
      expect(result.public.structure).toBeUndefined();
      expect(mocks.content).toHaveBeenCalledTimes(1);
      expect(mocks.tree).toHaveBeenCalledTimes(stage === 'recovery' ? 1 : 0);
    }
  );
});
