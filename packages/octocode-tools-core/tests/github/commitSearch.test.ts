import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ search: vi.fn(), walk: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      search: { commits: mocks.search },
      repos: { listCommits: mocks.walk },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'commit-search-tests',
  OctokitWithThrottling: class {},
}));
import { searchMultipleGitHubHistory } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import {
  GitHubSearchHistoryQueryLocalSchema,
  GitHubGetHistoryItemQueryLocalSchema,
} from '../../src/tools/github_search_pull_requests/historySchemes.js';
import { clearAllCache } from '../../src/utils/http/cache/management.js';

const query = {
  operation: 'commits',
  owner: 'o',
  repo: 'r',
  keywords: ['fix parser'],
  pageSize: 2,
};
const item = (sha: string) => ({
  sha,
  html_url: `https://github.com/o/r/commit/${sha}`,
  commit: {
    message: `fix parser ${sha}`,
    author: { name: 'Alice', email: 'a@example.com', date: '2026-01-01' },
    committer: { name: 'Bob', email: 'b@example.com', date: '2026-01-02' },
  },
  author: { login: 'alice' },
  parents: [],
});
async function execute(q: Record<string, unknown>) {
  const result = await searchMultipleGitHubHistory({ queries: [q] } as never);
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

describe('public commit-message search', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('executes schema-valid continuations and covers all matching commits with one request per page', async () => {
    mocks.search.mockResolvedValueOnce({
      data: {
        total_count: 3,
        incomplete_results: false,
        items: [item('a'), item('b')],
      },
    });
    mocks.search.mockResolvedValueOnce({
      data: { total_count: 3, incomplete_results: false, items: [item('c')] },
    });
    const first = await execute(query);
    expect(first.commits?.map((c: { sha: string }) => c.sha)).toEqual([
      'a',
      'b',
    ]);
    const next = GitHubSearchHistoryQueryLocalSchema.parse(
      first.next.nextPage.query
    );
    expect(next).toMatchObject({ keywords: query.keywords, page: 2 });
    const second = await execute(next);
    expect([...first.commits, ...second.commits].map(c => c.sha)).toEqual([
      'a',
      'b',
      'c',
    ]);
    expect(second.next?.nextPage).toBeUndefined();
    expect(mocks.search).toHaveBeenCalledTimes(2);
    expect(mocks.search).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({
        q: '"fix parser" repo:o/r',
        page: 1,
        per_page: 2,
      })
    );
    expect(mocks.walk).not.toHaveBeenCalled();
  });

  it.each([{ path: 'src/' }, { branch: 'feature' }])(
    'rejects unsupported keyword scope %j before network I/O',
    async scope => {
      expect(
        GitHubSearchHistoryQueryLocalSchema.safeParse({ ...query, ...scope })
          .success
      ).toBe(false);
      await execute({ ...query, ...scope });
      expect(mocks.search).not.toHaveBeenCalled();
      expect(mocks.walk).not.toHaveBeenCalled();
    }
  );

  it('preserves incomplete results with an executable retry', async () => {
    mocks.search.mockResolvedValueOnce({
      data: { total_count: 0, incomplete_results: true, items: [] },
    });
    mocks.search.mockResolvedValueOnce({
      data: {
        total_count: 1,
        incomplete_results: false,
        items: [item('recovered')],
      },
    });
    const first = await execute(query);
    expect(first.incompleteResults).toBe(true);
    expect(first.isPartial).toBe(true);
    const recovered = await execute(
      GitHubSearchHistoryQueryLocalSchema.parse(first.next.retry.query)
    );
    expect(recovered.commits.map((c: { sha: string }) => c.sha)).toEqual([
      'recovered',
    ]);
    expect(mocks.search).toHaveBeenCalledTimes(2);
  });

  it('reuses a complete search response without another upstream request', async () => {
    mocks.search.mockResolvedValue({
      data: {
        total_count: 1,
        incomplete_results: false,
        items: [item('cached')],
      },
    });
    const first = await execute(query);
    const second = await execute(query);
    expect(second.commits).toEqual(first.commits);
    expect(mocks.search).toHaveBeenCalledOnce();
  });

  it('reports the 1000-result terminal cap without an impossible next page', async () => {
    mocks.search.mockResolvedValue({
      data: {
        total_count: 1500,
        incomplete_results: false,
        items: [item('last')],
      },
    });
    const result = await execute({ ...query, pageSize: 100, page: 10 });
    expect(result.terminalLimit).toBe(true);
    expect(result.partialReasons).toContain('providerResultCap');
    expect(result.next?.nextPage).toBeUndefined();
    expect(mocks.search).toHaveBeenCalledOnce();
  });

  it('supports login/email filters and committer date bounds', async () => {
    mocks.search.mockResolvedValue({
      data: { total_count: 0, incomplete_results: false, items: [] },
    });
    await execute({
      ...query,
      author: 'alice',
      committer: 'b@example.com',
      since: '2026-01-01',
      until: '2026-02-01',
    });
    expect(mocks.search).toHaveBeenCalledWith(
      expect.objectContaining({
        q: '"fix parser" repo:o/r author:"alice" committer-email:"b@example.com" committer-date:2026-01-01..2026-02-01',
      })
    );
  });

  it('keeps keywords out of exact commit and compare contracts', () => {
    expect(
      GitHubGetHistoryItemQueryLocalSchema.safeParse({
        ...query,
        operation: 'commit',
        ref: 'abc',
      }).success
    ).toBe(false);
    expect(
      GitHubGetHistoryItemQueryLocalSchema.safeParse({
        ...query,
        operation: 'compare',
        base: 'main',
        head: 'next',
      }).success
    ).toBe(false);
  });

  it.each([
    { page: 11, pageSize: 100 },
    { since: 'not-a-date' },
    { owner: 'o OR repo:other/repo' },
  ])('rejects unsupported search bounds before I/O: %j', async invalid => {
    await execute({ ...query, ...invalid });
    expect(mocks.search).not.toHaveBeenCalled();
    expect(mocks.walk).not.toHaveBeenCalled();
  });

  it('resolves relative date windows and keeps qualifier-like message terms quoted', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-05T12:00:00Z'));
    try {
      mocks.search.mockResolvedValue({
        data: { total_count: 0, incomplete_results: false, items: [] },
      });
      await execute({
        ...query,
        keywords: ['repo:other/repo OR fix'],
        since: '2d',
      });
      expect(mocks.search).toHaveBeenCalledWith(
        expect.objectContaining({
          q: '"repo:other/repo OR fix" repo:o/r committer-date:>=2026-09-03T12:00:00.000Z',
        })
      );
    } finally {
      vi.useRealTimers();
    }
  });
});
