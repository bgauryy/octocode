import { beforeEach, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({ repos: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({ rest: { search: { repos: mocks.repos } } })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'fixture'),
}));
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { searchRepos } from '../../src/providers/github/githubSearch.js';
import { searchGitHubRepos } from '../../src/tools/github_search_repos/execution.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';
import { GitHubSearchQuerySchema } from '../../src/tools/github_search/scheme.js';

beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
});

it('executes the final public retry and bypasses incomplete cached results', async () => {
  mocks.repos
    .mockResolvedValueOnce({
      data: { items: [], total_count: 0, incomplete_results: true },
      headers: {},
    })
    .mockResolvedValueOnce({
      data: { items: [], total_count: 0, incomplete_results: false },
      headers: {},
    });
  const run = async (query: Record<string, unknown>) => {
    const { operation: _operation, pageSize, ...rest } = query;
    const data = await searchGitHubRepos(
      { ...rest, limit: pageSize } as never,
      {} as never,
      () => ({ provider: { searchRepos } }) as never
    );
    const result = buildGitHubSearchFinalizer()({
      queries: [query],
      results: [{ index: 0, data }],
      config: { toolName: 'ghSearch' },
    } as never);
    return (
      result.structuredContent as {
        results: Array<{ data: Record<string, any> }>;
      }
    ).results[0]!.data;
  };
  const first = await run({
    operation: 'repositories',
    keywords: ['fixture'],
    pageSize: 10,
  });
  expect(first).toMatchObject({ incompleteResults: true, isPartial: true });
  expect(
    GitHubSearchQuerySchema.safeParse(first.next.retry.query).success
  ).toBe(true);
  const second = await run(first.next.retry.query);
  expect(second.incompleteResults).not.toBe(true);
  expect(second.next?.retry).toBeUndefined();
  expect(mocks.repos).toHaveBeenCalledTimes(2);
});
