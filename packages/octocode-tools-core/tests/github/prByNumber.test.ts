import { describe, expect, it, vi } from 'vitest';

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
  resolveCacheAuthFingerprint: vi.fn(async () => 'anon'),
}));

import { getOctokit } from '../../src/github/client.js';
import { fetchGitHubPullRequestByNumberAPIInternal } from '../../src/github/prByNumber.js';
import { SEARCH_ERRORS } from '../../src/errors/domainErrors.js';

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
  } as never;
}

describe('fetchGitHubPullRequestByNumberAPIInternal', () => {
  it('returns an error when owner is missing', async () => {
    const result = await fetchGitHubPullRequestByNumberAPIInternal({
      owner: undefined as never,
      repo: 'react',
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_REQUIRED_PARAMS.message);
    expect(result.pullRequests).toHaveLength(0);
  });

  it('returns an error when repo is missing', async () => {
    const result = await fetchGitHubPullRequestByNumberAPIInternal({
      owner: 'facebook',
      repo: undefined as never,
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_REQUIRED_PARAMS.message);
  });

  it('returns an error when prNumber is missing', async () => {
    const result = await fetchGitHubPullRequestByNumberAPIInternal({
      owner: 'facebook',
      repo: 'react',
      prNumber: undefined as never,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_REQUIRED_PARAMS.message);
  });

  it('returns an error when owner is an array', async () => {
    const result = await fetchGitHubPullRequestByNumberAPIInternal({
      owner: ['facebook', 'meta'] as never,
      repo: 'react',
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_SINGLE_VALUES.message);
    expect(result.hints).toBeDefined();
  });

  it('returns an error when repo is an array', async () => {
    const result = await fetchGitHubPullRequestByNumberAPIInternal({
      owner: 'facebook',
      repo: ['react', 'react-dom'] as never,
      prNumber: 42,
    });
    expect(result.error).toBe(SEARCH_ERRORS.PR_SINGLE_VALUES.message);
  });

  it('fetches a PR by number and returns it in the result', async () => {
    mockGetOctokit.mockResolvedValue(makeOctokit());

    const result = await fetchGitHubPullRequestByNumberAPIInternal({
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

    const result = await fetchGitHubPullRequestByNumberAPIInternal({
      owner: 'facebook',
      repo: 'react',
      prNumber: 99999,
    });
    expect(result.error).toBeDefined();
    expect(result.pullRequests).toHaveLength(0);
  });
});
