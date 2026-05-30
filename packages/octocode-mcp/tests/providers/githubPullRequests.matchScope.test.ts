import { describe, it, expect, beforeEach, vi } from 'vitest';

// Pin the provider-boundary mapping `matchScope -> match`. The query builder
// already proves `match -> in:title|body|comments`; this closes the chain by
// proving the provider forwards the public `matchScope` field as `match` to
// the search API (the seam that was silently dropping it before the fix).
const mockSearchAPI = vi.hoisted(() => vi.fn());

vi.mock('../../src/github/pullRequestSearch.js', () => ({
  searchGitHubPullRequestsAPI: mockSearchAPI,
}));

import { searchPullRequests } from '../../src/providers/github/githubPullRequests.js';

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

describe('searchPullRequests — matchScope mapping to API `match`', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSearchAPI.mockResolvedValue(emptyApiResult());
  });

  it('forwards a single-value matchScope as `match`', async () => {
    await searchPullRequests({
      query: 'Suspense',
      matchScope: ['title'],
    } as PRQuery);

    expect(mockSearchAPI).toHaveBeenCalledTimes(1);
    const forwarded = mockSearchAPI.mock.calls[0][0];
    expect(forwarded.match).toEqual(['title']);
    expect(forwarded.query).toBe('Suspense');
  });

  it('forwards a multi-value matchScope as `match`', async () => {
    await searchPullRequests({
      query: 'x',
      matchScope: ['title', 'body'],
    } as PRQuery);

    expect(mockSearchAPI.mock.calls[0][0].match).toEqual(['title', 'body']);
  });

  it('forwards undefined `match` when matchScope is absent', async () => {
    await searchPullRequests({ query: 'x' } as PRQuery);

    expect(mockSearchAPI.mock.calls[0][0].match).toBeUndefined();
  });
});
