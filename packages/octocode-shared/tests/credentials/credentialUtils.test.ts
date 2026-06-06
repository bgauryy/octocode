import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  normalizeHostname,
  isTokenExpired,
  isRefreshTokenExpired,
} from '../../src/credentials/credentialUtils.js';
import type { StoredCredentials } from '../../src/credentials/types.js';

function makeCredentials(
  overrides: Partial<StoredCredentials['token']> = {}
): StoredCredentials {
  return {
    hostname: 'github.com',
    username: 'testuser',
    gitProtocol: 'https',
    createdAt: '2025-01-01T00:00:00Z',
    updatedAt: '2025-01-01T00:00:00Z',
    token: {
      token: 'ghp_test',
      tokenType: 'oauth',
      ...overrides,
    },
  };
}

describe('normalizeHostname', () => {
  it('lowercases hostname', () => {
    expect(normalizeHostname('GitHub.COM')).toBe('github.com');
  });

  it('strips https:// prefix', () => {
    expect(normalizeHostname('https://github.com')).toBe('github.com');
  });

  it('strips http:// prefix', () => {
    expect(normalizeHostname('http://github.com')).toBe('github.com');
  });

  it('strips trailing slash', () => {
    expect(normalizeHostname('github.com/')).toBe('github.com');
  });

  it('strips https + trailing slash combined', () => {
    expect(normalizeHostname('https://GitHub.COM/')).toBe('github.com');
  });

  it('handles enterprise host', () => {
    expect(normalizeHostname('https://git.corp.example.com/')).toBe(
      'git.corp.example.com'
    );
  });

  it('preserves port numbers', () => {
    expect(normalizeHostname('https://github.example.com:8443/')).toBe(
      'github.example.com:8443'
    );
  });

  it('handles already-normalized hostname', () => {
    expect(normalizeHostname('github.com')).toBe('github.com');
  });

  it('handles empty string', () => {
    expect(normalizeHostname('')).toBe('');
  });
});

describe('isTokenExpired', () => {
  afterEach(() => vi.useRealTimers());

  it('returns false for non-expiring token (no expiresAt)', () => {
    expect(isTokenExpired(makeCredentials())).toBe(false);
  });

  it('returns true for token expired in the past', () => {
    const pastDate = new Date(Date.now() - 60 * 60 * 1000).toISOString();
    expect(isTokenExpired(makeCredentials({ expiresAt: pastDate }))).toBe(true);
  });

  it('returns true when less than 5 minutes remaining (boundary)', () => {
    const almostExpired = new Date(Date.now() + 4 * 60 * 1000).toISOString();
    expect(isTokenExpired(makeCredentials({ expiresAt: almostExpired }))).toBe(
      true
    );
  });

  it('returns false when more than 5 minutes remaining', () => {
    const futureDate = new Date(Date.now() + 10 * 60 * 1000).toISOString();
    expect(isTokenExpired(makeCredentials({ expiresAt: futureDate }))).toBe(
      false
    );
  });

  it('returns true for invalid date string (safety)', () => {
    expect(isTokenExpired(makeCredentials({ expiresAt: 'not-a-date' }))).toBe(
      true
    );
  });

  it('returns false for empty string expiresAt (falsy, treated as non-expiring)', () => {
    expect(isTokenExpired(makeCredentials({ expiresAt: '' }))).toBe(false);
  });
});

describe('isRefreshTokenExpired', () => {
  afterEach(() => vi.useRealTimers());

  it('returns false when no refreshTokenExpiresAt', () => {
    expect(isRefreshTokenExpired(makeCredentials())).toBe(false);
  });

  it('returns true when refresh token expired', () => {
    const pastDate = new Date(Date.now() - 1000).toISOString();
    expect(
      isRefreshTokenExpired(
        makeCredentials({ refreshTokenExpiresAt: pastDate })
      )
    ).toBe(true);
  });

  it('returns false when refresh token still valid', () => {
    const futureDate = new Date(Date.now() + 60 * 60 * 1000).toISOString();
    expect(
      isRefreshTokenExpired(
        makeCredentials({ refreshTokenExpiresAt: futureDate })
      )
    ).toBe(false);
  });

  it('returns true for invalid date string (safety)', () => {
    expect(
      isRefreshTokenExpired(
        makeCredentials({ refreshTokenExpiresAt: 'garbage' })
      )
    ).toBe(true);
  });
});
