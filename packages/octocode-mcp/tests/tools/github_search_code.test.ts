import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core';

// Mock the GitHub client
const mockOctokit = vi.hoisted(() => ({
  rest: {
    search: {
      code: vi.fn(),
    },
  },
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(() => mockOctokit),
}));

// Mock the cache to prevent interference
vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn(async (_key: string, fn: () => unknown) => {
    // Always execute the function, don't use cache
    return await fn();
  }),
}));

// Mock serverConfig
vi.mock('../../src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(() => Promise.resolve('test-token')),
  isLoggingEnabled: vi.fn(() => false),
}));

// Import after mocking
import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';

describe.skip('GitHubCodeSearchQuerySchema', () => {
  describe('new qualifiers validation', () => {
    // Helper to add required research fields to queries
    const withResearchFields = <T extends object>(query: T) => ({
      ...query,
      id: 'test:code-search',
      mainResearchGoal: 'Test research goal',
      researchGoal: 'Testing schema validation',
      reasoning: 'Unit test for schema',
    });

    it('should validate owner qualifier', () => {
      const validOwnerQuery = withResearchFields({
        keywordsToSearch: ['function'],
        owner: 'octocat',
      });

      const result = GitHubCodeSearchQuerySchema.safeParse(validOwnerQuery);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.owner).toBe('octocat');
      }
    });

    it('should validate owner qualifier with organization name', () => {
      const validOrgOwnerQuery = withResearchFields({
        keywordsToSearch: ['function'],
        owner: 'wix-private',
      });

      const result = GitHubCodeSearchQuerySchema.safeParse(validOrgOwnerQuery);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.owner).toBe('wix-private');
      }
    });

    it('should validate path qualifier', () => {
      const pathQuery = withResearchFields({
        keywordsToSearch: ['function'],
        path: 'src/components',
      });

      const result = GitHubCodeSearchQuerySchema.safeParse(pathQuery);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.path).toBe('src/components');
      }
    });

    it('should validate complex query with all qualifiers', () => {
      const complexQuery = withResearchFields({
        keywordsToSearch: ['function', 'component'],
        owner: 'facebook',
        repo: 'react',
        path: 'src/components',
        filename: 'App.js',
        extension: 'js',
        match: 'file',
        limit: 10,
      });

      const result = GitHubCodeSearchQuerySchema.safeParse(complexQuery);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.owner).toBe('facebook');
        expect(result.data.repo).toBe('react');
        expect(result.data.path).toBe('src/components');
        expect(result.data.filename).toBe('App.js');
        expect(result.data.extension).toBe('js');
        expect(result.data.match).toBe('file');
        expect(result.data.limit).toBe(10);
      }
    });

    it('should reject array values for owner (simplified schema)', () => {
      const arrayOwnerQuery = withResearchFields({
        keywordsToSearch: ['function'],
        owner: ['facebook', 'microsoft'],
      });

      const result = GitHubCodeSearchQuerySchema.safeParse(arrayOwnerQuery);
      expect(result.success).toBe(false);
      if (!result.success) {
        // Should fail because owner only accepts strings now
        expect(
          result.error.issues.some(issue => issue.path.includes('owner'))
        ).toBe(true);
      }
    });

    it('should accept minimal query with keywords only', () => {
      const basicQuery = withResearchFields({
        keywordsToSearch: ['function'],
      });

      const result = GitHubCodeSearchQuerySchema.safeParse(basicQuery);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.keywordsToSearch).toEqual(['function']);
      }
    });
  });
});

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
      keywordsToSearch: ['useMemo', 'React'],
      owner: 'test',
      repo: 'repo',
      limit: 5,
    });

    expect(result).not.toHaveProperty('error');
    const callArgs = mockOctokit.rest.search.code.mock.calls[0]?.[0];
    expect(callArgs.q).toBe('useMemo React repo:test/repo');
    expect(callArgs.q).not.toMatch(/stars:>10/);
    // Note: order parameter was deprecated by GitHub in April 2023
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
      keywordsToSearch: ['useMemo', 'React'],
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
      keywordsToSearch: ['useMemo', 'React'],
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
      keywordsToSearch: ['useMemo', 'React'],
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
      keywordsToSearch: ['useMemo', 'React'],
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
