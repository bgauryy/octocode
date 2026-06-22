import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { RequestError } from 'octokit';

const mockGetOctokit = vi.hoisted(() => vi.fn());
const mockBuildPullRequestSearchQuery = vi.hoisted(() => vi.fn());
const mockShouldUseSearchForPRs = vi.hoisted(() => vi.fn());
const mockGenerateCacheKey = vi.hoisted(() => vi.fn());
const mockWithDataCache = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
  OctokitWithThrottling: class MockOctokit {},
}));

vi.mock('../../../octocode-tools-core/src/github/queryBuilders.js', () => ({
  buildPullRequestSearchQuery: mockBuildPullRequestSearchQuery,
  shouldUseSearchForPRs: mockShouldUseSearchForPRs,
}));

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withDataCache: mockWithDataCache,
}));

import { searchGitHubPullRequestsAPI } from '../../../octocode-tools-core/src/github/pullRequestSearch.js';

function makeSearch422(
  errorEntries: Array<Record<string, unknown>>
): RequestError {
  return new RequestError('Validation Failed', 422, {
    response: {
      status: 422,
      headers: {},
      data: { message: 'Validation Failed', errors: errorEntries },
      url: 'https://api.github.com/search/issues',
      retryCount: 0,
    },
    request: {
      method: 'GET',
      url: 'https://api.github.com/search/issues',
      headers: {},
    },
  });
}

describe('PR search — nonexistent entity degrades to empty, not error', () => {
  let mockOctokit: {
    rest: { search: { issuesAndPullRequests: ReturnType<typeof vi.fn> } };
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockOctokit = {
      rest: { search: { issuesAndPullRequests: vi.fn() } },
    };
    mockGetOctokit.mockResolvedValue(mockOctokit);
    mockGenerateCacheKey.mockReturnValue('test-cache-key');
    mockWithDataCache.mockImplementation(
      async (_k: string, op: () => Promise<unknown>) => op()
    );
    mockShouldUseSearchForPRs.mockReturnValue(true);
    mockBuildPullRequestSearchQuery.mockReturnValue(
      'repo:facebook/react is:pr state:open author:zzzz_nonexistent_user_xyz999'
    );
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it('returns a clean EMPTY result (no error) when the author does not exist', async () => {
    mockOctokit.rest.search.issuesAndPullRequests.mockRejectedValue(
      makeSearch422([
        {
          message:
            'The listed users cannot be searched either because the users do not exist or you do not have permission to view the users.',
          resource: 'Search',
          field: 'q',
        },
      ])
    );

    const result = await searchGitHubPullRequestsAPI({
      owner: 'facebook',
      repo: 'react',
      author: 'zzzz_nonexistent_user_xyz999',
      state: 'open',
    });

    expect(result.error).toBeUndefined();
    expect(result.pull_requests).toEqual([]);
    expect(result.total_count).toBe(0);
    expect(result.pagination?.hasMore).toBe(false);
  });

  it('STILL returns an error for a genuinely malformed-query 422 (propagation intact)', async () => {
    mockOctokit.rest.search.issuesAndPullRequests.mockRejectedValue(
      makeSearch422([
        {
          message:
            'The search contains only logical operators (AND, OR, NOT) without any search terms.',
          resource: 'Search',
          field: 'q',
          code: 'invalid',
        },
      ])
    );

    const result = await searchGitHubPullRequestsAPI({
      owner: 'facebook',
      repo: 'react',
      query: 'AND OR NOT',
    });

    expect(result.error).toBeDefined();
    expect(result.pull_requests).toEqual([]);
  });
});
