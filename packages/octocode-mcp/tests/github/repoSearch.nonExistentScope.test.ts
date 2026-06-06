import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { RequestError } from 'octokit';

const mockGetOctokit = vi.hoisted(() => vi.fn());
const mockBuildRepoSearchQuery = vi.hoisted(() => vi.fn());
const mockGenerateCacheKey = vi.hoisted(() => vi.fn());
const mockWithDataCache = vi.hoisted(() => vi.fn());
const mockLogSessionError = vi.hoisted(() => vi.fn());
const mockLogRateLimit = vi.hoisted(() => vi.fn());

vi.mock('../../src/session.js', () => ({
  logSessionError: mockLogSessionError,
  logRateLimit: mockLogRateLimit,
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
  OctokitWithThrottling: class MockOctokit {},
}));

vi.mock('../../src/github/queryBuilders.js', () => ({
  buildRepoSearchQuery: mockBuildRepoSearchQuery,
}));

vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withDataCache: mockWithDataCache,
}));

import { searchGitHubReposAPI } from '../../src/github/repoSearch.js';

function makeSearch422(
  errorEntries: Array<Record<string, unknown>>
): RequestError {
  return new RequestError('Validation Failed', 422, {
    response: {
      status: 422,
      headers: {},
      data: { message: 'Validation Failed', errors: errorEntries },
      url: 'https://api.github.com/search/repositories',
      retryCount: 0,
    },
    request: {
      method: 'GET',
      url: 'https://api.github.com/search/repositories',
      headers: {},
    },
  });
}

describe('repo search — nonexistent owner degrades to empty + flag', () => {
  let mockOctokit: {
    rest: { search: { repos: ReturnType<typeof vi.fn> } };
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockOctokit = { rest: { search: { repos: vi.fn() } } };
    mockGetOctokit.mockResolvedValue(mockOctokit);
    mockGenerateCacheKey.mockReturnValue('test-cache-key');
    mockWithDataCache.mockImplementation(
      async (_k: string, op: () => Promise<unknown>) => op()
    );
    mockBuildRepoSearchQuery.mockReturnValue('user:zzz_nonexistent_org_xyz999');
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it('flags nonExistentScope when the owner/org does not exist (422)', async () => {
    mockOctokit.rest.search.repos.mockRejectedValue(
      makeSearch422([
        {
          message:
            'The listed users cannot be searched either because the users do not exist or you do not have permission to view the users.',
          resource: 'Search',
          field: 'q',
        },
      ])
    );

    const result = await searchGitHubReposAPI({
      owner: 'zzz_nonexistent_org_xyz999',
      keywordsToSearch: ['anything'],
    });

    expect('error' in result).toBe(false);
    if ('data' in result) {
      expect(result.data.repositories).toEqual([]);
      expect(result.data.nonExistentScope).toBe(true);
    }
  });

  it('does NOT flag nonExistentScope on a genuine zero-result search', async () => {
    mockOctokit.rest.search.repos.mockResolvedValue({
      data: { total_count: 0, items: [] },
      headers: {},
    });

    const result = await searchGitHubReposAPI({
      keywordsToSearch: ['zzz_no_such_repo_xyz'],
    });

    expect('error' in result).toBe(false);
    if ('data' in result) {
      expect(result.data.repositories).toEqual([]);
      expect(result.data.nonExistentScope).toBeUndefined();
    }
  });
});
