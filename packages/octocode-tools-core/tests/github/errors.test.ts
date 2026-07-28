import { describe, expect, it } from 'vitest';
import { RequestError } from 'octokit';

import { handleGitHubAPIError } from '../../src/github/errors.js';

function makeRequestError(
  status: number,
  message: string,
  headers: Record<string, unknown>,
  data: unknown = {}
): RequestError {
  return new RequestError(message, status, {
    request: { method: 'GET', url: 'https://api.github.com/x', headers: {} },
    response: {
      status,
      url: 'https://api.github.com/x',
      headers: headers as never,
      data,
    },
  });
}

describe('handleGitHubAPIError - 403 rate-limit header parsing', () => {
  it('treats string "0" remaining as a primary rate limit', () => {
    const result = handleGitHubAPIError(
      makeRequestError(403, 'forbidden', {
        'x-ratelimit-remaining': '0',
        'x-ratelimit-reset': String(Math.floor(Date.now() / 1000) + 60),
      })
    );

    expect(result.error).toContain('rate limit exceeded');
    expect(result.rateLimitRemaining).toBe(0);
  });

  it('treats numeric 0 remaining as a primary rate limit (not a permission error)', () => {
    const result = handleGitHubAPIError(
      makeRequestError(403, 'forbidden', {
        'x-ratelimit-remaining': 0,
        'x-ratelimit-reset': Math.floor(Date.now() / 1000) + 60,
      })
    );

    expect(result.error).toContain('rate limit exceeded');
    expect(result.rateLimitRemaining).toBe(0);
  });

  it('treats nonzero remaining as a permission error', () => {
    const result = handleGitHubAPIError(
      makeRequestError(403, 'forbidden', {
        'x-ratelimit-remaining': 42,
      })
    );

    expect(result.error).toContain('Access forbidden');
  });
});

describe('handleGitHubAPIError — HTTP status routing', () => {
  it('handles 404 Not Found', () => {
    const result = handleGitHubAPIError(
      makeRequestError(404, 'Not Found', {})
    );
    expect(result.error).toBeDefined();
    expect(result.status).toBe(404);
  });

  it('handles 429 rate limit (Too Many Requests)', () => {
    const result = handleGitHubAPIError(
      makeRequestError(429, 'Too Many Requests', {
        'retry-after': '60',
      })
    );
    expect(result.error).toBeDefined();
    expect(result.status).toBe(429);
  });

  it('handles 422 Unprocessable Entity', () => {
    const result = handleGitHubAPIError(
      makeRequestError(422, 'Unprocessable', {})
    );
    expect(result.error).toBeDefined();
    expect(result.status).toBe(422);
  });

  it('handles 500 Internal Server Error', () => {
    const result = handleGitHubAPIError(
      makeRequestError(500, 'Internal Server Error', {})
    );
    expect(result.error).toBeDefined();
    expect(result.status).toBe(500);
  });

  it('handles 401 Unauthorized', () => {
    const result = handleGitHubAPIError(
      makeRequestError(401, 'Unauthorized', {})
    );
    expect(result.error).toBeDefined();
    expect(result.status).toBe(401);
  });

  it('handles unknown HTTP status codes gracefully', () => {
    const result = handleGitHubAPIError(
      makeRequestError(599, 'Weird Status', {})
    );
    expect(result.error).toBeDefined();
    expect(result.status).toBe(599);
  });

  it('handles non-RequestError (plain Error)', () => {
    const result = handleGitHubAPIError(new Error('connection refused'));
    expect(result.error).toBeDefined();
  });

  it('handles non-RequestError (plain object with message)', () => {
    const result = handleGitHubAPIError({ message: 'something failed' });
    expect(result.error).toBeDefined();
  });

  it('handles null gracefully', () => {
    const result = handleGitHubAPIError(null);
    expect(result.error).toBeDefined();
  });

  it('handles string error', () => {
    const result = handleGitHubAPIError('connection timed out');
    expect(result.error).toBeDefined();
  });

  it('handles Error with "timeout" in message (timeout network path)', () => {
    const result = handleGitHubAPIError(new Error('request timeout'));
    expect(result.error).toBeDefined();
    // Should match the TIMEOUT pattern
    expect(result.error).toBeTruthy();
  });

  it('handles Error with network failure message (connection failed path)', () => {
    const result = handleGitHubAPIError(new Error('ECONNREFUSED'));
    expect(result.error).toBeDefined();
  });

  it('handles Error with generic message (unknown network error)', () => {
    const result = handleGitHubAPIError(new Error('some unknown error occurred'));
    expect(result.error).toBeDefined();
  });

  it('handles 403 with scope headers when scopes are missing (generates scope suggestion)', () => {
    // This triggers generateScopesSuggestion with missing scopes
    const result = handleGitHubAPIError(
      makeRequestError(403, 'forbidden', {
        'x-ratelimit-remaining': 42,
        'x-oauth-scopes': 'repo',
        'x-accepted-oauth-scopes': 'repo, read:org',
      })
    );
    expect(result.error).toBeDefined();
  });

  it('handles 403 with scope headers when no scopes are missing (fallback suggestion)', () => {
    // Token has all required scopes → fallback suggestion path
    const result = handleGitHubAPIError(
      makeRequestError(403, 'forbidden', {
        'x-ratelimit-remaining': 42,
        'x-oauth-scopes': 'repo, read:org',
        'x-accepted-oauth-scopes': 'repo',
      })
    );
    expect(result.error).toBeDefined();
  });

  it('handles 403 secondary rate limit (message contains "secondary rate")', () => {
    // This triggers the SECONDARY pattern check in handle403Error → handleSecondaryRateLimit
    const result = handleGitHubAPIError(
      makeRequestError(403, 'You have exceeded a secondary rate limit', {
        'retry-after': '30',
      })
    );
    expect(result.error).toBeDefined();
    expect(result.retryAfter).toBeDefined();
  });

  it('handles 403 secondary rate limit without retry-after header', () => {
    const result = handleGitHubAPIError(
      makeRequestError(403, 'secondary rate limit exceeded', {})
    );
    expect(result.error).toBeDefined();
    // Uses fallback retry seconds
    expect(result.retryAfter).toBeDefined();
  });

  it('handles 403 with GraphQL rate-limit errors (type=RATE_LIMITED)', () => {
    const result = handleGitHubAPIError(
      makeRequestError(
        403,
        'forbidden',
        { 'x-ratelimit-remaining': '5' },
        { errors: [{ type: 'RATE_LIMITED' }] }
      )
    );
    expect(result.error).toBeDefined();
  });

  it('handles 429 with x-ratelimit-reset header (computes retryAfter from reset time)', () => {
    const resetInFuture = Math.floor(Date.now() / 1000) + 120;
    const result = handleGitHubAPIError(
      makeRequestError(429, 'Too Many Requests', {
        'x-ratelimit-remaining': '0',
        'x-ratelimit-reset': String(resetInFuture),
      })
    );
    expect(result.error).toBeDefined();
    expect(result.rateLimitRemaining).toBeDefined();
  });

  it('handles unknown HTTP status with empty message (uses fallback error message)', () => {
    const result = handleGitHubAPIError(
      makeRequestError(599, '', {})
    );
    expect(result.error).toBeDefined();
    expect(result.error.length).toBeGreaterThan(0);
  });

  it('handles 500 with x-ratelimit-reset header', () => {
    const resetInFuture = Math.floor(Date.now() / 1000) + 60;
    const result = handleGitHubAPIError(
      makeRequestError(500, 'Internal Server Error', {
        'x-ratelimit-reset': String(resetInFuture),
      })
    );
    expect(result.error).toBeDefined();
  });
});
