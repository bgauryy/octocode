import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  search: vi.fn(),
  get: vi.fn(),
  comments: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      issues: { get: mocks.get, listComments: mocks.comments },
      search: { issuesAndPullRequests: mocks.search },
      repos: { get: async () => ({ data: { full_name: 'o/r' } }) },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'history-efficiency',
  OctokitWithThrottling: class {},
}));

import {
  getMultipleGitHubHistoryItems,
  searchMultipleGitHubHistory,
} from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import {
  GitHubGetHistoryItemQueryLocalSchema,
  GitHubSearchHistoryQueryLocalSchema,
} from '../../src/tools/github_search_pull_requests/historySchemes.js';
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { transformPullRequestItemFromSearch } from '../../src/github/prContentFetcher/transform.js';
import { formatPRForResponse } from '../../src/github/prTransformation.js';
import { transformPullRequestResult } from '../../src/providers/github/githubPullRequests.js';

const issue = (number: number) => ({
  number,
  title: `Issue ${number}`,
  body: 'abcdefgh',
  state: 'open',
});
async function execute(query: Record<string, unknown>, detail = false) {
  const result = await (
    detail ? getMultipleGitHubHistoryItems : searchMultipleGitHubHistory
  )({ queries: [query] } as never);
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

describe('history public continuations and request budgets', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('retries incomplete issue search instead of caching a temporary empty result', async () => {
    mocks.search.mockResolvedValueOnce({
      data: { items: [], total_count: 0, incomplete_results: true },
      headers: {},
    });
    mocks.search.mockResolvedValueOnce({
      data: { items: [issue(1)], total_count: 1, incomplete_results: false },
      headers: {},
    });
    const first = await execute({
      operation: 'issues',
      owner: 'o',
      repo: 'r',
      keywords: ['fix'],
    });
    expect(first.isPartial).toBe(true);
    const next = GitHubSearchHistoryQueryLocalSchema.parse(
      first.next.retry.query
    );
    const second = await execute(next);
    expect(second.issues.map((i: { number: number }) => i.number)).toEqual([1]);
    expect(mocks.search).toHaveBeenCalledTimes(2);
  });

  it('continues a short issue search page and covers the complete fixture', async () => {
    mocks.search.mockResolvedValueOnce({
      data: { items: [issue(1)], total_count: 3, incomplete_results: true },
      headers: {},
    });
    mocks.search.mockResolvedValueOnce({
      data: {
        items: [issue(2), issue(3)],
        total_count: 3,
        incomplete_results: false,
      },
      headers: {},
    });
    const first = await execute({
      operation: 'issues',
      owner: 'o',
      repo: 'r',
      keywords: ['fix'],
      pageSize: 2,
    });
    expect(first.next.nextPage).toBeDefined();
    const second = await execute(
      GitHubSearchHistoryQueryLocalSchema.parse(first.next.nextPage.query)
    );
    expect([...first.issues, ...second.issues].map(i => i.number)).toEqual([
      1, 2, 3,
    ]);
    expect(mocks.search).toHaveBeenCalledTimes(2);
  });

  it('reassembles body windows with one upstream issue GET', async () => {
    mocks.get.mockResolvedValue({ data: issue(1), headers: {} });
    let query: Record<string, unknown> | undefined = {
      operation: 'issue',
      owner: 'o',
      repo: 'r',
      number: 1,
      charLength: 3,
    };
    let body = '';
    for (let budget = 0; query && budget < 5; budget++) {
      const data = await execute(
        GitHubGetHistoryItemQueryLocalSchema.parse(query),
        true
      );
      body += data.issues[0].body;
      query = data.next?.continueBody?.query;
    }
    expect(body).toBe('abcdefgh');
    expect(mocks.get).toHaveBeenCalledOnce();
  });

  it('preserves comment page size and reuses raw pages across body windows and bot filters', async () => {
    mocks.get.mockResolvedValue({ data: issue(1), headers: {} });
    const comments = [1, 2, 3].map(id => ({
      id,
      body: `comment ${id}`,
      user: { login: id === 2 ? 'ci[bot]' : 'human' },
    }));
    mocks.comments.mockImplementation(async ({ page, per_page }) => ({
      data: comments.slice((page - 1) * per_page, page * per_page),
      headers:
        page * per_page < comments.length
          ? { link: '<https://api.github.com/comments?page=2>; rel="next"' }
          : {},
    }));
    const query = {
      operation: 'issue',
      owner: 'o',
      repo: 'r',
      number: 1,
      pageSize: 2,
      charLength: 3,
      content: { comments: { discussion: true } },
    };
    const first = await execute(query, true);
    expect(first, JSON.stringify(first)).toHaveProperty(
      'next.nextCommentsPage'
    );
    const continuation = GitHubGetHistoryItemQueryLocalSchema.parse(
      first.next.nextCommentsPage.query
    );
    expect(continuation).toMatchObject({ pageSize: 2, commentPage: 2 });
    const second = await execute(continuation, true);
    expect(
      [...first.issues[0].comments, ...second.issues[0].comments].map(c => c.id)
    ).toEqual(['1', '3']);
    const bots = await execute(
      {
        ...query,
        charOffset: 3,
        content: { comments: { discussion: true, includeBots: true } },
      },
      true
    );
    expect(bots.issues[0].comments.map((c: { id: string }) => c.id)).toEqual([
      '1',
      '2',
    ]);
    expect(mocks.get).toHaveBeenCalledOnce();
    expect(mocks.comments).toHaveBeenCalledTimes(2);
  });

  it('preserves merged PR state from search metadata without fetching details', async () => {
    const octokit = { rest: { pulls: { get: vi.fn() } } };
    const transformed = await transformPullRequestItemFromSearch(
      {
        ...issue(1),
        state: 'closed',
        html_url: 'https://github.com/o/r/pull/1',
        pull_request: { merged_at: '2026-09-01T00:00:00Z' },
      } as never,
      { owner: 'o', repo: 'r' },
      octokit as never
    );
    const data = transformPullRequestResult(
      { pullRequests: [formatPRForResponse(transformed)], totalCount: 1 },
      { projectId: 'o/r' }
    );
    expect(data.items[0]).toMatchObject({
      state: 'merged',
      mergedAt: '2026-09-01T00:00:00Z',
    });
    expect(octokit.rest.pulls.get).not.toHaveBeenCalled();
  });
});
