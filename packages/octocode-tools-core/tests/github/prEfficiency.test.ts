import { fetchCollectionPage } from '../../src/github/prContentFetcher/collectionPaging.js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ listCommits: vi.fn(), getCommit: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      pulls: { listCommits: mocks.listCommits },
      repos: { getCommit: mocks.getCommit },
    },
  }),
  OctokitWithThrottling: class {},
  resolveCacheAuthFingerprint: async () => 'pr-efficiency',
}));

import { fetchPRCommitsWithFiles } from '../../src/github/prContentFetcher/commits.js';
import { transformPullRequestResult } from '../../src/providers/github/githubPullRequests.js';
import { mapPullRequestProviderResultData } from '../../src/tools/providerMappers/pullRequests.js';
import { withSearchPageContinuation } from '../../src/tools/github_search_pull_requests/historySearchPagination.js';
import { GitHubSearchHistoryQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';
import { shapeCommits } from '../../src/tools/github_search_pull_requests/contentResponse/commentsShaping.js';

const commits = Array.from({ length: 6 }, (_, i) => ({
  sha: `commit-${i}`,
  commit: {
    message: `message-${i}`,
    author: { name: 'author', date: `2026-01-0${i + 1}T00:00:00Z` },
  },
}));
const files = [
  { filename: 'src/file.ts', status: 'modified', additions: 1, deletions: 0 },
];
const request = {
  commits: { includeFiles: true },
  patches: { mode: 'none' },
} as never;

describe('PR fetch efficiency and lossless metadata', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listCommits.mockResolvedValue({ data: commits, headers: {} });
    mocks.getCommit.mockImplementation(async ({ ref }) => ({
      data: { sha: ref, files },
      headers: {},
    }));
  });

  it('fetches commit summaries without detail requests', async () => {
    const result = await fetchPRCommitsWithFiles('o', 'r', 1, {
      content: { commits: {} },
    } as never);
    expect(result?.map(c => c.sha)).toEqual(commits.map(c => c.sha));
    expect(mocks.getCommit).toHaveBeenCalledTimes(0);
  });

  it('enriches only the displayed page and retains all summaries for one shaping pagination', async () => {
    const result = await fetchPRCommitsWithFiles('o', 'r', 1, {
      content: { commits: { includeFiles: true } },
      commitPage: 2,
      itemsPerPage: 2,
    } as never);
    const shaped = shapeCommits(
      { commits: result },
      { commitPage: 2, pageSize: 2 },
      request
    );
    expect(shaped.commits?.map(c => c.sha)).toEqual(['commit-2', 'commit-3']);
    expect(shaped.commits?.every(c => c.files?.length === 1)).toBe(true);
    expect(shaped.contentPagination?.commits.totalItems).toBe(6);
    expect(mocks.getCommit).toHaveBeenCalledTimes(2);
  });

  it('stops on a full final page when headers have no next link', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce({
        data: Array.from({ length: 100 }, (_, i) => i),
        headers: {},
      })
      .mockResolvedValueOnce({ data: [], headers: {} });
    const result = await fetchCollectionPage(
      { owner: 'o', repo: 'r', prNumber: 2, surface: 'reviews' },
      1,
      fetch
    );
    expect(result.items).toHaveLength(100);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it('exposes a short-page next link without eagerly fetching it', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce({
        data: [1],
        headers: {
          link: '<https://api.github.com/resource?page=2>; rel="next"',
        },
      })
      .mockResolvedValueOnce({ data: [2], headers: {} });
    const identity = {
      owner: 'o',
      repo: 'r',
      prNumber: 3,
      surface: 'reviews' as const,
    };
    const first = await fetchCollectionPage(identity, 1, fetch);
    expect(first.items).toEqual([1]);
    expect(first.collectionState).toEqual({ page: 1, hasMore: true });
    expect(fetch).toHaveBeenCalledTimes(1);
    const second = await fetchCollectionPage(identity, 2, fetch);
    expect([...first.items, ...second.items]).toEqual([1, 2]);
    expect(second.collectionState.hasMore).toBe(false);
    expect(fetch).toHaveBeenNthCalledWith(2, 2);
  });

  it('preserves requested commit files through both adapters', () => {
    const provider = transformPullRequestResult(
      {
        pullRequests: [
          {
            number: 1,
            title: 'PR',
            commitDetails: [
              { sha: 's', message: 'm', author: 'a', date: '', files },
            ],
          },
        ],
      } as never,
      {}
    );
    const mapped = mapPullRequestProviderResultData(provider);
    expect(mapped.pullRequests[0]?.commits?.[0]).toMatchObject({ files });
  });

  it('executes the schema-valid incomplete retry through both adapters and recovers all fixture results', () => {
    const queries: Record<string, unknown>[] = [];
    const execute = (query: Record<string, unknown>) => {
      queries.push(query);
      const provider = transformPullRequestResult(
        {
          pullRequests:
            queries.length === 1 ? [] : [{ number: 17, title: 'Recovered' }],
          incompleteResults: queries.length === 1,
          pagination: { currentPage: 4, totalPages: 4, hasMore: false },
        } as never,
        {}
      );
      return withSearchPageContinuation(
        mapPullRequestProviderResultData(provider).resultData as never,
        query,
        'pullRequests'
      ) as Record<string, any>;
    };
    const first = execute({
      operation: 'pullRequests',
      owner: 'o',
      repo: 'r',
      keywords: ['schema'],
      page: 4,
    });
    expect(first.incompleteResults).toBe(true);
    expect(first.isPartial).toBe(true);
    expect(first.next.retry.tool).toBe('ghSearchHistory');
    const retry = GitHubSearchHistoryQueryLocalSchema.parse(
      first.next.retry.query
    );
    const second = execute(retry);
    expect(queries[1]).toMatchObject({ page: 4, keywords: ['schema'] });
    expect(
      second.pullRequests.map((pr: { number: number }) => pr.number)
    ).toEqual([17]);
    expect(second.isPartial).toBeUndefined();
  });
});
