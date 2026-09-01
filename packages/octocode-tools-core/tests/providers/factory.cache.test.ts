import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  clearProviderCache,
  getProvider,
} from '../../src/providers/factory.js';

describe('provider instance cache', () => {
  afterEach(() => {
    vi.useRealTimers();
    clearProviderCache();
  });

  it('reuses normalized endpoint identities and isolates credentials', () => {
    const shared = getProvider('github', {
      baseUrl: 'https://GITHUB.example.test/api/v3/',
      token: 'token-a',
    });

    expect(
      getProvider('github', {
        baseUrl: 'https://github.example.test/api/v3',
        token: 'token-a',
      })
    ).toBe(shared);
    expect(
      getProvider('github', {
        baseUrl: 'https://github.example.test/api/v3',
        token: 'token-b',
      })
    ).not.toBe(shared);
  });

  it('expires providers after one hour and clears them on reset', () => {
    vi.useFakeTimers({ now: new Date('2026-08-30T00:00:00Z') });
    const beforeExpiry = getProvider('github');

    vi.advanceTimersByTime(60 * 60 * 1000);
    const afterExpiry = getProvider('github');
    expect(afterExpiry).not.toBe(beforeExpiry);

    clearProviderCache();
    expect(getProvider('github')).not.toBe(afterExpiry);
  });

  it('evicts the least-recently-used provider before inserting past the cap', () => {
    const oldest = getProvider('github', {
      baseUrl: 'https://github-0.example.test/api/v3',
    });

    for (let index = 1; index <= 20; index++) {
      getProvider('github', {
        baseUrl: `https://github-${index}.example.test/api/v3`,
      });
    }

    expect(
      getProvider('github', {
        baseUrl: 'https://github-0.example.test/api/v3',
      })
    ).not.toBe(oldest);
  });
});
