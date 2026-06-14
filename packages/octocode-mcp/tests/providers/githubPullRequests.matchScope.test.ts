import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockSearchAPI = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/github/pullRequestSearch.js', () => ({
  searchGitHubPullRequestsAPI: mockSearchAPI,
}));

import { searchPullRequests } from '../../../octocode-tools-core/src/providers/github/githubPullRequests.js';

type PRQuery = Parameters<typeof searchPullRequests>[0];

function emptyApiResult() {
  return {
    pull_requests: [],
    total_count: 0,
    pagination: {
      currentPage: 1,
      totalPages: 1,
      perPage: 30,
      totalMatches: 0,
      hasMore: false,
    },
  };
}

describe('searchPullRequests — match field forwarded to GitHub API `match`', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSearchAPI.mockResolvedValue(emptyApiResult());
  });

  it('forwards a single-value match as `match`', async () => {
    await searchPullRequests({
      query: 'Suspense',
      match: ['title'],
    } as PRQuery);

    expect(mockSearchAPI).toHaveBeenCalledTimes(1);
    const forwarded = mockSearchAPI.mock.calls[0]![0];
    expect(forwarded.match).toEqual(['title']);
    expect(forwarded.query).toBe('Suspense');
  });

  it('forwards a multi-value match as `match`', async () => {
    await searchPullRequests({
      query: 'x',
      match: ['title', 'body'],
    } as PRQuery);

    expect(mockSearchAPI.mock.calls[0]![0].match).toEqual(['title', 'body']);
  });

  it('forwards undefined match when match is absent', async () => {
    await searchPullRequests({ query: 'x' } as PRQuery);

    expect(mockSearchAPI.mock.calls[0]![0].match).toBeUndefined();
  });
});
