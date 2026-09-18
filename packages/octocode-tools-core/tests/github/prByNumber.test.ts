import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';
import type { AuthInfo } from '@modelcontextprotocol/server';

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
  resolveCacheAuthFingerprint: vi.fn(
    async (auth?: AuthInfo) => auth?.token ?? 'anon'
  ),
}));

vi.mock('../../src/utils/http/cache/diskStore.js', () => ({
  readDiskCache: vi.fn(async () => undefined),
  writeDiskCache: vi.fn(async () => {}),
}));

import { getOctokit } from '../../src/github/client.js';
import { fetchGitHubPullRequestByNumberAPI } from '../../src/github/prByNumber.js';
import { SEARCH_ERRORS } from '../../src/errors/domainErrors.js';
import { searchGitHubPullRequestsAPI } from '../../src/github/pullRequestSearch.js';
import { cache, pendingRequests } from '../../src/utils/http/cache/store.js';
import { etagSoftCache } from '../../src/utils/http/cache/conditional.js';

const mockGetOctokit = vi.mocked(getOctokit);

function makePR(number = 42) {
  return {
    number,
    title: `PR ${number}`,
    state: 'open',
    draft: false,
    body: 'body text',
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-02T00:00:00Z',
    closed_at: null,
    merged_at: null,
    merge_commit_sha: null,
    html_url: `https://github.com/facebook/react/pull/${number}`,
    user: { login: 'author', avatar_url: '', type: 'User', html_url: '' },
    head: {
      ref: 'feat/branch',
      sha: 'abc',
      label: 'author:feat/branch',
      repo: null,
      user: null,
    },
    base: {
      ref: 'main',
      sha: 'def',
      label: 'facebook:main',
      repo: null,
      user: null,
    },
    labels: [],
    assignees: [],
    requested_reviewers: [],
    requested_teams: [],
    milestone: null,
    url: '',
    diff_url: '',
    patch_url: '',
    issue_url: '',
    commits_url: '',
    review_comments_url: '',
    comments_url: '',
    statuses_url: '',
    _links: {
      html: { href: '' },
      self: { href: '' },
      commits: { href: '' },
      statuses: { href: '' },
      review_comments: { href: '' },
      review_comment: { href: '' },
      comments: { href: '' },
      issue: { href: '' },
    },
    author_association: 'OWNER',
    auto_merge: null,
    locked: false,
    active_lock_reason: null,
    node_id: '',
    additions: 0,
    deletions: 0,
    changed_files: 0,
    commits: 1,
    review_comments: 0,
    comments: 0,
    maintainer_can_modify: false,
    rebaseable: true,
    mergeable: true,
    merged: false,
    mergeable_state: 'clean',
    merged_by: null,
  };
}

function makeOctokit(prData = makePR()) {
  return {
    rest: {
      pulls: {
        get: vi
          .fn()
          .mockResolvedValue({ data: prData, status: 200, headers: {} }),
        listReviews: vi.fn().mockResolvedValue({ data: [] }),
        listReviewComments: vi.fn().mockResolvedValue({ data: [] }),
        listCommits: vi.fn().mockResolvedValue({ data: [] }),
        listFiles: vi.fn().mockResolvedValue({ data: [] }),
      },
      issues: {
        listComments: vi.fn().mockResolvedValue({ data: [] }),
      },
    },
  };
}

describe('fetchGitHubPullRequestByNumberAPI', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    cache.flushAll();
    etagSoftCache.flushAll();
    pendingRequests.clear();
  });

  afterEach(() => vi.useRealTimers());

  it('returns an error when owner is missing', async () => {
    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: undefined as never,
      repo: 'react',
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_REQUIRED_PARAMS.message);
    expect(result.pullRequests).toHaveLength(0);
  });

  it('returns an error when repo is missing', async () => {
    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: 'facebook',
      repo: undefined as never,
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_REQUIRED_PARAMS.message);
  });

  it('returns an error when prNumber is missing', async () => {
    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: 'facebook',
      repo: 'react',
      prNumber: undefined as never,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_REQUIRED_PARAMS.message);
  });

  it('returns an error when owner is an array', async () => {
    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: ['facebook', 'meta'] as never,
      repo: 'react',
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_SINGLE_VALUES.message);
    expect(result.hints).toBeDefined();
  });

  it('returns an error when repo is an array', async () => {
    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: 'facebook',
      repo: ['react', 'react-dom'] as never,
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_SINGLE_VALUES.message);
  });

  it('fetches a PR by number and returns it in the result', async () => {
    mockGetOctokit.mockResolvedValue(makeOctokit() as never);

    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: 'facebook',
      repo: 'react',
      prNumber: 42,
    });
    expect(result.error).toBeUndefined();
    expect(result.pullRequests).toHaveLength(1);
    expect(result.totalCount).toBe(1);
  });

  it('returns an error result when the API throws', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        pulls: {
          get: vi
            .fn()
            .mockRejectedValue(
              Object.assign(new Error('Not Found'), { status: 404 })
            ),
        },
      },
    } as never);

    const result = await fetchGitHubPullRequestByNumberAPI({
      owner: 'facebook',
      repo: 'react',
      prNumber: 99999,
    });
    expect(result.error).toBeDefined();
    expect(result.pullRequests).toHaveLength(0);
  });

  it('reuses raw metadata across selectors without losing patches or provider pages', async () => {
    const octokit = makeOctokit({ ...makePR(), changed_files: 2 });
    const file = (filename: string) => ({
      filename,
      status: 'modified',
      additions: 1,
      deletions: 0,
      changes: 1,
      patch: `@@ -1 +1 @@\n+${filename}`,
    });
    octokit.rest.pulls.listFiles
      .mockResolvedValueOnce({
        data: [file('a.ts')],
        headers: { link: '<https://api.github.com/files?page=2>; rel="next"' },
      } as never)
      .mockResolvedValueOnce({ data: [file('b.ts')], headers: {} } as never);
    mockGetOctokit.mockResolvedValue(octokit as never);
    const identity = { owner: 'facebook', repo: 'react', prNumber: 42 };

    const body = await searchGitHubPullRequestsAPI(identity);
    const files = await searchGitHubPullRequestsAPI({
      ...identity,
      content: { changedFiles: true },
    });
    const patches = await searchGitHubPullRequestsAPI({
      ...identity,
      content: { patches: { mode: 'all' } },
    });
    const second = await searchGitHubPullRequestsAPI({
      ...identity,
      content: { patches: { mode: 'all' } },
      collectionPages: { changedFiles: 2 },
    });

    expect(body.pullRequests[0]?.body).toBe('body text');
    expect(files.pullRequests[0]?.fileChanges?.[0]?.patch).toBeUndefined();
    expect(patches.pullRequests[0]?.fileChanges?.[0]?.patch).toBe(
      file('a.ts').patch
    );
    expect(
      [
        ...patches.pullRequests[0]!.fileChanges!,
        ...second.pullRequests[0]!.fileChanges!,
      ].map(entry => entry.filename)
    ).toEqual(['a.ts', 'b.ts']);
    expect(
      patches.pullRequests[0]?.collectionStates?.changedFiles?.hasMore
    ).toBe(true);
    expect(
      second.pullRequests[0]?.collectionStates?.changedFiles?.hasMore
    ).toBe(false);
    expect(octokit.rest.pulls.get).toHaveBeenCalledTimes(1);
    expect(octokit.rest.pulls.listFiles).toHaveBeenCalledTimes(2);
  });

  it('isolates raw metadata between authentication identities', async () => {
    const octokit = makeOctokit();
    octokit.rest.pulls.get
      .mockResolvedValueOnce({
        data: { ...makePR(), body: 'private body' },
        headers: {},
        status: 200,
      })
      .mockResolvedValueOnce({
        data: { ...makePR(), body: 'other body' },
        headers: {},
        status: 200,
      });
    mockGetOctokit.mockResolvedValue(octokit as never);
    const query = { owner: 'facebook', repo: 'react', prNumber: 42 };
    const a = { token: 'identity-a' } as AuthInfo;
    const b = { token: 'identity-b' } as AuthInfo;

    expect(
      (await searchGitHubPullRequestsAPI(query, a)).pullRequests[0]?.body
    ).toBe('private body');
    expect(
      (
        await searchGitHubPullRequestsAPI(
          { ...query, content: { reviews: true } },
          b
        )
      ).pullRequests[0]?.body
    ).toBe('other body');
    expect(
      (
        await searchGitHubPullRequestsAPI(
          { ...query, content: { changedFiles: true } },
          a
        )
      ).pullRequests[0]?.body
    ).toBe('private body');
    expect(octokit.rest.pulls.get).toHaveBeenCalledTimes(2);
  });

  it('revalidates metadata at its original expiry rather than extending it per selector', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-13T00:00:00Z'));
    const octokit = makeOctokit();
    octokit.rest.pulls.get
      .mockResolvedValueOnce({
        data: makePR(),
        headers: { ETag: '"pr-v1"' },
        status: 200,
      })
      .mockRejectedValueOnce(
        new RequestError('Not modified', 304, {
          request: {
            method: 'GET',
            url: '/repos/facebook/react/pulls/42',
            headers: {},
          },
        })
      )
      .mockResolvedValueOnce({
        data: { ...makePR(), body: 'updated body' },
        headers: { etag: '"pr-v2"' },
        status: 200,
      });
    mockGetOctokit.mockResolvedValue(octokit as never);
    const query = { owner: 'facebook', repo: 'react', prNumber: 42 };
    const selected = { ...query, content: { reviews: true } };
    await searchGitHubPullRequestsAPI(query);
    vi.advanceTimersByTime(29 * 60 * 1000);
    await searchGitHubPullRequestsAPI(selected);
    expect(octokit.rest.pulls.get).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(2 * 60 * 1000);
    const revalidated = await searchGitHubPullRequestsAPI(selected);
    expect(revalidated.error).toBeUndefined();
    expect(revalidated.pullRequests[0]?.body).toBe('body text');
    expect(octokit.rest.pulls.get).toHaveBeenNthCalledWith(2, {
      owner: 'facebook',
      repo: 'react',
      pull_number: 42,
      headers: { 'if-none-match': '"pr-v1"' },
    });
    vi.advanceTimersByTime(31 * 60 * 1000);
    expect(
      (await searchGitHubPullRequestsAPI(selected)).pullRequests[0]?.body
    ).toBe('updated body');
    expect(octokit.rest.pulls.get).toHaveBeenCalledTimes(3);
  });

  it('preserves rate errors and does not cache failed metadata acquisition', async () => {
    const octokit = makeOctokit();
    octokit.rest.pulls.get
      .mockRejectedValueOnce(
        new RequestError('Rate limited', 429, {
          request: {
            method: 'GET',
            url: '/repos/facebook/react/pulls/42',
            headers: {},
          },
          response: {
            status: 429,
            url: 'https://api.github.com/repos/facebook/react/pulls/42',
            headers: { 'retry-after': '120' },
            data: {},
          },
        })
      )
      .mockResolvedValueOnce({ data: makePR(), headers: {}, status: 200 });
    mockGetOctokit.mockResolvedValue(octokit as never);
    const query = { owner: 'facebook', repo: 'react', prNumber: 42 };

    const failed = await searchGitHubPullRequestsAPI(query);
    expect(failed.status).toBe(429);
    expect(failed.retryAfter).toBe(120);
    expect((await searchGitHubPullRequestsAPI(query)).error).toBeUndefined();
    expect(octokit.rest.pulls.get).toHaveBeenCalledTimes(2);
  });
});
