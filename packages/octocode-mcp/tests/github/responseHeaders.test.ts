import { describe, it, expect } from 'vitest';
import { normalizeResponseHeaders } from '../../../octocode-tools-core/src/github/responseHeaders.js';

describe('normalizeResponseHeaders (validated narrowing of HTTP headers)', () => {
  it('passes string values through unchanged', () => {
    expect(
      normalizeResponseHeaders({ etag: 'abc', 'x-ratelimit-remaining': '42' })
    ).toEqual({ etag: 'abc', 'x-ratelimit-remaining': '42' });
  });

  it('stringifies finite numeric header values (octokit emits numbers)', () => {
    expect(normalizeResponseHeaders({ 'x-ratelimit-limit': 5000 })).toEqual({
      'x-ratelimit-limit': '5000',
    });
  });

  it('drops undefined / null / non-finite / non-scalar values', () => {
    expect(
      normalizeResponseHeaders({
        a: 'keep',
        b: undefined,
        c: null,
        d: NaN,
        e: { nested: true },
        f: ['x'],
      })
    ).toEqual({ a: 'keep' });
  });

  it('returns an empty record for non-object input', () => {
    expect(normalizeResponseHeaders(undefined)).toEqual({});
    expect(normalizeResponseHeaders(null)).toEqual({});
    expect(normalizeResponseHeaders('not headers')).toEqual({});
    expect(normalizeResponseHeaders(42)).toEqual({});
  });
});
