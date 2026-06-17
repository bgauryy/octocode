import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubReposAPI } from '../../../octocode-tools-core/src/github/repoSearch.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { handleGitHubAPIError } from '../../../octocode-tools-core/src/github/errors.js';
import { buildRepoSearchQuery } from '../../../octocode-tools-core/src/github/queryBuilders.js';
import type { GitHubReposSearchQuery } from '../../src/public.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('../../../octocode-tools-core/src/github/errors.js');
vi.mock('../../../octocode-tools-core/src/github/queryBuilders.js');
vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn((_, operation) => operation()),
}));

describe('GitHub Repository Search', () => {
  const mockOctokit = {
    rest: {
      search: {
        repos: vi.fn(),
      },
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();

    vi.mocked(getOctokit).mockResolvedValue(mockOctokit as any);
    vi.mocked(buildRepoSearchQuery).mockReturnValue('test query');
  });

  describe('searchGitHubReposAPI - Success Scenarios', () => {
    it('should search repositories successfully', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'facebook/react',
              stargazers_count: 50000,
              description: 'A JavaScript library for building user interfaces',
              html_url: 'https://github.com/facebook/react',
              created_at: '2013-05-24T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
            },
            {
              full_name: 'vuejs/vue',
              stargazers_count: 40000,
              description: 'Vue.js framework',
              html_url: 'https://github.com/vuejs/vue',
              created_at: '2016-01-01T10:00:00Z',
              updated_at: '2024-01-14T08:20:00Z',
              pushed_at: '2024-01-14T06:00:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['react', 'javascript'],
      };

      const result = await searchGitHubReposAPI(params);

      expect(result).toHaveProperty('data');
      if ('data' in result) {
        expect(result.data).toHaveProperty('repositories');
        expect(result.data.repositories).toHaveLength(2);
        expect(result.data.repositories[0]).toEqual({
          owner: 'facebook',
          repo: 'react',
          stars: 50000,
          description: 'A JavaScript library for building user interfaces',
          url: 'https://github.com/facebook/react',
          createdAt: expect.any(String),
          updatedAt: expect.any(String),
          pushedAt: expect.any(String),
        });
      }
    });

    it('should handle empty query with validation error', async () => {
      vi.mocked(buildRepoSearchQuery).mockReturnValue('');

      const params: GitHubReposSearchQuery = {
        keywords: [],
      };

      const result = await searchGitHubReposAPI(params);

      expect(result).toHaveProperty('error');
      if ('error' in result) {
        expect(result.error).toContain('Search query cannot be empty');
        expect(result.type).toBe('http');
        expect(result.status).toBe(400);
      }
    });

    it('should handle query with only whitespace', async () => {
      vi.mocked(buildRepoSearchQuery).mockReturnValue('   ');

      const params: GitHubReposSearchQuery = {
        keywords: [''],
      };

      const result = await searchGitHubReposAPI(params);

      expect(result).toHaveProperty('error');
      if ('error' in result) {
        expect(result.error).toContain('Search query cannot be empty');
      }
    });

    it('should truncate long descriptions', async () => {
      const longDescription = 'A'.repeat(200);
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: longDescription,
              html_url: 'https://github.com/test/repo',
              updated_at: '2024-01-15T10:30:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]!.description).toHaveLength(153);
        expect(result.data.repositories[0]!.description.endsWith('...')).toBe(
          true
        );
      }
    });

    it('should handle repos without description', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: null,
              html_url: 'https://github.com/test/repo',
              updated_at: '2024-01-15T10:30:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]!.description).toBe('No description');
      }
    });

    it('should preserve API sort order (e.g. best match)', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'low/stars',
              stargazers_count: 100,
              description: 'Low stars',
              html_url: 'https://github.com/low/stars',
              updated_at: '2024-01-15T10:30:00Z',
            },
            {
              full_name: 'high/stars',
              stargazers_count: 50000,
              description: 'High stars',
              html_url: 'https://github.com/high/stars',
              updated_at: '2024-01-15T10:30:00Z',
            },
            {
              full_name: 'medium/stars',
              stargazers_count: 1000,
              description: 'Medium stars',
              html_url: 'https://github.com/medium/stars',
              updated_at: '2024-01-15T10:30:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]!.stars).toBe(100);
        expect(result.data.repositories[1]!.stars).toBe(50000);
        expect(result.data.repositories[2]!.stars).toBe(1000);
      }
    });

    it('should preserve API sort order for equal stars (secondary sort)', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'owner1/older',
              stargazers_count: 1000,
              description: 'Older',
              html_url: 'https://github.com/owner1/older',
              updated_at: '2024-01-10T10:30:00Z',
            },
            {
              full_name: 'owner2/newer',
              stargazers_count: 1000,
              description: 'Newer',
              html_url: 'https://github.com/owner2/newer',
              updated_at: '2024-01-15T10:30:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]!.owner).toBe('owner1');
        expect(result.data.repositories[0]!.repo).toBe('older');
        expect(result.data.repositories[1]!.owner).toBe('owner2');
        expect(result.data.repositories[1]!.repo).toBe('newer');
      }
    });

    it('should handle sort parameter', async () => {
      const mockResponse = {
        data: {
          items: [],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
        sort: 'stars',
      };

      await searchGitHubReposAPI(params);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith(
        expect.objectContaining({
          sort: 'stars',
        })
      );
    });

    it('should not include sort for best-match', async () => {
      const mockResponse = {
        data: {
          items: [],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
        sort: 'best-match',
      };

      await searchGitHubReposAPI(params);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith(
        expect.not.objectContaining({
          sort: 'best-match',
        })
      );
    });

    it('should respect limit parameter', async () => {
      const mockResponse = {
        data: {
          items: [],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
        limit: 50,
      };

      await searchGitHubReposAPI(params);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 50,
        })
      );
    });

    it('should cap limit at 100', async () => {
      const mockResponse = {
        data: {
          items: [],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
        limit: 200,
      };

      await searchGitHubReposAPI(params);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 100,
        })
      );
    });

    it('should use default limit of 30', async () => {
      const mockResponse = {
        data: {
          items: [],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      await searchGitHubReposAPI(params);

      expect(mockOctokit.rest.search.repos).toHaveBeenCalledWith(
        expect.objectContaining({
          per_page: 30,
        })
      );
    });

    it('should handle repos with missing stargazers_count', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: null,
              description: 'Test',
              html_url: 'https://github.com/test/repo',
              updated_at: '2024-01-15T10:30:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]!.stars).toBe(0);
      }
    });

    it('should handle malformed full_name', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'no-slash',
              stargazers_count: 100,
              description: 'Test',
              html_url: 'https://github.com/test',
              updated_at: '2024-01-15T10:30:00Z',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]!.owner).toBe('no-slash');
        expect(result.data.repositories[0]!.repo).toBe('');
      }
    });
  });

  describe('searchGitHubReposAPI - New Research Context Fields', () => {
    it('should include defaultBranch when present', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              default_branch: 'main',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).toHaveProperty('defaultBranch');
        expect(result.data.repositories[0]!.defaultBranch).toBe('main');
      }
    });

    it('should include visibility when present', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              visibility: 'public',
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).toHaveProperty('visibility');
        expect(result.data.repositories[0]!.visibility).toBe('public');
      }
    });

    it('should include topics when array is non-empty', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              topics: ['typescript', 'react', 'testing'],
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).toHaveProperty('topics');
        expect(result.data.repositories[0]!.topics).toEqual([
          'typescript',
          'react',
          'testing',
        ]);
      }
    });

    it('should not include topics when array is empty', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              topics: [],
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).not.toHaveProperty('topics');
      }
    });

    it('should include forksCount when greater than 0', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              forks_count: 42,
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).toHaveProperty('forksCount');
        expect(result.data.repositories[0]!.forksCount).toBe(42);
      }
    });

    it('should not include forksCount when 0', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              forks_count: 0,
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).not.toHaveProperty('forksCount');
      }
    });

    it('should include openIssuesCount when greater than 0', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              open_issues_count: 15,
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).toHaveProperty('openIssuesCount');
        expect(result.data.repositories[0]!.openIssuesCount).toBe(15);
      }
    });

    it('should not include openIssuesCount when 0', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'test/repo',
              stargazers_count: 100,
              description: 'Test repo',
              html_url: 'https://github.com/test/repo',
              created_at: '2024-01-01T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              open_issues_count: 0,
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        expect(result.data.repositories[0]).not.toHaveProperty(
          'openIssuesCount'
        );
      }
    });

    it('should include all new fields when all are present', async () => {
      const mockResponse = {
        data: {
          items: [
            {
              full_name: 'facebook/react',
              stargazers_count: 50000,
              description: 'A JavaScript library',
              html_url: 'https://github.com/facebook/react',
              created_at: '2013-05-24T10:00:00Z',
              updated_at: '2024-01-15T10:30:00Z',
              pushed_at: '2024-01-15T08:00:00Z',
              default_branch: 'main',
              visibility: 'public',
              topics: ['javascript', 'react', 'frontend'],
              forks_count: 15000,
              open_issues_count: 500,
            },
          ],
        },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['react'],
      };

      const result = await searchGitHubReposAPI(params);

      if ('data' in result) {
        const repo = result.data.repositories[0]!;
        expect(repo.defaultBranch).toBe('main');
        expect(repo.visibility).toBe('public');
        expect(repo.topics).toEqual(['javascript', 'react', 'frontend']);
        expect(repo.forksCount).toBe(15000);
        expect(repo.openIssuesCount).toBe(500);
      }
    });
  });

  describe('searchGitHubReposAPI - Error Handling', () => {
    it('should handle API errors', async () => {
      const mockError = new Error('API Error');
      mockOctokit.rest.search.repos.mockRejectedValue(mockError);
      vi.mocked(handleGitHubAPIError).mockReturnValue({
        error: 'API Error',
        type: 'http',
      });

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      expect(result).toHaveProperty('error');
      expect(handleGitHubAPIError).toHaveBeenCalledWith(mockError);
    });

    it('should handle network errors', async () => {
      const networkError = new Error('Network timeout');
      mockOctokit.rest.search.repos.mockRejectedValue(networkError);
      vi.mocked(handleGitHubAPIError).mockReturnValue({
        error: 'Network timeout',
        type: 'network',
      });

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      const result = await searchGitHubReposAPI(params);

      expect(result).toHaveProperty('error');
      if ('error' in result) {
        expect(result.type).toBe('network');
      }
    });

    it('should pass authInfo to getOctokit', async () => {
      const mockResponse = {
        data: { items: [] },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const authInfo = { user: 'test-user', token: 'test-token' };
      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      await searchGitHubReposAPI(params, authInfo as any);

      expect(getOctokit).toHaveBeenCalledWith(authInfo);
    });
  });

  describe('searchGitHubReposAPI - Caching', () => {
    it('should use cache with session ID', async () => {
      const { withDataCache } =
        await import('../../../octocode-tools-core/src/utils/http/cache.js');
      const mockWithDataCache = vi.mocked(withDataCache);

      const mockResponse = {
        data: { items: [] },
        headers: {},
      };

      mockOctokit.rest.search.repos.mockResolvedValue(mockResponse);

      const params: GitHubReposSearchQuery = {
        keywords: ['test'],
      };

      await searchGitHubReposAPI(params, undefined, 'test-session');

      expect(mockWithDataCache).toHaveBeenCalled();
    });
  });
});
