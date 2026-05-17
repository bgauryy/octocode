/**
 * Rate Limit Integration Tests
 *
 * These tests verify that the provider error handling properly extracts
 * rate limit information from API errors and propagates it to consumers.
 *
 * These tests address the issues documented in RATE_LIMIT_IMPROVEMENT_PLAN.md:
 * - RL-001: handleError methods now use proper error handlers
 * - RL-002: rateLimit info is now propagated to ProviderResponse
 * - RL-003: Existing error handlers are now utilized
 * - RL-004: Tests verify REAL implementation (not mocked responses)
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { GitHubProvider } from '../../src/providers/github/GitHubProvider.js';
import { GitLabProvider } from '../../src/providers/gitlab/GitLabProvider.js';
import { RequestError } from 'octokit';

// Mock the API functions to throw rate limit errors
vi.mock('../../src/github/codeSearch.js', () => ({
  searchGitHubCodeAPI: vi.fn(),
}));

vi.mock('../../src/gitlab/codeSearch.js', () => ({
  searchGitLabCodeAPI: vi.fn(),
}));

import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';
import { searchGitLabCodeAPI } from '../../src/gitlab/codeSearch.js';

const mockSearchGitHubCodeAPI = searchGitHubCodeAPI as ReturnType<typeof vi.fn>;
const mockSearchGitLabCodeAPI = searchGitLabCodeAPI as ReturnType<typeof vi.fn>;

describe('Rate Limit Integration Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('GitHubProvider rate limit handling', () => {
    let provider: GitHubProvider;

    beforeEach(() => {
      provider = new GitHubProvider({ type: 'github', token: 'test-token' });
    });

    it('should extract rate limit info from primary rate limit error (403 with x-ratelimit headers)', async () => {
      const resetTimestamp = Math.floor(Date.now() / 1000) + 3600;

      // Create a realistic RequestError like GitHub API would return
      const rateLimitError = new RequestError('API rate limit exceeded', 403, {
        response: {
          url: 'https://api.github.com/search/code',
          status: 403,
          headers: {
            'x-ratelimit-remaining': '0',
            'x-ratelimit-reset': String(resetTimestamp),
          },
          data: {
            message: 'API rate limit exceeded for user',
            documentation_url: 'https://docs.github.com/rest/rate-limiting',
          },
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com/search/code',
          headers: {},
        },
      });

      mockSearchGitHubCodeAPI.mockRejectedValue(rateLimitError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      // Verify the error is properly extracted
      expect(result.error).toBeDefined();
      expect(result.status).toBe(403);
      expect(result.provider).toBe('github');

      // Verify rate limit info is properly extracted
      expect(result.rateLimit).toBeDefined();
      expect(result.rateLimit?.remaining).toBe(0);
      expect(result.rateLimit?.reset).toBeDefined();
      expect(result.rateLimit?.reset).toBeGreaterThan(
        Math.floor(Date.now() / 1000)
      );
    });

    it('should extract rate limit info from secondary rate limit error (403 with retry-after)', async () => {
      const rateLimitError = new RequestError(
        'You have exceeded a secondary rate limit',
        403,
        {
          response: {
            url: 'https://api.github.com/search/code',
            status: 403,
            headers: {
              'retry-after': '60',
            },
            data: {
              message: 'You have exceeded a secondary rate limit',
            },
            retryCount: 0,
          },
          request: {
            method: 'GET',
            url: 'https://api.github.com/search/code',
            headers: {},
          },
        }
      );

      mockSearchGitHubCodeAPI.mockRejectedValue(rateLimitError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result.error).toBeDefined();
      expect(result.status).toBe(403);

      // Verify rate limit info
      expect(result.rateLimit).toBeDefined();
      expect(result.rateLimit?.remaining).toBe(0);
      expect(result.rateLimit?.retryAfter).toBe(60);
    });

    it('should preserve rate limit info returned by GitHub API helpers', async () => {
      const resetMs = Date.now() + 60_000;
      mockSearchGitHubCodeAPI.mockResolvedValue({
        error: 'API rate limit exceeded',
        status: 403,
        type: 'http',
        rateLimitRemaining: 0,
        rateLimitReset: resetMs,
        retryAfter: 60,
      });

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result.status).toBe(403);
      expect(result.provider).toBe('github');
      expect(result.rateLimit).toEqual({
        remaining: 0,
        reset: Math.floor(resetMs / 1000),
        retryAfter: 60,
      });
    });

    it('should NOT include rateLimit for non-rate-limit errors', async () => {
      const notFoundError = new RequestError('Not Found', 404, {
        response: {
          url: 'https://api.github.com/repos/owner/repo',
          status: 404,
          headers: {},
          data: { message: 'Not Found' },
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/owner/repo',
          headers: {},
        },
      });

      mockSearchGitHubCodeAPI.mockRejectedValue(notFoundError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      expect(result.status).toBe(404);
      expect(result.rateLimit).toBeUndefined();
    });

    it('should propagate correct status code instead of always returning 500', async () => {
      const forbiddenError = new RequestError('Resource not accessible', 403, {
        response: {
          url: 'https://api.github.com/search/code',
          status: 403,
          headers: {
            'x-ratelimit-remaining': '100', // Not a rate limit issue
          },
          data: { message: 'Resource not accessible by integration' },
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com/search/code',
          headers: {},
        },
      });

      mockSearchGitHubCodeAPI.mockRejectedValue(forbiddenError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'owner/repo',
      });

      // Should return 403, NOT 500 like the old broken implementation
      expect(result.status).toBe(403);
    });
  });

  describe('GitLabProvider rate limit handling', () => {
    let provider: GitLabProvider;

    beforeEach(() => {
      provider = new GitLabProvider();
    });

    it('should extract rate limit info from 429 response', async () => {
      // Simulate GitLab 429 rate limit error
      const rateLimitError = {
        cause: {
          description: 'Rate limit exceeded',
          status: 429,
        },
        response: {
          status: 429,
          headers: {
            'retry-after': '30',
            'ratelimit-reset': String(Math.floor(Date.now() / 1000) + 30),
          },
        },
      };

      mockSearchGitLabCodeAPI.mockRejectedValue(rateLimitError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'group/project',
      });

      expect(result.status).toBe(429);
      expect(result.provider).toBe('gitlab');

      // Verify rate limit info
      expect(result.rateLimit).toBeDefined();
      expect(result.rateLimit?.remaining).toBe(0);
      expect(result.rateLimit?.retryAfter).toBe(30);
    });

    it('should NOT include rateLimit for non-rate-limit errors', async () => {
      const notFoundError = {
        cause: {
          description: 'Project not found',
          status: 404,
        },
      };

      mockSearchGitLabCodeAPI.mockRejectedValue(notFoundError);

      const result = await provider.searchCode({
        keywords: ['test'],
        projectId: 'group/project',
      });

      expect(result.status).toBe(404);
      expect(result.rateLimit).toBeUndefined();
    });
  });
});
