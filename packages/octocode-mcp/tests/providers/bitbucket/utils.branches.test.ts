import { beforeEach, describe, it, expect, vi } from 'vitest';

const mockLogRateLimit = vi.hoisted(() => vi.fn());
vi.mock('../../../src/session.js', () => ({
  logRateLimit: mockLogRateLimit,
}));

import {
  extractBitbucketRateLimit,
  handleBitbucketAPIResponse,
} from '../../../src/providers/bitbucket/utils.js';
import type { BitbucketAPIError } from '../../../src/bitbucket/types.js';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('extractBitbucketRateLimit - branch coverage', () => {
  it('should return undefined when no rate limit fields present', () => {
    const error = {} as BitbucketAPIError;
    expect(extractBitbucketRateLimit(error)).toBeUndefined();
  });

  it('should calculate reset from retryAfter when rateLimitReset is absent', () => {
    const before = Math.floor(Date.now() / 1000);
    const error = {
      retryAfter: 60,
    } as BitbucketAPIError;

    const result = extractBitbucketRateLimit(error);
    const after = Math.floor(Date.now() / 1000);

    expect(result).toBeDefined();
    expect(result!.remaining).toBe(0);
    expect(result!.reset).toBeGreaterThanOrEqual(before + 60);
    expect(result!.reset).toBeLessThanOrEqual(after + 60);
    expect(result!.retryAfter).toBe(60);
  });

  it('should use rateLimitReset directly when provided', () => {
    const error = {
      rateLimitReset: 1700000000,
      rateLimitRemaining: 5,
    } as BitbucketAPIError;

    const result = extractBitbucketRateLimit(error);

    expect(result).toBeDefined();
    expect(result!.reset).toBe(1700000000);
    expect(result!.remaining).toBe(5);
  });

  it('should return undefined when only rateLimitRemaining is set (no reset derivable)', () => {
    const error = {
      rateLimitRemaining: 10,
    } as BitbucketAPIError;

    const result = extractBitbucketRateLimit(error);
    expect(result).toBeUndefined();
  });

  it('should prefer rateLimitReset over retryAfter for reset value', () => {
    const error = {
      rateLimitReset: 1700000000,
      retryAfter: 30,
      rateLimitRemaining: 2,
    } as BitbucketAPIError;

    const result = extractBitbucketRateLimit(error);
    expect(result!.reset).toBe(1700000000);
  });
});

describe('handleBitbucketAPIResponse - branch coverage', () => {
  it('should handle error with string error field', () => {
    const result = handleBitbucketAPIResponse(
      { error: 'Something went wrong', status: 500 },
      data => data
    );
    expect(result.error).toBe('Something went wrong');
    expect(result.status).toBe(500);
    expect(result.provider).toBe('bitbucket');
  });

  it('should handle error with non-string error field', () => {
    const result = handleBitbucketAPIResponse(
      { error: { message: 'fail' } as unknown as string, status: 400 },
      data => data
    );
    expect(result.error).toBe('[object Object]');
  });

  it('should handle error with rate limit info and hints', () => {
    const result = handleBitbucketAPIResponse(
      {
        error: 'Rate limited',
        status: 429,
        rateLimitRemaining: 0,
        rateLimitReset: 1700000000,
        retryAfter: 60,
        hints: ['Try again later'],
      } as unknown as { error: string; status: number },
      data => data
    );
    expect(result.error).toBe('Rate limited');
    expect(result.rateLimit).toBeDefined();
    expect(result.hints).toEqual(['Try again later']);
    expect(mockLogRateLimit).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: 'bitbucket',
        retry_after_seconds: 60,
        rate_limit_remaining: 0,
      })
    );
  });

  it('should handle missing data field', () => {
    const result = handleBitbucketAPIResponse(
      { status: 200 } as { data?: unknown; status: number },
      data => data
    );
    expect(result.error).toBe('No data returned from Bitbucket API');
    expect(result.status).toBe(500);
  });

  it('should handle null data field', () => {
    const result = handleBitbucketAPIResponse(
      { data: null, status: 200 } as unknown as {
        data: unknown;
        status: number;
      },
      data => data
    );
    expect(result.error).toBe('No data returned from Bitbucket API');
  });

  it('should handle error without status (defaults to 500)', () => {
    const result = handleBitbucketAPIResponse(
      { error: 'Unknown error' } as { error: string; status?: number },
      data => data
    );
    expect(result.status).toBe(500);
  });
});
