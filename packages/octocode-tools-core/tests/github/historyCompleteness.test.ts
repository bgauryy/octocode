import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getCommit: vi.fn(),
  compare: vi.fn(),
  repos: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: {
      repos: {
        getCommit: mocks.getCommit,
        compareCommitsWithBasehead: mocks.compare,
      },
      search: { repos: mocks.repos },
    },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'fixture'),
}));
vi.mock('../../src/utils/http/cache/dataCache.js', () => ({
  withDataCache: vi.fn(async (_key, fetcher) => fetcher()),
}));

import { fetchCommit } from '../../src/github/commit.js';
import { compareRefs } from '../../src/github/compare.js';
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';
import { searchRepos } from '../../src/providers/github/githubSearch.js';

const file = (name: string) => ({
  filename: name,
  status: 'modified',
  additions: 1,
  deletions: 0,
  patch: name,
});
const commit = (sha: string) => ({
  sha,
  commit: {
    message: sha,
    author: { name: 'Author', email: '', date: '2026-01-01T00:00:00Z' },
  },
  parents: [],
});
const link = '<https://api.github.com/fixture?page=2>; rel="next"';
function row(
  result: Awaited<ReturnType<typeof getMultipleGitHubHistoryItems>>
) {
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

describe('GitHub history completeness regression and request budget', () => {
  beforeEach(() => vi.resetAllMocks());

  it('finds a path on the second upstream commit-file page', async () => {
    mocks.getCommit
      .mockResolvedValueOnce({
        data: { ...commit('pinned'), files: [file('first.ts')] },
        headers: { link },
      })
      .mockResolvedValueOnce({
        data: { ...commit('pinned'), files: [file('later.ts')] },
        headers: {},
      });
    const first = row(
      await getMultipleGitHubHistoryItems({
        queries: [
          {
            operation: 'commit',
            owner: 'o',
            repo: 'r',
            ref: 'main',
            path: 'later.ts',
            includeDiff: true,
          },
        ],
      })
    );
    expect(first.error).toBeUndefined();
    expect(first.files ?? []).toEqual([]);
    expect(first.error).toBeUndefined();
    expect(first.isPartial).toBe(true);
    expect(mocks.getCommit).toHaveBeenCalledTimes(1);
    const next = GitHubGetHistoryItemQueryLocalSchema.parse(
      first.next.nextFilePage.query
    );
    const result = row(
      await getMultipleGitHubHistoryItems({ queries: [next] })
    );
    expect(result.changedFiles).toBe(1);
    expect(result.files.map((f: { filename: string }) => f.filename)).toEqual([
      'later.ts',
    ]);
    expect(mocks.getCommit).toHaveBeenCalledTimes(2);
    expect(mocks.getCommit.mock.calls[1]![0].ref).toBe('pinned');
  });

  it('does not report a complete result if a later upstream page fails', async () => {
    mocks.getCommit
      .mockResolvedValueOnce({
        data: { ...commit('pinned'), files: [file('first.ts')] },
        headers: { link },
      })
      .mockRejectedValueOnce(
        Object.assign(new Error('rate limited'), { status: 429 })
      );
    const first = row(
      await getMultipleGitHubHistoryItems({
        queries: [
          {
            operation: 'commit',
            owner: 'o',
            repo: 'r',
            ref: 'main',
            includeDiff: true,
          },
        ],
      })
    );
    expect(first.error).toBeUndefined();
    expect(first.isPartial).toBe(true);
    const result = row(
      await getMultipleGitHubHistoryItems({
        queries: [first.next.nextFilePage.query],
      })
    );
    expect(result.error).toBeDefined();
  });

  it('reports the commit provider file cap explicitly', async () => {
    mocks.getCommit.mockResolvedValue({
      data: {
        ...commit('pinned'),
        files: Array.from({ length: 100 }, (_, i) => file(`${i}.ts`)),
      },
      headers: {},
    });
    const result = await fetchCommit({
      owner: 'o',
      repo: 'r',
      ref: 'main',
      fileBatch: 30,
    });
    expect(result.data).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      providerLimit: { maxFiles: 3000 },
    });
  });

  it('executes public compare continuations and recovers all 405 commits once', async () => {
    const all = Array.from({ length: 405 }, (_, i) => commit(`sha-${i}`));
    mocks.compare.mockImplementation(async ({ page = 1, per_page = 250 }) => ({
      data: {
        status: 'ahead',
        ahead_by: 405,
        behind_by: 0,
        total_commits: 405,
        commits: all.slice((page - 1) * per_page, page * per_page),
        ...(page === 1 ? { files: [file('a.ts')] } : {}),
      },
      headers: page * per_page < 405 ? { link } : {},
    }));
    let query: Record<string, unknown> | undefined = {
      operation: 'compare',
      owner: 'o',
      repo: 'r',
      base: 'v1',
      head: 'v2',
      pageSize: 100,
    };
    const seen: string[] = [];
    for (let budget = 0; query && budget < 6; budget++) {
      expect(
        GitHubGetHistoryItemQueryLocalSchema.safeParse(query).success
      ).toBe(true);
      const data = row(
        await getMultipleGitHubHistoryItems({ queries: [query] } as never)
      );
      seen.push(...data.commits.map((c: { sha: string }) => c.sha));
      query = data.next?.nextPage?.query;
    }
    expect(seen).toEqual(all.map(c => c.sha));
    expect(mocks.compare).toHaveBeenCalledTimes(5);
  });

  it('does not claim zero changed files on later comparison commit pages', async () => {
    mocks.compare.mockResolvedValue({
      data: {
        status: 'ahead',
        ahead_by: 2,
        behind_by: 0,
        total_commits: 2,
        commits: [commit('second')],
      },
      headers: {},
    });
    const result = await compareRefs({
      owner: 'o',
      repo: 'r',
      base: 'v1',
      head: 'v2',
      page: 2,
    } as never);
    expect(result.data).not.toHaveProperty('changedFiles');
  });

  it('reports comparison file cap even when a path filter finds nothing', async () => {
    mocks.compare.mockResolvedValue({
      data: {
        status: 'ahead',
        ahead_by: 1,
        behind_by: 0,
        total_commits: 1,
        commits: [commit('one')],
        files: Array.from({ length: 300 }, (_, i) => file(`${i}.ts`)),
      },
      headers: {},
    });
    const result = await compareRefs({
      owner: 'o',
      repo: 'r',
      base: 'v1',
      head: 'v2',
      path: 'omitted.ts',
    });
    expect(result.data).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      providerLimit: { maxFiles: 300 },
    });
  });

  it('preserves incomplete repository search through the API and provider', async () => {
    mocks.repos.mockResolvedValue({
      data: { items: [], total_count: 0, incomplete_results: true },
      headers: {},
    });
    const result = await searchRepos({ keywords: ['fixture'], limit: 10 });
    expect(result.data).toHaveProperty('incompleteResults', true);
  });
});
