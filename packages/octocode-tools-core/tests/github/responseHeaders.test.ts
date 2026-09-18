import { describe, expect, it, vi } from 'vitest';
import {
  normalizeResponseHeaders,
  rateLimitWarning,
  extractEtag,
} from '../../src/github/responseHeaders.js';

describe('normalizeResponseHeaders', () => {
  it('returns empty object for null/undefined/non-object', () => {
    expect(normalizeResponseHeaders(null)).toEqual({});
    expect(normalizeResponseHeaders(undefined)).toEqual({});
    expect(normalizeResponseHeaders('string')).toEqual({});
  });

  it('keeps string values', () => {
    expect(normalizeResponseHeaders({ 'content-type': 'application/json' }))
      .toEqual({ 'content-type': 'application/json' });
  });

  it('converts finite number values to string', () => {
    expect(normalizeResponseHeaders({ 'x-ratelimit-remaining': 42 }))
      .toEqual({ 'x-ratelimit-remaining': '42' });
  });

  it('drops non-string non-number values', () => {
    const result = normalizeResponseHeaders({ keep: 'yes', drop: true, also: null });
    expect(result).toEqual({ keep: 'yes' });
  });

  it('drops non-finite numbers', () => {
    const result = normalizeResponseHeaders({ bad: Infinity, also: NaN });
    expect(result).toEqual({});
  });
});

describe('rateLimitWarning', () => {
  it('returns undefined when headers are missing rate-limit info', () => {
    expect(rateLimitWarning({})).toBeUndefined();
  });

  it('returns undefined when quota is healthy', () => {
    // remaining=200 > max(3, 1000*0.15=150) → healthy
    const headers = {
      'x-ratelimit-remaining': '200',
      'x-ratelimit-limit': '1000',
    };
    expect(rateLimitWarning(headers)).toBeUndefined();
  });

  it('returns warning when remaining is low (< 15%)', () => {
    const headers = {
      'x-ratelimit-remaining': '10',
      'x-ratelimit-limit': '100',
    };
    const result = rateLimitWarning(headers);
    expect(result).toContain('10/100');
    expect(result).toContain('GitHub rate limit low');
  });

  it('includes reset time when x-ratelimit-reset is present', () => {
    const resetEpoch = Math.floor((Date.now() + 30_000) / 1000);
    const headers = {
      'x-ratelimit-remaining': '2',
      'x-ratelimit-limit': '100',
      'x-ratelimit-reset': String(resetEpoch),
    };
    const result = rateLimitWarning(headers);
    expect(result).toContain('resets in');
  });

  it('omits reset clause when x-ratelimit-reset is absent', () => {
    const headers = {
      'x-ratelimit-remaining': '2',
      'x-ratelimit-limit': '100',
    };
    const result = rateLimitWarning(headers);
    expect(result).not.toContain('resets in');
  });

  it('returns undefined when limit is 0', () => {
    const headers = {
      'x-ratelimit-remaining': '0',
      'x-ratelimit-limit': '0',
    };
    expect(rateLimitWarning(headers)).toBeUndefined();
  });
});

describe('extractEtag', () => {
  it('returns undefined when no etag header', () => {
    expect(extractEtag({})).toBeUndefined();
  });

  it('returns etag from lowercase key', () => {
    expect(extractEtag({ etag: '"abc123"' })).toBe('"abc123"');
  });

  it('returns etag from ETag key', () => {
    expect(extractEtag({ ETag: '"abc123"' })).toBe('"abc123"');
  });

  it('returns etag via case-insensitive fallback', () => {
    expect(extractEtag({ ETAG: '"abc123"' })).toBe('"abc123"');
  });

  it('returns undefined when etag is empty string', () => {
    expect(extractEtag({ etag: '' })).toBeUndefined();
  });
});
