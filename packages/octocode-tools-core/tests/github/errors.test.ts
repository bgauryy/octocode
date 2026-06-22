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
