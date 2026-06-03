import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Use vi.hoisted to ensure mocks are available during module initialization
const mockOctokit = vi.hoisted(() => ({
  rest: {
    search: {
      code: vi.fn(),
      repos: vi.fn(),
      commits: vi.fn(),
      issuesAndPullRequests: vi.fn(),
    },
    repos: {
      getContent: vi.fn(),
      get: vi.fn(),
      getCommit: vi.fn(),
    },
    pulls: {
      get: vi.fn(),
      listCommits: vi.fn(),
    },
  },
}));

const mockOctokitWithThrottling = vi.hoisted(() => {
  const MockClass = vi
    .fn()
    .mockImplementation(() => mockOctokit) as unknown as Record<
    string,
    unknown
  > & {
    mockImplementation: (fn: () => unknown) => unknown;
  };
  MockClass.plugin = vi.fn().mockReturnValue(MockClass);
  return MockClass;
});

const mockGenerateCacheKey = vi.hoisted(() => vi.fn());
const mockWithCache = vi.hoisted(() => vi.fn());
const mockWithDataCache = vi.hoisted(() => vi.fn());
const mockCreateResult = vi.hoisted(() => vi.fn());
const mockContentSanitizer = vi.hoisted(() => ({
  sanitizeContent: vi.fn(),
}));
const mockminifyContent = vi.hoisted(() => vi.fn());
const mockOptimizeTextMatch = vi.hoisted(() => vi.fn());

// Mock dependencies
vi.mock('octokit', () => ({
  Octokit: mockOctokitWithThrottling,
  RequestError: class RequestError extends Error {
    public status: number;
    public request: Record<string, unknown>;
    public response?: Record<string, unknown>;

    constructor(
      message: string,
      statusCode: number,
      options: {
        request: Record<string, unknown>;
        response?: Record<string, unknown>;
      }
    ) {
      super(message);
      this.name = 'HttpError';
      this.status = statusCode;
      this.request = options.request;
      this.response = options.response;
    }
  },
}));

vi.mock('@octokit/plugin-throttling', () => ({
  throttling: vi.fn(),
}));

vi.mock('../../src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withCache: mockWithCache,
  withDataCache: mockWithDataCache,
}));

vi.mock('../../src/mcp/responses.js', () => ({
  createResult: mockCreateResult,
  optimizeTextMatch: mockOptimizeTextMatch,
}));

vi.mock('octocode-security-utils/contentSanitizer', () => ({
  ContentSanitizer: mockContentSanitizer,
}));

vi.mock('../../src/utils/minifier/minifier.js', () => ({
  minifyContent: mockminifyContent,
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(() => Promise.resolve(mockOctokit)),
  clearOctokitInstances: vi.fn(),
}));

// Import after mocking
import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';
import { searchGitHubReposAPI } from '../../src/github/repoSearch.js';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure.js';
import { searchGitHubPullRequestsAPI } from '../../src/github/pullRequestSearch.js';
import type { GitHubCodeSearchQuery } from '@octocodeai/octocode-core';
import { initialize, cleanup } from '../../src/serverConfig.js';

describe('GitHub API Utils', () => {
  beforeEach(async () => {
    // Clear all mocks
    vi.clearAllMocks();

    // Initialize config
    await initialize();

    // Reset Octokit mock implementation
    mockOctokitWithThrottling.mockImplementation(() => mockOctokit);

    // Set up default mock behaviors
    mockGenerateCacheKey.mockReturnValue('test-cache-key');
    mockWithCache.mockImplementation(async (_key, fn) => await fn());
    mockWithDataCache.mockImplementation(async (_key, fn) => await fn());
    mockCreateResult.mockImplementation(params => ({
      content: [{ type: 'text', text: JSON.stringify(params) }],
      isError: params.isError || false,
    }));
    mockContentSanitizer.sanitizeContent.mockImplementation(content => ({
      content: content, // Return the content as-is for most tests
      warnings: [],
      hasSecrets: false,

      secretsDetected: [],
    }));
    mockminifyContent.mockResolvedValue({
      content: 'minified content',
      failed: false,
      type: 'javascript',
    });
    mockOptimizeTextMatch.mockImplementation(text => text);

    // Set up environment
    process.env.GITHUB_TOKEN = 'test-token';
  }, 15000); // Increase timeout to 15 seconds to handle slow GitHub CLI calls

  afterEach(() => {
    vi.resetAllMocks();
    cleanup();
    delete process.env.GITHUB_TOKEN;
    delete process.env.GH_TOKEN;
  });

  describe('Basic Setup', () => {
    it('should import all API functions', () => {
      expect(typeof searchGitHubCodeAPI).toEqual('function');
      expect(typeof searchGitHubReposAPI).toEqual('function');
      expect(typeof fetchGitHubFileContentAPI).toEqual('function');
      expect(typeof viewGitHubRepositoryStructureAPI).toEqual('function');
      expect(typeof searchGitHubPullRequestsAPI).toEqual('function');
    });
  });

  describe('GitHub Code Search API', () => {
    it('should search GitHub code successfully', async () => {
      const mockSearchResponse = {
        data: {
          total_count: 1,
          items: [
            {
              path: 'src/components/Button.tsx',
              repository: {
                id: 12345,
                full_name: 'facebook/react',
                html_url: 'https://github.com/facebook/react',
                fork: false,
                private: false,
              },
              sha: 'abc123',
              html_url:
                'https://github.com/facebook/react/blob/main/src/components/Button.tsx',
              text_matches: [
                {
                  fragment:
                    'const Button = () => {\n  return <button>Click me</button>;\n}',
                  matches: [
                    {
                      text: 'Button',
                      indices: [6, 12],
                    },
                  ],
                },
              ],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockSearchResponse);

      const params = {
        keywordsToSearch: ['Button'],
        language: 'typescript',
        owner: 'facebook',
        repo: 'react',

        minify: true,
      };

      await searchGitHubCodeAPI(params);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'Button repo:facebook/react',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });

      // The function should return the result directly, not call createResult
      const result = await searchGitHubCodeAPI(params);
      expect(result).toEqual(
        expect.objectContaining({
          data: expect.objectContaining({
            total_count: 1,
            items: expect.any(Array),
          }),
          status: 200,
        })
      );
    });

    it('should handle empty search query', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
        status: 200,
      });

      const params = {
        keywordsToSearch: [''],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };

      const result = await searchGitHubCodeAPI(params);

      // Empty queries should return an error since they don't provide meaningful search terms
      expect(result).toHaveProperty('error');
      expect((result as { error: string }).error).toBe(
        'Search query cannot be empty'
      );
    });

    it('should handle GitHub API rate limit error', async () => {
      const { RequestError } = await import('octokit');
      const rateLimitError = new RequestError('Rate limit exceeded', 403, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/search/code',
          headers: {},
        },
        response: {
          headers: {
            'x-ratelimit-remaining': '0',
            'x-ratelimit-reset': '1640995200',
          },
          status: 403,
          url: 'https://api.github.com/search/code',
          data: {},
        },
      } as never);

      mockOctokit.rest.search.code.mockRejectedValue(rateLimitError);

      const params = {
        keywordsToSearch: ['test'],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };
      const result = await searchGitHubCodeAPI(params);

      expect(result).toEqual(
        expect.objectContaining({
          error: expect.stringContaining('rate limit'),
          status: 403,
          type: 'http',
        })
      );
    });

    it('should handle authentication error', async () => {
      const { RequestError } = await import('octokit');
      const authError = new RequestError('Bad credentials', 401, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/search/code',
          headers: {},
        },
      } as never);

      mockOctokit.rest.search.code.mockRejectedValue(authError);

      const params = {
        keywordsToSearch: ['test'],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };
      const result = await searchGitHubCodeAPI(params);

      expect(result).toEqual(
        expect.objectContaining({
          error: 'GitHub authentication required',
          status: 401,
          type: 'http',
        })
      );
    });

    it('should handle validation error', async () => {
      const { RequestError } = await import('octokit');
      const validationError = new RequestError('Validation failed', 422, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/search/code',
          headers: {},
        },
      } as never);

      mockOctokit.rest.search.code.mockRejectedValue(validationError);

      const params = {
        keywordsToSearch: ['test'],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };
      const result = await searchGitHubCodeAPI(params);

      expect(result).toEqual(
        expect.objectContaining({
          error: 'Invalid search query or request parameters',
          status: 422,
          type: 'http',
        })
      );
    });

    it('should build complex search queries correctly', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
      });

      const params: GitHubCodeSearchQuery = {
        keywordsToSearch: ['function', 'export'],
        owner: 'microsoft',
        repo: 'vscode',
        filename: 'index.js',
        extension: 'js',
        path: 'src',
        match: 'file',
      };

      await searchGitHubCodeAPI(params);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function export filename:index.js extension:js path:src repo:microsoft/vscode in:file',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });
    });

    it('should handle user vs org distinction', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
      });

      // Test owner qualifier (automatically detects user vs org)
      const userParams = {
        keywordsToSearch: ['function'],
        owner: 'octocat',
        repo: 'test',
        minify: true,
      };

      await searchGitHubCodeAPI(userParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:octocat/test',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });

      // Test org owner (implementation auto-detects)
      const orgParams = {
        keywordsToSearch: ['function'],
        owner: 'github',
        repo: 'test',
        minify: true,
      };

      await searchGitHubCodeAPI(orgParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:github/test', // Implementation uses repo: when both owner and repo are provided
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });

      // Test multiple owners (array format)
      const multipleOwnersParams = {
        keywordsToSearch: ['function'],
        owner: 'octocat',
        repo: 'test',
        minify: true,
        excludeArchived: true,
        excludeForks: false,
      };

      await searchGitHubCodeAPI(multipleOwnersParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:octocat/test', // Implementation uses repo: when repo is provided
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });
    });

    it('should handle fork qualifier', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
      });

      // Test basic search without quality boost
      const forkTrueParams = {
        keywordsToSearch: ['function'],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };

      await searchGitHubCodeAPI(forkTrueParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:test/repo',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });

      // Test with specific repo (disables quality boost)
      const forkFalseParams = {
        keywordsToSearch: ['function'],
        owner: 'facebook',
        repo: 'react',
        minify: true,
      };

      await searchGitHubCodeAPI(forkFalseParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:facebook/react',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });

      // Test with owner and repo filters
      const ownerRepoParams = {
        keywordsToSearch: ['function'],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };

      await searchGitHubCodeAPI(ownerRepoParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:test/repo',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });
    });

    it('should handle archived qualifier', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
      });

      // Test with quality boost enabled (adds stars and pushed filters)
      const archivedTrueParams = {
        keywordsToSearch: ['function'],
        owner: 'test',
        repo: 'repo',
        minify: true,
      };

      await searchGitHubCodeAPI(archivedTrueParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:test/repo',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });

      // Test with specific owner (disables quality boost)
      const archivedFalseParams = {
        keywordsToSearch: ['function'],
        owner: 'microsoft',
        repo: 'test',
        minify: true,
      };

      await searchGitHubCodeAPI(archivedFalseParams);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:microsoft/test',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });
    });

    it('should prioritize owner+repo over user/org qualifiers', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
      });

      // When both owner+repo and user/org are provided, owner+repo should take precedence
      const params = {
        keywordsToSearch: ['function'],
        owner: 'facebook',
        repo: 'react',
        minify: true,
      };

      await searchGitHubCodeAPI(params);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:facebook/react',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });
    });

    it('should handle all new qualifiers together', async () => {
      mockOctokit.rest.search.code.mockResolvedValue({
        data: { total_count: 0, items: [] },
      });

      const params = {
        keywordsToSearch: ['function'],
        owner: 'octocat',
        repo: 'test',
        language: 'javascript',
        minify: true,
      };

      await searchGitHubCodeAPI(params);

      expect(mockOctokit.rest.search.code).toHaveBeenCalledWith({
        q: 'function repo:octocat/test',
        per_page: 30,
        page: 1,
        headers: {
          Accept: 'application/vnd.github.v3.text-match+json',
        },
      });
    });

    describe('GitHub Repository Search API', () => {
      it('should search GitHub repositories successfully', async () => {
        const mockRepoResponse = {
          data: {
            total_count: 1,
            items: [
              {
                full_name: 'facebook/react',
                stargazers_count: 50000,
                description:
                  'A declarative, efficient, and flexible JavaScript library for building user interfaces.',
                language: 'JavaScript',
                html_url: 'https://github.com/facebook/react',
                forks: 15000,
                created_at: '2013-05-24T10:00:00Z',
                updated_at: '2023-12-01T10:00:00Z',
                pushed_at: '2023-12-01T08:00:00Z',
                owner: { login: 'facebook' },
              },
            ],
          },
        };

        mockOctokit.rest.search.repos.mockResolvedValue(mockRepoResponse);

        const params = {
          keywordsToSearch: ['react'],
          stars: '>1000',
        };

        await searchGitHubReposAPI(params);

        expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith({
          q: 'react stars:>1000 is:not-archived',
          per_page: 30,
          page: 1,
        });

        // The function should return the result directly, not call createResult
        const result = await searchGitHubReposAPI(params);
        expect(result).toEqual({
          data: {
            repositories: [
              {
                owner: 'facebook',
                repo: 'react',
                defaultBranch: undefined,
                stars: 50000,
                description:
                  'A declarative, efficient, and flexible JavaScript library for building user interfaces.',
                url: 'https://github.com/facebook/react',
                createdAt: '2013-05-24T10:00:00Z',
                updatedAt: '2023-12-01T10:00:00Z',
                pushedAt: '2023-12-01T08:00:00Z',
                visibility: undefined,
                language: 'JavaScript',
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              perPage: 30,
              totalMatches: 1,
              hasMore: false,
            },
          },
          status: 200,
          // normalizeResponseHeaders() returns a clean {} when the provider
          // omits headers (was previously `undefined` cast as a Record).
          headers: {},
          rawResponseChars: expect.any(Number),
        });
      });

      it('should handle empty search query for repositories', async () => {
        mockOctokit.rest.search.repos.mockResolvedValue({
          data: { total_count: 0, items: [] },
          status: 200,
        });

        const params = {};
        const result = await searchGitHubReposAPI(params);

        // With the new implementation, empty queries work because we always add archive/fork filters
        expect(result).not.toHaveProperty('error');
      });

      it('should build complex repository search queries', async () => {
        mockOctokit.rest.search.repos.mockResolvedValue({
          data: { total_count: 0, items: [] },
        });

        const params = {
          keywordsToSearch: ['machine', 'learning'],
          owner: 'google',
          topicsToSearch: ['ml', 'ai'],
          stars: '>100',
          forks: '10..50',
          size: '<1000',
          created: '>2020-01-01',
          updated: '<2023-12-31',
          archived: false,
          'include-forks': 'false' as const,
          match: ['name', 'description'] as (
            | 'name'
            | 'description'
            | 'readme'
          )[],
          sort: 'stars' as const,
        };

        await searchGitHubReposAPI(params);

        const expectedQuery =
          'machine learning user:google topic:ml topic:ai stars:>100 size:<1000 created:>2020-01-01 pushed:<2023-12-31 in:name in:description is:not-archived';

        expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith({
          q: expectedQuery,
          per_page: 30,
          page: 1,
          sort: 'stars',
        });
      });

      it('should handle repository search rate limit error', async () => {
        const { RequestError } = await import('octokit');
        const rateLimitError = new RequestError('Rate limit exceeded', 403, {
          request: {
            method: 'GET',
            url: 'https://api.github.com/search/repositories',
            headers: {},
          },
          response: {
            headers: {
              'x-ratelimit-remaining': '0',
              'x-ratelimit-reset': '1640995200',
            },
            status: 403,
            url: 'https://api.github.com/search/repositories',
            data: {},
          },
        } as never);

        mockOctokit.rest.search.repos.mockRejectedValue(rateLimitError);

        const params = { keywordsToSearch: ['test'] };
        const result = await searchGitHubReposAPI(params);

        expect(result).toEqual(
          expect.objectContaining({
            error: expect.stringContaining('rate limit'),
            status: 403,
            type: 'http',
          })
        );
      });

      it('should truncate long descriptions', async () => {
        const longDescription =
          'This is a very long description that exceeds the 150 character limit and should be truncated with ellipsis to keep the output manageable and readable for users who are browsing repositories.';

        const mockRepoResponse = {
          data: {
            total_count: 1,
            items: [
              {
                full_name: 'test/repo',
                stargazers_count: 100,
                description: longDescription,
                language: 'JavaScript',
                html_url: 'https://github.com/test/repo',
                forks_count: 10,
                updated_at: '2023-12-01T10:00:00Z',
                owner: { login: 'test' },
              },
            ],
          },
        };

        mockOctokit.rest.search.repos.mockResolvedValue(mockRepoResponse);

        const params = { keywordsToSearch: ['test'] };
        const result = await searchGitHubReposAPI(params);

        expect(result).toEqual(
          expect.objectContaining({
            data: expect.objectContaining({
              repositories: [
                expect.objectContaining({
                  description: longDescription.substring(0, 150) + '...',
                }),
              ],
            }),
            status: 200,
          })
        );
      });
    });
  });
});
