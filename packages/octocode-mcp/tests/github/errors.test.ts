import { describe, it, expect } from 'vitest';
import { RequestError } from 'octokit';
import {
  handleGitHubAPIError,
  isNoResultsSearchError,
} from '../../src/github/errors.js';

function makeSearch422(
  errorEntries: Array<Record<string, unknown>>,
  message = 'Validation Failed'
): RequestError {
  return new RequestError(message, 422, {
    response: {
      status: 422,
      headers: {},
      data: { message, errors: errorEntries },
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

describe('GitHub Error Handling', () => {
  describe('handleGitHubAPIError', () => {
    it('should handle 401 authentication error', () => {
      const error = new RequestError('Authentication required', 401, {
        response: {
          status: 401,
          headers: {},
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'GitHub authentication required',
        status: 401,
        type: 'http',
        scopesSuggestion:
          "TELL THE USER: Refresh your GitHub token! Run 'gh auth login' OR 'gh auth refresh' OR set a new GITHUB_TOKEN/GH_TOKEN environment variable",
      });
    });

    it('should handle 403 rate limit error with reset time', () => {
      const error = new RequestError('Rate limit exceeded', 403, {
        response: {
          status: 403,
          headers: {
            'x-ratelimit-remaining': '0',
            'x-ratelimit-reset': '1640995200',
          },
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);
      const resetTime = new Date(1640995200 * 1000);

      expect(result).toMatchObject({
        error: expect.stringContaining('GitHub API rate limit exceeded'),
        status: 403,
        type: 'http',
        rateLimitRemaining: 0,
        rateLimitReset: resetTime.getTime(),
        retryAfter: expect.any(Number),
        scopesSuggestion:
          'Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)',
      });
      expect(result.error).toContain(resetTime.toISOString());
      expect(result.error).toContain('seconds');
    });

    it('should handle 403 rate limit error without reset time', () => {
      const error = new RequestError('Rate limit exceeded', 403, {
        response: {
          status: 403,
          headers: {
            'x-ratelimit-remaining': '0',
          },
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error:
          'GitHub API rate limit exceeded. Reset time unavailable - check GitHub status or try again later',
        status: 403,
        type: 'http',
        rateLimitRemaining: 0,
        rateLimitReset: undefined,
        retryAfter: undefined,
        scopesSuggestion:
          'Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)',
      });
    });

    it('should handle 403 secondary rate limit error', () => {
      const error = new RequestError(
        'You have exceeded a secondary rate limit',
        403,
        {
          response: {
            status: 403,
            headers: {
              'retry-after': '120',
            },
            data: {},
            url: 'https://api.github.com',
            retryCount: 0,
          },
          request: {
            method: 'GET',
            url: 'https://api.github.com',
            headers: {},
          },
        }
      );

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'GitHub secondary rate limit triggered. Retry after 120 seconds',
        status: 403,
        type: 'http',
        rateLimitRemaining: 0,
        retryAfter: 120,
        scopesSuggestion: 'Reduce request frequency to avoid abuse detection',
      });
    });

    it('should handle 403 secondary rate limit without retry-after header', () => {
      const error = new RequestError(
        'You have triggered the secondary rate limit',
        403,
        {
          response: {
            status: 403,
            headers: {},
            data: {},
            url: 'https://api.github.com',
            retryCount: 0,
          },
          request: {
            method: 'GET',
            url: 'https://api.github.com',
            headers: {},
          },
        }
      );

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'GitHub secondary rate limit triggered. Retry after 60 seconds',
        status: 403,
        type: 'http',
        rateLimitRemaining: 0,
        retryAfter: 60,
        scopesSuggestion: 'Reduce request frequency to avoid abuse detection',
      });
    });

    it('should handle GraphQL rate limit error', () => {
      const futureResetTime = Math.floor(Date.now() / 1000) + 3600;
      const error = new RequestError('GraphQL rate limit exceeded', 403, {
        response: {
          status: 403,
          headers: {
            'x-ratelimit-reset': String(futureResetTime),
          },
          data: {
            errors: [
              {
                type: 'RATE_LIMITED',
                message: 'API rate limit exceeded',
              },
            ],
          },
          url: 'https://api.github.com/graphql',
          retryCount: 0,
        },
        request: {
          method: 'POST',
          url: 'https://api.github.com/graphql',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);
      const resetTime = new Date(futureResetTime * 1000);

      expect(result).toMatchObject({
        error: expect.stringContaining('GitHub API rate limit exceeded'),
        status: 403,
        type: 'http',
        rateLimitRemaining: 0,
        rateLimitReset: resetTime.getTime(),
        retryAfter: expect.any(Number),
        scopesSuggestion:
          'Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)',
      });
      expect(result.error).toContain(resetTime.toISOString());
      expect(result.retryAfter).toBeGreaterThan(3600);
      expect(result.retryAfter).toBeLessThan(3610);
    });

    it('should handle 429 rate limit error with retry-after headers', () => {
      const resetTimestamp = Math.floor(Date.now() / 1000) + 120;
      const error = new RequestError('Too many requests', 429, {
        response: {
          status: 429,
          headers: {
            'retry-after': '45',
            'x-ratelimit-remaining': '0',
            'x-ratelimit-reset': String(resetTimestamp),
          },
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);

      expect(result).toMatchObject({
        error: 'Too many requests',
        status: 429,
        type: 'http',
        rateLimitRemaining: 0,
        rateLimitReset: resetTimestamp * 1000,
        retryAfter: 45,
        scopesSuggestion:
          'Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)',
      });
    });

    it('should handle 403 permissions error with scope suggestions', () => {
      const error = new RequestError('Forbidden', 403, {
        response: {
          status: 403,
          headers: {
            'x-ratelimit-remaining': '5000',
            'x-accepted-oauth-scopes': 'repo, read:org',
            'x-oauth-scopes': 'read:user',
          },
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'Access forbidden - insufficient permissions',
        status: 403,
        type: 'http',
        scopesSuggestion:
          'Missing required scopes: repo, read:org. Run: gh auth refresh -s repo -s read:org',
      });
    });

    it('should handle 404 not found error', () => {
      const error = new RequestError('Not Found', 404, {
        response: {
          status: 404,
          headers: {},
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'Repository, resource, or path not found',
        status: 404,
        type: 'http',
      });
    });

    it('should handle 422 validation error', () => {
      const error = new RequestError('Validation Failed', 422, {
        response: {
          status: 422,
          headers: {},
          data: {},
          url: 'https://api.github.com',
          retryCount: 0,
        },
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
      });

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'Invalid search query or request parameters',
        status: 422,
        type: 'http',
        scopesSuggestion: 'Check search syntax and parameter values',
      });
    });

    it('should handle network connection errors', () => {
      const error = new Error('getaddrinfo ENOTFOUND api.github.com');

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'Network connection failed',
        type: 'network',
        scopesSuggestion: 'Check internet connection and GitHub API status',
      });
    });

    it('should handle timeout errors', () => {
      const error = new Error('Request timeout exceeded');

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'Request timeout',
        type: 'network',
        scopesSuggestion: 'Retry the request or check network connectivity',
      });
    });

    it('should handle generic Error objects', () => {
      const error = new Error('Something went wrong');

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'Something went wrong',
        type: 'unknown',
      });
    });

    it('should handle non-Error objects', () => {
      const error = 'String error';

      const result = handleGitHubAPIError(error);

      expect(result).toEqual({
        error: 'String error',
        type: 'unknown',
      });
    });

    it('should handle null/undefined errors', () => {
      const result = handleGitHubAPIError(null);

      expect(result).toEqual({
        error: 'Unknown error occurred',
        type: 'unknown',
      });
    });
  });

  it('should use default message when RequestError has empty message for unknown status', () => {
    const error = new RequestError('', 418, {
      response: {
        status: 418,
        headers: {},
        data: {},
        url: 'https://api.github.com',
        retryCount: 0,
      },
      request: {
        method: 'GET',
        url: 'https://api.github.com',
        headers: {},
      },
    });

    const result = handleGitHubAPIError(error);

    expect(result.type).toBe('http');
    expect(result.status).toBe(418);
    expect(typeof result.error).toBe('string');
    expect(result.error.length).toBeGreaterThan(0);
  });

  it('should use fallback suggestion when token already has all required scopes', () => {
    const error = new RequestError('Forbidden', 403, {
      response: {
        status: 403,
        headers: {
          'x-accepted-oauth-scopes': 'repo',
          'x-oauth-scopes': 'repo, read:user, read:org',
          'x-ratelimit-remaining': '10',
        },
        data: {},
        url: 'https://api.github.com',
        retryCount: 0,
      },
      request: {
        method: 'GET',
        url: 'https://api.github.com',
        headers: {},
      },
    });

    const result = handleGitHubAPIError(error);

    expect(result.status).toBe(403);
    expect(result.type).toBe('http');
    expect(typeof result.scopesSuggestion).toBe('string');
  });
});

describe('isNoResultsSearchError', () => {
  it('treats a 422 "users cannot be searched" validation error as no-results', () => {
    const error = makeSearch422([
      {
        message:
          'The listed users cannot be searched either because the users do not exist or you do not have permission to view the users.',
        resource: 'Search',
        field: 'q',
      },
    ]);
    expect(isNoResultsSearchError(error)).toBe(true);
  });

  it('treats a 422 "does not exist" validation error as no-results', () => {
    const error = makeSearch422([
      { message: 'The organization does not exist.', field: 'q' },
    ]);
    expect(isNoResultsSearchError(error)).toBe(true);
  });

  it('does NOT treat a generic/malformed-query 422 as no-results', () => {
    const error = makeSearch422([
      {
        message:
          'The search contains only logical operators (AND, OR, NOT) without any search terms.',
        resource: 'Search',
        field: 'q',
        code: 'invalid',
      },
    ]);
    expect(isNoResultsSearchError(error)).toBe(false);
  });

  it('does NOT treat a 422 with no errors[] payload as no-results', () => {
    const error = makeSearch422([]);
    expect(isNoResultsSearchError(error)).toBe(false);
  });

  it('does NOT treat non-422 statuses as no-results', () => {
    const error = new RequestError('Not Found', 404, {
      response: {
        status: 404,
        headers: {},
        data: {
          errors: [{ message: 'users do not exist' }],
        },
        url: 'https://api.github.com',
        retryCount: 0,
      },
      request: { method: 'GET', url: 'https://api.github.com', headers: {} },
    });
    expect(isNoResultsSearchError(error)).toBe(false);
  });

  it('does NOT treat non-RequestError values as no-results', () => {
    expect(isNoResultsSearchError(new Error('boom'))).toBe(false);
    expect(isNoResultsSearchError('boom')).toBe(false);
    expect(isNoResultsSearchError(undefined)).toBe(false);
  });

  it('still classifies the nonexistent-entity 422 as a normal error via handleGitHubAPIError (propagation intact)', () => {
    const error = makeSearch422([
      { message: 'The listed users cannot be searched...', field: 'q' },
    ]);
    const apiError = handleGitHubAPIError(error);
    expect(apiError.status).toBe(422);
    expect(apiError.type).toBe('http');
  });
});
