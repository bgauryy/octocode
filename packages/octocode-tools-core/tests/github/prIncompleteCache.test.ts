import { expect, it, vi } from 'vitest';

const search = vi.hoisted(() => vi.fn());
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      repos: { get: async () => ({ data: { full_name: 'o/r' } }) },
      search: { issuesAndPullRequests: search },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'pr-incomplete-cache',
  OctokitWithThrottling: class {},
}));
import { searchPullRequests } from '../../src/providers/github/githubPullRequests.js';
import {
  mapPullRequestToolQuery,
  mapPullRequestProviderResultData,
} from '../../src/tools/providerMappers/pullRequests.js';
import { withSearchPageContinuation } from '../../src/tools/github_search_pull_requests/historySearchPagination.js';
import { GitHubSearchHistoryQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';

it('executes an incomplete-search continuation through the real cache and caches the recovered response', async () => {
  search.mockResolvedValueOnce({
    data: { items: [], total_count: 0, incomplete_results: true },
  });
  search.mockResolvedValueOnce({
    data: {
      items: [
        {
          number: 17,
          title: 'Recovered',
          html_url: '',
          user: { login: 'a' },
          pull_request: {},
        },
      ],
      total_count: 1,
      incomplete_results: false,
    },
  });
  const execute = async (query: Record<string, unknown>) => {
    const response = await searchPullRequests(
      mapPullRequestToolQuery(query as never)
    );
    expect(response.error).toBeUndefined();
    return withSearchPageContinuation(
      mapPullRequestProviderResultData(response.data!).resultData as never,
      query,
      'pullRequests'
    ) as Record<string, any>;
  };
  const first = await execute(
    GitHubSearchHistoryQueryLocalSchema.parse({
      operation: 'pullRequests',
      owner: 'o',
      repo: 'r',
      keywords: ['schema'],
      page: 4,
    })
  );
  expect(first.incompleteResults).toBe(true);
  const retry = GitHubSearchHistoryQueryLocalSchema.parse(
    first.next.retry.query
  );
  const recovered = await execute(retry);
  expect(
    recovered.pullRequests.map((pr: { number: number }) => pr.number)
  ).toEqual([17]);
  expect(recovered.isPartial).toBeUndefined();
  await execute(retry);
  expect(search).toHaveBeenCalledTimes(2);
});
