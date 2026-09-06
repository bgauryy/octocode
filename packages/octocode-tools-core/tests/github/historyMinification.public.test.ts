import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getMINIFY_CONFIG } from '@octocodeai/octocode-engine';

const mocks = vi.hoisted(() => ({
  issueGet: vi.fn(),
  get: vi.fn(),
  discussion: vi.fn(),
  inline: vi.fn(),
  reviews: vi.fn(),
  files: vi.fn(),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      issues: { get: mocks.issueGet, listComments: mocks.discussion },
      pulls: {
        get: mocks.get,
        listReviewComments: mocks.inline,
        listReviews: mocks.reviews,
        listFiles: mocks.files,
      },
    },
  }),
  resolveCacheAuthFingerprint: async () => 'minification-fixture',
  OctokitWithThrottling: class {},
}));
vi.mock('../../src/providers/factory.js', () => ({
  getProvider: () => ({
    type: 'github',
    capabilities: {},
    searchPullRequests: async (query: never) =>
      (
        await import('../../src/providers/github/githubPullRequests.js')
      ).searchPullRequests(query),
  }),
}));
import { clearAllCache } from '../../src/utils/http/cache/management.js';
import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../src/tools/github_search_pull_requests/historySchemes.js';

const body =
  '\n# Exact body\n\n\n<!-- evidence anchor -->\n  Preserve these spaces.  \n';
const patch = [
  '@@ -1,41 +1,41 @@',
  ...Array.from({ length: 20 }, (_, i) => ` before${i}`),
  '-before',
  '+after',
  ...Array.from({ length: 20 }, (_, i) => ` after${i}`),
  '',
].join('\n');
const base = { operation: 'pullRequest', owner: 'o', repo: 'r', number: 91 };
async function execute(query: Record<string, unknown>) {
  const parsed = GitHubGetHistoryItemQueryLocalSchema.parse(query);
  const result = await getMultipleGitHubHistoryItems({ queries: [parsed] });
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}
beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
  mocks.get.mockResolvedValue({
    data: {
      number: 91,
      title: 'Fixture',
      body,
      html_url: '',
      user: { login: 'human' },
    },
  });
  mocks.discussion.mockResolvedValue({
    data: [
      { id: 1, body, user: { login: 'human' } },
      { id: 2, body: body + 'tail', user: { login: 'human' } },
    ],
    headers: {},
  });
  mocks.inline.mockResolvedValue({
    data: [{ id: 3, body, path: 'a.ts', user: { login: 'human' } }],
    headers: {},
  });
  mocks.reviews.mockResolvedValue({
    data: [{ id: 4, body, user: { login: 'human' }, state: 'APPROVED' }],
    headers: {},
  });
  mocks.files.mockResolvedValue({
    data: [{ filename: 'a.ts', patch, status: 'modified' }],
    headers: {},
  });
});

it('reconstructs issue body and comment bodies across independent text and item pages', async () => {
  mocks.issueGet.mockResolvedValue({
    data: { number: 91, title: 'Issue', body },
  });
  const all = [
    { id: 1, body },
    { id: 2, body: body + 'second' },
    { id: 3, body: 'last' },
  ];
  mocks.discussion.mockImplementation(async ({ page, per_page }) => ({
    data: all
      .slice((page - 1) * per_page, page * per_page)
      .map(row => ({ ...row, user: { login: 'human' } })),
    headers:
      page * per_page < all.length
        ? { link: '<https://api.github.com/comments?page=2>; rel="next"' }
        : {},
  }));
  let query: Record<string, unknown> | undefined = {
    ...base,
    operation: 'issue',
    content: { body: true },
    charLength: 7,
  };
  let issueBody = '';
  for (let budget = 0; query && budget < 20; budget++) {
    const data = await execute(query);
    issueBody += data.issues[0].body;
    query = data.next?.continueBody?.query;
  }
  expect(query).toBeUndefined();
  expect(issueBody).toBe(body);
  query = {
    ...base,
    operation: 'issue',
    content: { comments: { discussion: true } },
    charLength: 7,
    pageSize: 2,
  };
  const reconstructed: Record<string, string> = {};
  for (let budget = 0; query && budget < 40; budget++) {
    const data = await execute(query);
    expect(data.issues[0].body).toBeUndefined();
    for (const comment of data.issues[0].comments) {
      expect(comment.body.length).toBeLessThanOrEqual(7);
      reconstructed[comment.id] =
        (reconstructed[comment.id] ?? '') + comment.body;
    }
    query =
      data.next?.continueCommentBody?.query ??
      data.next?.nextCommentsPage?.query;
  }
  expect(query).toBeUndefined();
  expect(reconstructed).toEqual(
    Object.fromEntries(all.map(row => [row.id, row.body]))
  );
  expect(mocks.issueGet).toHaveBeenCalledTimes(1);
  expect(mocks.discussion).toHaveBeenCalledTimes(2);
});

it.each(['body', 'comments'] as const)(
  'automatically windows large issue %s and executes every continuation',
  async surface => {
    const source = 'Evidence stays exact. '.repeat(1300);
    mocks.issueGet.mockResolvedValue({
      data: { number: 91, title: 'Issue', body: source },
    });
    mocks.discussion.mockResolvedValue({
      data: [{ id: 1, body: source, user: { login: 'human' } }],
      headers: {},
    });
    let query: Record<string, unknown> | undefined = {
      ...base,
      operation: 'issue',
      content:
        surface === 'body'
          ? { body: true }
          : { comments: { discussion: true } },
    };
    let reconstructed = '';
    let pages = 0;
    for (; query && pages < 5; pages++) {
      const data = await execute(query);
      const returned =
        surface === 'body'
          ? data.issues[0].body
          : data.issues[0].comments[0].body;
      expect(returned.length).toBeLessThanOrEqual(12_000);
      reconstructed += returned;
      query =
        surface === 'body'
          ? data.next?.continueBody?.query
          : data.next?.continueCommentBody?.query;
    }
    expect(query).toBeUndefined();
    expect(pages).toBe(3);
    expect(reconstructed).toBe(source);
  }
);

describe('public PR history minification precedes lossless view pagination', () => {
  it.each([
    ...new Set([...Object.keys(getMINIFY_CONFIG().fileTypes), 'sc', 'sbt']),
  ])(
    'treats .%s patches as diffs rather than language-minifying changed source',
    async extension => {
      const changedSource = patch.replace(
        '+after',
        '+// keep added comment\n+\n+/* keep block */\n+after'
      );
      mocks.files.mockResolvedValue({
        data: [
          {
            filename: 'fixture.' + extension,
            patch: changedSource,
            status: 'modified',
          },
        ],
        headers: {},
      });
      const content = { patches: { mode: 'all' } };
      const exact = await execute({ ...base, minify: 'none', content });
      expect(exact.pullRequests[0].changedFiles[0].patch).toBe(changedSource);
      const compact = await execute({ ...base, minify: 'standard', content });
      const returned = compact.pullRequests[0].changedFiles[0].patch;
      expect(returned).toContain(
        '+// keep added comment\n+\n+/* keep block */\n+after'
      );
      expect(returned).toContain('-before');
    }
  );
  it.each(['body', 'comments', 'reviews', 'patches'] as const)(
    'reconstructs exact %s with minify:none',
    async surface => {
      const content =
        surface === 'comments'
          ? { comments: { discussion: true, reviewInline: true } }
          : surface === 'patches'
            ? { patches: { mode: 'all' } }
            : { [surface]: true };
      let query: Record<string, unknown> | undefined = {
        ...base,
        content,
        minify: 'none',
        charLength: 17,
        pageSize: 1,
      };
      const reconstructed: Record<string, string> = {};
      for (let budget = 0; query && budget < 100; budget++) {
        const data = await execute(query);
        const pr = data.pullRequests[0];
        if (surface === 'body')
          reconstructed.body = (reconstructed.body ?? '') + pr.body;
        else
          for (const item of pr[
            surface === 'patches' ? 'changedFiles' : surface
          ]) {
            const key = item.id ?? item.path;
            reconstructed[key] =
              (reconstructed[key] ?? '') +
              (surface === 'patches' ? item.patch : item.body);
          }
        query =
          surface === 'body'
            ? data.next?.continueBody?.query
            : surface === 'reviews'
              ? data.next?.continueReviewBody?.query
              : surface === 'comments'
                ? (data.next?.continueCommentBody?.query ??
                  data.next?.nextCommentsPage?.query)
                : (data.next?.continuePatch?.query ??
                  data.next?.nextChangedFilesPage?.query);
        if (query) expect(query.minify).toBe('none');
      }
      expect(query).toBeUndefined();
      expect(reconstructed).toEqual(
        surface === 'body'
          ? { body }
          : surface === 'reviews'
            ? { '4': body }
            : surface === 'comments'
              ? { '1': body, '2': body + 'tail', '3': body }
              : { 'a.ts': patch }
      );
    }
  );

  it('preserves exact selected patches without line-range filtering', async () => {
    const data = await execute({
      ...base,
      minify: 'none',
      content: { patches: { mode: 'selected', files: ['a.ts'] } },
    });
    expect(data.pullRequests[0].changedFiles[0].patch).toBe(patch);
  });

  it('matches source comments before standard transformations can remove their anchors', async () => {
    const data = await execute({
      ...base,
      matchString: 'evidence anchor',
      content: {
        comments: { discussion: true, reviewInline: true },
        reviews: true,
      },
    });
    expect(data.pullRequests[0].comments).toHaveLength(3);
    expect(data.pullRequests[0].comments[0].body).toBe(body);
    expect(data.pullRequests[0].reviews[0].body).toBe(body);
  });

  it.each(['comments', 'reviews', 'patches'] as const)(
    'reconstructs the standard %s view with executable continuations',
    async surface => {
      const content =
        surface === 'comments'
          ? { comments: { discussion: true, reviewInline: true } }
          : surface === 'patches'
            ? { patches: { mode: 'all' } }
            : { reviews: true };
      const key = surface === 'patches' ? 'changedFiles' : surface;
      const full = (await execute({ ...base, content, minify: 'standard' }))
        .pullRequests[0][key];
      const expected = Object.fromEntries(
        full.map((item: any) => [
          item.id ?? item.path,
          surface === 'patches' ? item.patch : item.body,
        ])
      );
      const reconstructed: Record<string, string> = {};
      let query: Record<string, unknown> | undefined = {
        ...base,
        content,
        minify: 'standard',
        charLength: 7,
        pageSize: 1,
      };
      for (let budget = 0; query && budget < 100; budget++) {
        const data = await execute(query);
        for (const item of data.pullRequests[0][key]) {
          const id = item.id ?? item.path;
          reconstructed[id] =
            (reconstructed[id] ?? '') +
            (surface === 'patches' ? item.patch : item.body);
        }
        query =
          surface === 'reviews'
            ? data.next?.continueReviewBody?.query
            : surface === 'comments'
              ? (data.next?.continueCommentBody?.query ??
                data.next?.nextCommentsPage?.query)
              : (data.next?.continuePatch?.query ??
                data.next?.nextChangedFilesPage?.query);
        if (query) expect(query.minify).toBe('standard');
      }
      expect(query).toBeUndefined();
      expect(reconstructed).toEqual(expected);
    }
  );

  it('compacts standard views once and reconstructs the same view across windows', async () => {
    const content = {
      body: true,
      comments: { discussion: true, reviewInline: true },
      reviews: true,
      patches: { mode: 'all' },
    };
    const exact = (await execute({ ...base, minify: 'none', content }))
      .pullRequests[0];
    const standard = (await execute({ ...base, minify: 'standard', content }))
      .pullRequests[0];
    expect(standard.body.length).toBeLessThan(exact.body.length);
    expect(standard.comments[0].body.length).toBeLessThan(
      exact.comments[0].body.length
    );
    expect(standard.reviews[0].body.length).toBeLessThan(
      exact.reviews[0].body.length
    );
    expect(standard.changedFiles[0].patch.length).toBeLessThan(patch.length);
    let query: Record<string, unknown> | undefined = {
      ...base,
      minify: 'standard',
      content: { body: true },
      charLength: 7,
    };
    let reconstructed = '';
    for (let budget = 0; query && budget < 30; budget++) {
      const data = await execute(query);
      reconstructed += data.pullRequests[0].body;
      query = data.next?.continueBody?.query;
    }
    expect(query).toBeUndefined();
    expect(reconstructed).toBe(standard.body);
  });
});
