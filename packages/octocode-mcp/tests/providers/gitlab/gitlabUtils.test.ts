import { beforeEach, describe, expect, it, vi } from 'vitest';

const mockLogRateLimit = vi.hoisted(() => vi.fn());
vi.mock('../../../src/session.js', () => ({
  logRateLimit: mockLogRateLimit,
}));

import { handleGitLabAPIResponse } from '../../../src/providers/gitlab/utils.js';

describe('GitLab provider utils', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('handleGitLabAPIResponse', () => {
    it('should transform successful API responses', () => {
      const result = handleGitLabAPIResponse(
        {
          data: { value: 2 },
          status: 200,
        },
        'gitlab',
        data => ({ doubled: data.value * 2 })
      );

      expect(result).toEqual({
        data: { doubled: 4 },
        status: 200,
        provider: 'gitlab',
        rawResponseChars: 11,
      });
    });

    it('should preserve API errors and extract rate limit info', () => {
      const result = handleGitLabAPIResponse(
        {
          error: 'Rate limited',
          status: 429,
          type: 'http',
          hints: ['Retry later'],
          rateLimitRemaining: 0,
          rateLimitReset: 123456,
          retryAfter: 60,
        },
        'gitlab',
        data => data
      );

      expect(result).toEqual({
        error: 'Rate limited',
        status: 429,
        provider: 'gitlab',
        hints: ['Retry later'],
        rateLimit: {
          remaining: 0,
          reset: 123456,
          retryAfter: 60,
        },
      });
      expect(mockLogRateLimit).toHaveBeenCalledWith(
        expect.objectContaining({
          provider: 'gitlab',
          retry_after_seconds: 60,
          rate_limit_remaining: 0,
        })
      );
    });

    it('should string-coerce errors when requested', () => {
      const error = {
        toString: () => 'Object error',
      };

      const result = handleGitLabAPIResponse(
        {
          error: error as unknown as string,
          status: 500,
          type: 'unknown',
        },
        'gitlab',
        data => data,
        { stringifyError: true }
      );

      expect(result).toEqual({
        error: 'Object error',
        status: 500,
        provider: 'gitlab',
        hints: undefined,
        rateLimit: undefined,
      });
    });

    it('should return a no-data error when API data is missing', () => {
      const result = handleGitLabAPIResponse(
        {
          status: 200,
          data: null,
        } as never,
        'gitlab',
        data => data
      );

      expect(result).toEqual({
        error: 'No data returned from GitLab API',
        status: 500,
        provider: 'gitlab',
      });
    });
  });
});
