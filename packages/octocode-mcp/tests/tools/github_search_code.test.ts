import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockOctokit = vi.hoisted(() => ({
  rest: {
    search: {
      code: vi.fn(),
    },
  },
}));

vi.mock('../../../octocode-tools-core/src/github/client.js', () => ({
  getOctokit: vi.fn(() => mockOctokit),
}));

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn(async (_key: string, fn: () => unknown) => {
    return await fn();
  }),
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
}));

import { searchGitHubCodeAPI } from '../../../octocode-tools-core/src/github/codeSearch.js';

describe('Quality Boosting and Research Goals', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should search code without quality boost filters', async () => {
    const mockResponse = {
      data: {
        total_count: 1,
        items: [
          {
            name: 'test.js',
            path: 'src/test.js',
            sha: 'abc123',
            url: 'https://api.github.com/repos/test/repo/contents/src/test.js',
            git_url: 'https://api.github.com/repos/test/repo/git/blobs/abc123',
            html_url: 'https://github.com/test/repo/blob/main/src/test.js',
            repository: {
              id: 1,
              full_name: 'test/repo',
              url: 'https://api.github.com/repos/test/repo',
            },
            score: 1.0,
            file_size: 100,
            language: 'JavaScript',
            last_modified_at: '2024-01-01T00:00:00Z',
            text_matches: [
              {
                object_url:
                  'https://api.github.com/repos/test/repo/contents/src/test.js',
                object_type: 'File',
                property: 'content',
                fragment:
                  'const memoizedValue = useMemo(() => computeExpensiveValue(a, b), [a, b]);',
                matches: [
                  {
                    text: 'useMemo',
                    indices: [15, 22],
                  },
                ],
              },
            ],
          },
        ],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['useMemo', 'React'],
      owner: 'test',
      repo: 'repo',
      limit: 5,
    });

    expect(result).not.toHaveProperty('error');
    const callArgs = mockOctokit.rest.search.code.mock.calls[0]?.[0];
    expect(callArgs.q).toBe('useMemo React repo:test/repo');
    expect(callArgs.q).not.toMatch(/stars:>10/);
  });

  it('should apply analysis research goal correctly', async () => {
    const mockResponse = {
      data: {
        total_count: 1,
        items: [],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['useMemo', 'React'],
      owner: 'test',
      repo: 'repo',
      limit: 5,
    });

    expect(result).not.toHaveProperty('error');
    const callArgs = mockOctokit.rest.search.code.mock.calls[0]?.[0];
    expect(callArgs.q).toBe('useMemo React repo:test/repo');
    expect(callArgs.q).not.toMatch(/stars:>10/);
  });

  it('should apply code_review research goal correctly', async () => {
    const mockResponse = {
      data: {
        total_count: 1,
        items: [],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['useMemo', 'React'],
      owner: 'test',
      repo: 'repo',
      limit: 5,
    });

    expect(result).not.toHaveProperty('error');
    const callArgs = mockOctokit.rest.search.code.mock.calls[0]?.[0];
    expect(callArgs.q).toBe('useMemo React repo:test/repo');
    expect(callArgs.q).not.toMatch(/stars:>10/);
  });

  it('should disable quality boost for specific repo searches', async () => {
    const mockResponse = {
      data: {
        total_count: 1,
        items: [],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['useMemo', 'React'],
      owner: 'facebook',
      repo: 'react',
      limit: 5,
    });

    expect(result).not.toHaveProperty('error');
    const callArgs = mockOctokit.rest.search.code.mock.calls[0]?.[0];
    expect(callArgs).toBeDefined();
    expect(callArgs!.q).not.toMatch(/stars:>10/);
    expect(callArgs!.q).toMatch(/repo:facebook\/react/);
  });

  it('should handle code search with extension filter correctly', async () => {
    const mockResponse = {
      data: {
        total_count: 1,
        items: [],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['useMemo', 'React'],
      owner: 'test',
      repo: 'repo',
      extension: 'tsx',
      limit: 5,
    });

    expect(result).not.toHaveProperty('error');
    const callArgs = mockOctokit.rest.search.code.mock.calls[0]?.[0];
    expect(callArgs.q).toMatch(/extension:tsx/);
  });
});
