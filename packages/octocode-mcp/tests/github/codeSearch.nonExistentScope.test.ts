import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { RequestError } from 'octokit';

const mockGetOctokit = vi.hoisted(() => vi.fn());
const mockBuildCodeSearchQuery = vi.hoisted(() => vi.fn());
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
  buildCodeSearchQuery: mockBuildCodeSearchQuery,
}));

vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withDataCache: mockWithDataCache,
}));

import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';

function makeSearch422(
  errorEntries: Array<Record<string, unknown>>
): RequestError {
  return new RequestError('Validation Failed', 422, {
    response: {
      status: 422,
      headers: {},
      data: { message: 'Validation Failed', errors: errorEntries },
      url: 'https://api.github.com/search/code',
      retryCount: 0,
    },
    request: {
      method: 'GET',
      url: 'https://api.github.com/search/code',
      headers: {},
    },
  });
}

describe('code search — nonexistent scope degrades to empty + flag', () => {
  let mockOctokit: {
    rest: { search: { code: ReturnType<typeof vi.fn> } };
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockOctokit = { rest: { search: { code: vi.fn() } } };
    mockGetOctokit.mockResolvedValue(mockOctokit);
    mockGenerateCacheKey.mockReturnValue('test-cache-key');
    mockWithDataCache.mockImplementation(
      async (_k: string, op: () => Promise<unknown>) => op()
    );
    mockBuildCodeSearchQuery.mockReturnValue(
      'useSyncExternalStore repo:nope/does-not-exist'
    );
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it('flags nonExistentScope when the repo/owner does not exist (422)', async () => {
    mockOctokit.rest.search.code.mockRejectedValue(
      makeSearch422([
        {
          message:
            'The listed repositories cannot be searched either because the repositories do not exist or you do not have permission to view them.',
          resource: 'Search',
          field: 'q',
        },
      ])
    );

    const result = await searchGitHubCodeAPI({
      keywordsToSearch: ['useSyncExternalStore'],
      owner: 'nope',
      repo: 'does-not-exist',
    });

    expect('error' in result).toBe(false);
    if ('data' in result) {
      expect(result.data.total_count).toBe(0);
      expect(result.data.items).toEqual([]);
      expect(result.data.nonExistentScope).toBe(true);
    }
  });

  it('does NOT flag nonExistentScope on a genuine zero-result search', async () => {
    mockOctokit.rest.search.code.mockResolvedValue({
      data: { total_count: 0, items: [] },
      headers: {},
    });

    const result = await searchGitHubCodeAPI({
      keywordsToSearch: ['zzz_no_such_symbol_xyz'],
      owner: 'facebook',
      repo: 'react',
    });

    expect('error' in result).toBe(false);
    if ('data' in result) {
      expect(result.data.total_count).toBe(0);
      expect(result.data.nonExistentScope).toBeUndefined();
    }
  });
});
