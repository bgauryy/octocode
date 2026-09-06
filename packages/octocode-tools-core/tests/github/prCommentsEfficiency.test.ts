import { beforeEach, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({
  discussion: vi.fn(),
  inline: vi.fn(),
  reviews: vi.fn(),
  files: vi.fn(),
  commits: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      issues: { listComments: mocks.discussion },
      pulls: {
        listReviewComments: mocks.inline,
        listReviews: mocks.reviews,
        listFiles: mocks.files,
        listCommits: mocks.commits,
        get: async () => ({
          data: {
            number: 91,
            title: 'Comments',
            html_url: '',
            user: { login: 'a' },
          },
        }),
      },
    },
  }),
  resolveCacheAuthFingerprint: async (auth?: { token?: string }) =>
    auth?.token ?? 'anon',
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
import { fetchPRComments } from '../../src/github/prContentFetcher/comments.js';
import { getOctokit } from '../../src/github/client.js';

const row = (id: number, bot = false) => ({
  id,
  body: `message ${id}`,
  user: { login: bot ? 'robot[bot]' : 'human' },
  created_at: '',
  updated_at: '',
  path: 'a.ts',
});
const baseQuery = {
  operation: 'pullRequest',
  owner: 'o',
  repo: 'r',
  number: 91,
  pageSize: 2,
  content: {
    comments: { discussion: true, reviewInline: true },
    reviews: true,
  },
};
it.each([{}, { includeBots: true }, { file: 'a.ts' }])(
  'rejects a comments selector without a requested surface: %j',
  comments => {
    expect(
      GitHubGetHistoryItemQueryLocalSchema.safeParse({
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        number: 91,
        content: { comments },
      }).success
    ).toBe(false);
  }
);
async function execute(query: Record<string, unknown>, token = 'one') {
  const result = await getMultipleGitHubHistoryItems({
    queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
    authInfo: { token },
  } as never);
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}
beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
  mocks.discussion.mockResolvedValue({
    data: [row(1), row(2, true), row(3), row(4)],
    headers: {},
  });
  mocks.inline.mockResolvedValue({
    data: [row(5), row(6, true), row(7), row(8)],
    headers: {},
  });
  mocks.reviews.mockResolvedValue({
    data: [
      { id: 9, body: 'review', state: 'APPROVED', user: { login: 'human' } },
    ],
    headers: {},
  });
});
it('walks the complete filtered mixed comment union while downloading each surface once', async () => {
  let query: Record<string, unknown> | undefined = baseQuery;
  const seen: string[] = [];
  for (let budget = 0; query && budget < 5; budget++) {
    const data = await execute(query);
    seen.push(
      ...data.pullRequests[0].comments.map(
        (comment: { id: string }) => comment.id
      )
    );
    expect(data.pullRequests[0].reviews[0].id).toBe('9');
    query = data.next?.nextCommentsPage?.query;
  }
  expect(seen).toEqual(['5', '7', '8', '1', '3', '4']);
  expect(mocks.discussion).toHaveBeenCalledTimes(1);
  expect(mocks.inline).toHaveBeenCalledTimes(1);
  expect(mocks.reviews).toHaveBeenCalledTimes(1);
});
it('reuses unfiltered raw collections across bot selectors and isolates auth identities', async () => {
  await execute(baseQuery);
  const withBots = {
    ...baseQuery,
    pageSize: 100,
    content: {
      comments: { discussion: true, reviewInline: true, includeBots: true },
      reviews: true,
    },
  };
  const data = await execute(withBots);
  expect(
    data.pullRequests[0].comments.map((comment: { id: string }) => comment.id)
  ).toEqual(['5', '6', '7', '8', '1', '2', '3', '4']);
  expect(mocks.discussion).toHaveBeenCalledTimes(1);
  expect(mocks.inline).toHaveBeenCalledTimes(1);
  expect(mocks.reviews).toHaveBeenCalledTimes(1);
  await execute(withBots, 'two');
  expect(mocks.discussion).toHaveBeenCalledTimes(2);
  expect(mocks.inline).toHaveBeenCalledTimes(2);
  expect(mocks.reviews).toHaveBeenCalledTimes(2);
});
it('reconstructs each comment body when body windows and comment pages are traversed together', async () => {
  let query: Record<string, unknown> | undefined = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    pageSize: 1,
    charLength: 4,
    content: { comments: { discussion: true } },
  };
  const bodies: Record<string, string> = {};
  for (let budget = 0; query && budget < 15; budget++) {
    const data = await execute(query);
    for (const comment of data.pullRequests[0].comments) {
      bodies[comment.id] = (bodies[comment.id] ?? '') + comment.body;
    }
    query =
      data.next?.continueCommentBody?.query ??
      data.next?.nextCommentsPage?.query;
  }
  expect(query).toBeUndefined();
  expect(bodies).toEqual({
    '1': 'message 1',
    '3': 'message 3',
    '4': 'message 4',
  });
  expect(mocks.discussion).toHaveBeenCalledTimes(1);
});
it('retries a failed collection instead of caching its empty fallback', async () => {
  mocks.discussion.mockRejectedValueOnce(new Error('temporary'));
  const client = await getOctokit();
  await expect(fetchPRComments(client, 'o', 'r', 91)).rejects.toThrow(
    'temporary'
  );
  expect(
    (await fetchPRComments(client, 'o', 'r', 91)).comments.map(c => c.id)
  ).toEqual(['1', '3', '4']);
  expect(mocks.discussion).toHaveBeenCalledTimes(2);
});
it('reconstructs all review bodies through executable public continuations', async () => {
  mocks.reviews.mockResolvedValue({
    data: [
      {
        id: 9,
        body: 'first review body',
        state: 'APPROVED',
        user: { login: 'human' },
      },
      {
        id: 10,
        body: 'second review has more content',
        state: 'COMMENTED',
        user: { login: 'human' },
      },
    ],
    headers: {},
  });
  let query: Record<string, unknown> | undefined = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    content: { reviews: true },
    charLength: 4,
  };
  const bodies: Record<string, string> = {};
  for (let budget = 0; query && budget < 10; budget++) {
    const data = await execute(query);
    for (const review of data.pullRequests[0].reviews) {
      bodies[review.id] = (bodies[review.id] ?? '') + (review.body ?? '');
    }
    query = data.next?.continueReviewBody?.query;
  }
  expect(query).toBeUndefined();
  expect(bodies).toEqual({
    '9': 'first review body',
    '10': 'second review has more content',
  });
  expect(mocks.reviews).toHaveBeenCalledTimes(1);
});
it.each(['discussion', 'inline', 'reviews', 'files', 'commits'] as const)(
  'does not cache a failed %s collection as a successful empty PR',
  async surface => {
    mocks.files.mockResolvedValue({
      data: [{ filename: 'a.ts', status: 'modified' }],
      headers: {},
    });
    mocks.commits.mockResolvedValue({
      data: [{ sha: 'a', commit: { message: 'a', author: null } }],
      headers: {},
    });
    mocks[surface].mockRejectedValueOnce(
      Object.assign(new Error('rate limited'), { status: 429 })
    );
    const query = {
      operation: 'pullRequest',
      owner: 'o',
      repo: 'r',
      number: 91,
      content:
        surface === 'reviews'
          ? { reviews: true }
          : surface === 'files'
            ? { changedFiles: true }
            : surface === 'commits'
              ? { commits: {} }
              : {
                  comments:
                    surface === 'inline'
                      ? { reviewInline: true }
                      : { discussion: true },
                },
    };
    const failed = await execute(query);
    expect(failed.error).toBeDefined();
    const recovered = await execute(query);
    expect(recovered.error).toBeUndefined();
    expect(mocks[surface]).toHaveBeenCalledTimes(2);
  }
);
it('reconstructs all PR patches across file and text pages', async () => {
  const files = [
    { filename: 'a.ts', patch: '0123456789', status: 'modified' },
    { filename: 'b.ts', patch: 'abcdefghij', status: 'modified' },
  ];
  mocks.files.mockResolvedValue({ data: files, headers: {} });
  let query: Record<string, unknown> | undefined = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    content: { patches: { mode: 'all' } },
    pageSize: 1,
    charLength: 4,
  };
  const patches: Record<string, string> = {};
  for (let budget = 0; query && budget < 10; budget++) {
    const data = await execute(query);
    for (const file of data.pullRequests[0].changedFiles) {
      patches[file.path] = (patches[file.path] ?? '') + (file.patch ?? '');
    }
    query =
      data.next?.continuePatch?.query ?? data.next?.nextChangedFilesPage?.query;
  }
  expect(query).toBeUndefined();
  expect(patches).toEqual({ 'a.ts': '0123456789', 'b.ts': 'abcdefghij' });
  expect(mocks.files).toHaveBeenCalledTimes(1);
});
it('reports omitted PR patches as terminal while preserving changed-file pagination', async () => {
  mocks.files.mockResolvedValue({
    data: [
      { filename: 'omitted.dat', status: 'modified' },
      { filename: 'present.ts', status: 'modified', patch: 'text' },
    ],
    headers: {},
  });
  const query = {
    operation: 'pullRequest',
    owner: 'o',
    repo: 'r',
    number: 91,
    content: { patches: { mode: 'all' } },
    pageSize: 1,
  };
  const result = await getMultipleGitHubHistoryItems({
    queries: [GitHubGetHistoryItemQueryLocalSchema.parse(query)],
  });
  const row = (
    result.structuredContent as { results: Array<{ meta: any; data: any }> }
  ).results[0]!;
  expect(row.data.pullRequests[0].changedFiles[0]).toMatchObject({
    patchUnavailable: { reason: 'providerOmittedPatch' },
    isPartial: true,
    terminalLimit: true,
  });
  expect(row.meta.diagnostics).toMatchObject({
    partial: true,
    codes: expect.arrayContaining(['terminalLimitReached']),
  });
  const next = row.data.next.nextChangedFilesPage.query;
  const data = await execute(next);
  expect(data.pullRequests[0].changedFiles[0]).toMatchObject({
    path: 'present.ts',
    patch: 'text',
  });
});
