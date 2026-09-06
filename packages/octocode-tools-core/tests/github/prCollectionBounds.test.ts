import { beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  discussion: vi.fn(),
  inline: vi.fn(),
  reviews: vi.fn(),
  commits: vi.fn(),
  files: vi.fn(),
  detail: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      issues: { listComments: mocks.discussion },
      pulls: {
        listReviewComments: mocks.inline,
        listReviews: mocks.reviews,
        listCommits: mocks.commits,
        listFiles: mocks.files,
        get: async () => ({
          data: {
            number: 91,
            title: 'Bounds',
            changed_files: 3000,
            additions: 9999,
            deletions: 8888,
            html_url: '',
            user: { login: 'a' },
          },
        }),
      },
      repos: { getCommit: mocks.detail },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'bounds-review',
  OctokitWithThrottling: class {},
}));
vi.mock('../../src/providers/factory.js', () => ({
  getProvider: (_type: unknown, options: { authInfo?: never }) => ({
    type: 'github',
    capabilities: {},
    searchPullRequests: async (query: never) =>
      (
        await import('../../src/providers/github/githubPullRequests.js')
      ).searchPullRequests(query, options.authInfo),
  }),
}));
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';

async function execute(content: Record<string, unknown>) {
  const result = await getMultipleGitHubHistoryItems({
    queries: [
      GitHubGetHistoryItemQueryLocalSchema.parse({
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        number: 91,
        pageSize: 1,
        content,
      }),
    ],
  });
  return (result.structuredContent as any).results[0].data;
}
beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
});

async function run(query: Record<string, unknown>) {
  const result = await getMultipleGitHubHistoryItems({
    queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
  });
  const row = (result.structuredContent as any).results[0];
  expect(row.data.error).toBeUndefined();
  return row;
}

it('advances empty filtered batches and stops fetching exhausted mixed comment sources', async () => {
  mocks.discussion.mockImplementation(async ({ page }) => ({
    data: [
      {
        id: page,
        body: page === 1 ? 'unmatched' : 'needle',
        user: { login: 'human' },
        created_at: '',
        updated_at: '',
      },
    ],
    headers:
      page === 1
        ? { link: '<https://api.github.com/x?page=2>; rel="next"' }
        : {},
  }));
  mocks.inline.mockResolvedValue({
    data: [
      { id: 9, body: 'needle', path: 'a.ts', user: { login: 'robot[bot]' } },
    ],
    headers: {},
  });
  const first = await run({
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    matchString: 'needle',
    content: { comments: { discussion: true, reviewInline: true } },
    pageSize: 1,
  });
  expect(first.data.pullRequests[0].comments ?? []).toEqual([]);
  expect(first.meta.diagnostics.partial).toBe(true);
  expect(first.data.next.nextCommentsPage.query.collectionPages).toEqual({
    discussion: 2,
    inline: 0,
  });
  const second = await run(first.data.next.nextCommentsPage.query);
  expect(
    second.data.pullRequests[0].comments.map((c: { id: string }) => c.id)
  ).toEqual(['2']);
  expect(second.data.next?.nextCommentsPage).toBeUndefined();
  expect(mocks.inline).toHaveBeenCalledTimes(1);
  expect(mocks.discussion).toHaveBeenCalledTimes(2);
});

it.each(['files', 'commits'] as const)(
  'recovers the complete %s provider-cap fixture with bounded requests and typed terminal state',
  async surface => {
    const total = surface === 'files' ? 3000 : 250;
    mocks[surface].mockImplementation(async ({ page, per_page }) => ({
      data: Array.from(
        { length: Math.min(per_page, total - (page - 1) * per_page) },
        (_, i) => {
          const n = (page - 1) * per_page + i;
          return surface === 'files'
            ? {
                filename: `f-${n}`,
                status: 'added',
                additions: 1,
                deletions: 0,
              }
            : {
                sha: `s-${n}`,
                commit: {
                  message: 'm',
                  author: { name: 'a', date: new Date(n * 1000).toISOString() },
                },
              };
        }
      ),
      headers:
        page * per_page < total
          ? { link: '<https://api.github.com/x>; rel="next"' }
          : {},
    }));
    let query: Record<string, unknown> | undefined = {
      operation: 'pullRequest',
      owner: 'o',
      repo: 'r',
      number: 91,
      pageSize: 100,
      content: surface === 'files' ? { changedFiles: true } : { commits: {} },
    };
    const seen: string[] = [];
    let terminal: any;
    for (let budget = 0; query && budget < 35; budget++) {
      const before = mocks[surface].mock.calls.length;
      const row = await run(query);
      expect(mocks[surface].mock.calls.length - before).toBeLessThanOrEqual(1);
      const pr = row.data.pullRequests[0];
      if (surface === 'files')
        expect(pr).toMatchObject({
          changedFilesCount: 3000,
          additions: 9999,
          deletions: 8888,
        });
      seen.push(
        ...(surface === 'files'
          ? pr.changedFiles.map((f: { path: string }) => f.path)
          : pr.commits.map((c: { sha: string }) => c.sha))
      );
      terminal = row;
      query =
        row.data.next?.[
          surface === 'files' ? 'nextChangedFilesPage' : 'nextCommitsPage'
        ]?.query;
    }
    expect(query).toBeUndefined();
    expect(seen).toEqual(
      Array.from(
        { length: total },
        (_, i) => `${surface === 'files' ? 'f' : 's'}-${i}`
      )
    );
    expect(new Set(seen).size).toBe(total);
    expect(terminal.data.terminalLimit).toBe(true);
    expect(terminal.meta.diagnostics.partial).toBe(true);
    expect(mocks[surface]).toHaveBeenCalledTimes(surface === 'files' ? 30 : 5);
  }
);

it('reconstructs review bodies across both output pages and provider batches', async () => {
  mocks.reviews.mockImplementation(async ({ page }) => ({
    data: Array.from({ length: 3 }, (_, i) => ({
      id: (page - 1) * 3 + i,
      body: `review-${page}-${i}-text`,
      user: { login: 'human' },
      state: 'APPROVED',
    })),
    headers: page < 2 ? { link: '<https://api.github.com/x>; rel="next"' } : {},
  }));
  let query: Record<string, unknown> | undefined = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    pageSize: 2,
    charLength: 4,
    minify: 'none',
    content: { reviews: true },
  };
  const bodies: Record<string, string> = {};
  for (let budget = 0; query && budget < 30; budget++) {
    const { data } = await run(query);
    for (const r of data.pullRequests[0].reviews)
      bodies[r.id] = (bodies[r.id] ?? '') + (r.body ?? '');
    query =
      data.next?.continueReviewBody?.query ?? data.next?.nextReviewsPage?.query;
  }
  expect(query).toBeUndefined();
  expect(bodies).toEqual(
    Object.fromEntries(
      Array.from({ length: 6 }, (_, i) => [
        String(i),
        `review-${Math.floor(i / 3) + 1}-${i % 3}-text`,
      ])
    )
  );
  expect(mocks.reviews).toHaveBeenCalledTimes(2);
});

it.each(['discussion', 'inline', 'reviews'] as const)(
  'bounds acquisition and recovers every %s item through public continuations',
  async surface => {
    mocks[surface].mockImplementation(async ({ page }: { page: number }) => ({
      data: Array.from({ length: page < 3 ? 100 : 5 }, (_, i) => ({
        id: (page - 1) * 100 + i + 1,
        body: 'body',
        user: { login: 'human' },
        state: 'APPROVED',
        created_at: '',
        updated_at: '',
        path: 'a.ts',
      })),
      headers:
        page < 3
          ? {
              link: `<https://api.github.com/resource?page=${page + 1}>; rel="next"`,
            }
          : {},
    }));
    let query: Record<string, unknown> | undefined = {
      operation: 'pullRequest',
      owner: 'o',
      repo: 'r',
      number: 91,
      pageSize: 17,
      content:
        surface === 'reviews'
          ? { reviews: true }
          : {
              comments:
                surface === 'inline'
                  ? { reviewInline: true }
                  : { discussion: true },
            },
    };
    const seen: string[] = [];
    for (let budget = 0; query && budget < 30; budget++) {
      const before = mocks[surface].mock.calls.length;
      const result = await getMultipleGitHubHistoryItems({
        queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
      });
      const data = (result.structuredContent as any).results[0].data;
      expect(data.error).toBeUndefined();
      expect(mocks[surface].mock.calls.length - before).toBeLessThanOrEqual(1);
      const rows =
        data.pullRequests[0][surface === 'reviews' ? 'reviews' : 'comments'];
      expect(rows.length).toBeLessThanOrEqual(17);
      seen.push(...rows.map((r: { id: string }) => r.id));
      query =
        data.next?.[
          surface === 'reviews' ? 'nextReviewsPage' : 'nextCommentsPage'
        ]?.query;
    }
    expect(query).toBeUndefined();
    expect(seen).toEqual(Array.from({ length: 205 }, (_, i) => String(i + 1)));
    expect(mocks[surface]).toHaveBeenCalledTimes(3);
  }
);

it('bounds nested commit files and executes exact-commit continuations', async () => {
  mocks.commits.mockResolvedValue({
    data: [{ sha: 'abc', commit: { message: 'commit', author: null } }],
    headers: {},
  });
  mocks.detail.mockImplementation(async ({ page }: { page: number }) => ({
    data: {
      sha: 'abc',
      parents: [],
      commit: { message: 'commit', author: null },
      files: Array.from({ length: page === 1 ? 100 : 1 }, (_, i) => ({
        filename: `file-${(page - 1) * 100 + i}.ts`,
        status: 'modified',
        additions: 1,
        deletions: 0,
        patch: '+hello',
      })),
    },
    headers:
      page === 1
        ? { link: '<https://api.github.com/resource?page=2>; rel="next"' }
        : {},
  }));
  const data = await execute({ commits: { includeFiles: true } });
  expect(data.error).toBeUndefined();
  expect(mocks.detail).toHaveBeenCalledTimes(1);
  const commit = data.pullRequests[0].commits[0];
  expect(commit.files).toHaveLength(1);
  const seen = commit.files.map((f: { filename: string }) => f.filename);
  let query = commit.next?.nextFilePage?.query;
  expect(query).toBeDefined();
  for (let budget = 0; query && budget < 110; budget++) {
    const before = mocks.detail.mock.calls.length;
    const result = await getMultipleGitHubHistoryItems({
      queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
    });
    const page = (result.structuredContent as any).results[0].data;
    expect(page.error).toBeUndefined();
    expect(mocks.detail.mock.calls.length - before).toBeLessThanOrEqual(1);
    seen.push(...page.files.map((f: { filename: string }) => f.filename));
    query = page.next?.nextFilePage?.query;
  }
  expect(query).toBeUndefined();
  expect(seen).toEqual(Array.from({ length: 101 }, (_, i) => `file-${i}.ts`));
  expect(mocks.detail).toHaveBeenCalledTimes(2);
});
