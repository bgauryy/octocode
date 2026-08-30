import { describe, expect, it, vi } from 'vitest';
import {
  createPullRequestErrorResult,
  createPullRequestEmptyResult,
  searchPullRequestsWithREST,
} from '../../src/github/pullRequestSearch/restSearch.js';
import type { GitHubAPIError } from '../../src/github/githubAPI.js';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../../src/config.js';

// ---------------------------------------------------------------------------
// createPullRequestErrorResult (pure)
// ---------------------------------------------------------------------------

function makeApiError(overrides: Partial<GitHubAPIError> = {}): GitHubAPIError {
  return {
    error: 'Something broke',
    status: 500,
    ...overrides,
  };
}

describe('createPullRequestErrorResult', () => {
  it('returns a result with empty pullRequests and totalCount=0', () => {
    const apiError = makeApiError({ status: 404 });
    const result = createPullRequestErrorResult(apiError, 'not found', [
      'try again',
    ]);
    expect(result.pullRequests).toHaveLength(0);
    expect(result.totalCount).toBe(0);
  });

  it('propagates the error message', () => {
    const apiError = makeApiError({ status: 403 });
    const result = createPullRequestErrorResult(apiError, 'access denied', []);
    expect(result.error).toBe('access denied');
  });

  it('propagates the HTTP status', () => {
    const apiError = makeApiError({ status: 422 });
    const result = createPullRequestErrorResult(apiError, 'bad request', []);
    expect(result.status).toBe(422);
  });

  it('includes hints in the result', () => {
    const apiError = makeApiError();
    const result = createPullRequestErrorResult(apiError, 'err', [
      'hint A',
      'hint B',
    ]);
    expect(result.hints).toEqual(['hint A', 'hint B']);
  });

  it('propagates rate limit fields from the api error', () => {
    const apiError = makeApiError({
      rateLimitRemaining: 0,
      rateLimitReset: 1700000000,
      retryAfter: 60,
    });
    const result = createPullRequestErrorResult(apiError, 'rate limited', []);
    expect(result.rateLimitRemaining).toBe(0);
    expect(result.rateLimitReset).toBe(1700000000);
    expect(result.retryAfter).toBe(60);
  });
});

// ---------------------------------------------------------------------------
// createPullRequestEmptyResult (pure)
// ---------------------------------------------------------------------------

describe('createPullRequestEmptyResult', () => {
  it('returns a result with empty pullRequests', () => {
    const result = createPullRequestEmptyResult({ owner: 'foo', repo: 'bar' });
    expect(result.pullRequests).toHaveLength(0);
    expect(result.totalCount).toBe(0);
  });

  it('uses GITHUB_SEARCH_DEFAULT_LIMIT when no limit provided', () => {
    const result = createPullRequestEmptyResult({ owner: 'foo', repo: 'bar' });
    expect(result.pagination!.perPage).toBe(GITHUB_SEARCH_DEFAULT_LIMIT);
  });

  it('clamps limit to GITHUB_SEARCH_MAX_LIMIT', () => {
    const result = createPullRequestEmptyResult({
      owner: 'foo',
      repo: 'bar',
      limit: GITHUB_SEARCH_MAX_LIMIT + 50,
    });
    expect(result.pagination!.perPage).toBe(GITHUB_SEARCH_MAX_LIMIT);
  });

  it('uses provided limit when within bounds', () => {
    const result = createPullRequestEmptyResult({
      owner: 'foo',
      repo: 'bar',
      limit: 10,
    });
    expect(result.pagination!.perPage).toBe(10);
  });

  it('defaults currentPage to 1 when no page provided', () => {
    const result = createPullRequestEmptyResult({ owner: 'foo', repo: 'bar' });
    expect(result.pagination!.currentPage).toBe(1);
  });

  it('uses provided page', () => {
    const result = createPullRequestEmptyResult({
      owner: 'foo',
      repo: 'bar',
      page: 3,
    });
    expect(result.pagination!.currentPage).toBe(3);
  });

  it('sets totalPages to 0 and hasMore to false', () => {
    const result = createPullRequestEmptyResult({ owner: 'foo', repo: 'bar' });
    expect(result.pagination!.totalPages).toBe(0);
    expect(result.pagination!.hasMore).toBe(false);
  });

  it('sets totalMatches to 0', () => {
    const result = createPullRequestEmptyResult({ owner: 'foo', repo: 'bar' });
    expect(result.pagination!.totalMatches).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// searchPullRequestsWithREST (requires mocked octokit)
// ---------------------------------------------------------------------------

function makePRItem(number = 1) {
  return {
    number,
    title: `PR ${number}`,
    state: 'open' as const,
    draft: false,
    body: 'body',
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
    commits_url: '',
    review_comments_url: '',
    comments_url: '',
    statuses_url: '',
    issue_url: '',
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
    author_association: 'OWNER' as const,
    auto_merge: null,
    locked: false,
    active_lock_reason: null,
    node_id: '',
    diff_url: '',
    patch_url: '',
    number_of_comments: 0,
  };
}

function makeOctokit(overrides: Record<string, unknown> = {}) {
  return {
    rest: {
      pulls: {
        list: vi.fn().mockResolvedValue({
          data: [makePRItem(1)],
          headers: {},
          status: 200,
        }),
        get: vi.fn(),
        listReviews: vi.fn().mockResolvedValue({ data: [] }),
        listReviewComments: vi.fn().mockResolvedValue({ data: [] }),
        listCommits: vi.fn().mockResolvedValue({ data: [] }),
        listFiles: vi.fn().mockResolvedValue({ data: [] }),
      },
    },
    ...overrides,
  } as unknown as Parameters<typeof searchPullRequestsWithREST>[0];
}

describe('searchPullRequestsWithREST', () => {
  it('returns pull request results on success', async () => {
    const octokit = makeOctokit();
    const result = await searchPullRequestsWithREST(octokit, {
      owner: 'facebook',
      repo: 'react',
    });
    expect(result.pullRequests.length).toBeGreaterThan(0);
    expect(result.totalCount).toBeGreaterThan(0);
  });

  it('passes state "merged" as "closed" to the API', async () => {
    const octokit = makeOctokit();
    await searchPullRequestsWithREST(octokit, {
      owner: 'facebook',
      repo: 'react',
      state: 'merged',
    });
    expect(octokit.rest.pulls.list).toHaveBeenCalledWith(
      expect.objectContaining({ state: 'closed' })
    );
  });

  it('passes sort:"updated" to the API', async () => {
    const octokit = makeOctokit();
    await searchPullRequestsWithREST(octokit, {
      owner: 'facebook',
      repo: 'react',
      sort: 'updated',
    });
    expect(octokit.rest.pulls.list).toHaveBeenCalledWith(
      expect.objectContaining({ sort: 'updated' })
    );
  });

  it('passes head/base filters when provided', async () => {
    const octokit = makeOctokit();
    await searchPullRequestsWithREST(octokit, {
      owner: 'facebook',
      repo: 'react',
      head: 'feat/my-branch',
      base: 'main',
    });
    expect(octokit.rest.pulls.list).toHaveBeenCalledWith(
      expect.objectContaining({ head: 'feat/my-branch', base: 'main' })
    );
  });

  it('sets hasMore=true and nextPage when page is full', async () => {
    const perPage = 2;
    const mockOctokit = {
      ...makeOctokit(),
      rest: {
        pulls: {
          list: vi.fn().mockResolvedValue({
            data: [makePRItem(1), makePRItem(2)],
            headers: {},
            status: 200,
          }),
          get: vi.fn(),
          listReviews: vi.fn().mockResolvedValue({ data: [] }),
          listReviewComments: vi.fn().mockResolvedValue({ data: [] }),
          listCommits: vi.fn().mockResolvedValue({ data: [] }),
          listFiles: vi.fn().mockResolvedValue({ data: [] }),
        },
      },
    } as unknown as Parameters<typeof searchPullRequestsWithREST>[0];

    const result = await searchPullRequestsWithREST(mockOctokit, {
      owner: 'facebook',
      repo: 'react',
      limit: perPage,
      page: 1,
    });
    expect(result.pagination!.hasMore).toBe(true);
    expect(result.pagination!.nextPage).toBe(2);
  });

  it('sets hasMore=false when response has fewer items than perPage', async () => {
    const result = await searchPullRequestsWithREST(makeOctokit(), {
      owner: 'facebook',
      repo: 'react',
      limit: 30,
    });
    // Only 1 item returned, perPage=30 → hasMore=false
    expect(result.pagination!.hasMore).toBe(false);
  });

  it('returns empty result on no-results search error', async () => {
    const mockOctokit = {
      rest: {
        pulls: {
          list: vi
            .fn()
            .mockRejectedValue(
              Object.assign(new Error('No results'), { status: 422 })
            ),
          get: vi.fn(),
          listReviews: vi.fn(),
          listReviewComments: vi.fn(),
          listCommits: vi.fn(),
          listFiles: vi.fn(),
        },
      },
    } as unknown as Parameters<typeof searchPullRequestsWithREST>[0];

    const result = await searchPullRequestsWithREST(mockOctokit, {
      owner: 'facebook',
      repo: 'react',
    });
    // should return empty not error
    expect(result.pullRequests).toHaveLength(0);
  });

  it('returns error result on unexpected API error', async () => {
    const mockOctokit = {
      rest: {
        pulls: {
          list: vi
            .fn()
            .mockRejectedValue(
              Object.assign(new Error('Internal server error'), { status: 500 })
            ),
          get: vi.fn(),
          listReviews: vi.fn(),
          listReviewComments: vi.fn(),
          listCommits: vi.fn(),
          listFiles: vi.fn(),
        },
      },
    } as unknown as Parameters<typeof searchPullRequestsWithREST>[0];

    const result = await searchPullRequestsWithREST(mockOctokit, {
      owner: 'facebook',
      repo: 'react',
    });
    expect(result.error).toBeDefined();
    expect(result.pullRequests).toHaveLength(0);
  });
});
