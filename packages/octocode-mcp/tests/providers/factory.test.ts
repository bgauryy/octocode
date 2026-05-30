/**
 * Provider Factory Tests
 *
 * Covers getProvider, clearProviderCache, and initializeProviders including
 * cache hit/miss, key normalization, TTL expiry, eviction, and error paths.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const { constructorSpy, MockGitHubProvider } = vi.hoisted(() => {
  const spy = vi.fn();
  class MockProvider {
    readonly type = 'github' as const;
    config?: unknown;
    constructor(config?: unknown) {
      this.config = config;
      spy(config);
    }
  }
  return { constructorSpy: spy, MockGitHubProvider: MockProvider };
});

vi.mock('../../src/providers/github/GitHubProvider.js', () => ({
  GitHubProvider: MockGitHubProvider,
}));

import {
  getProvider,
  clearProviderCache,
  initializeProviders,
} from '../../src/providers/factory.js';

describe('provider factory', () => {
  beforeEach(() => {
    clearProviderCache();
    constructorSpy.mockClear();
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    clearProviderCache();
  });

  describe('getProvider - type handling', () => {
    it('returns a GitHubProvider for the default type', () => {
      const provider = getProvider();
      expect(provider).toBeInstanceOf(MockGitHubProvider);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('returns a GitHubProvider for explicit "github"', () => {
      const provider = getProvider('github');
      expect(provider).toBeInstanceOf(MockGitHubProvider);
    });

    it('passes config merged with the type to the constructor', () => {
      getProvider('github', { type: 'github', token: 'abc' });
      expect(constructorSpy).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'github', token: 'abc' })
      );
    });

    it('throws for an unknown provider type', () => {
      expect(() =>
        // @ts-expect-error - exercising the runtime guard with an invalid type
        getProvider('gitlab')
      ).toThrow(/Unknown provider type: 'gitlab'/);
    });

    it('throws for an empty provider type', () => {
      expect(() =>
        // @ts-expect-error - exercising the runtime guard with an invalid type
        getProvider('')
      ).toThrow(/Only 'github' is supported/);
    });
  });

  describe('getProvider - caching', () => {
    it('returns the cached instance on a second call with no config', () => {
      const first = getProvider('github');
      const second = getProvider('github');
      expect(second).toBe(first);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('returns the cached instance for the same token', () => {
      const first = getProvider('github', { type: 'github', token: 't1' });
      const second = getProvider('github', { type: 'github', token: 't1' });
      expect(second).toBe(first);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('creates separate instances for different tokens', () => {
      const a = getProvider('github', { type: 'github', token: 't1' });
      const b = getProvider('github', { type: 'github', token: 't2' });
      expect(a).not.toBe(b);
      expect(constructorSpy).toHaveBeenCalledTimes(2);
    });

    it('uses authInfo.token when token is absent', () => {
      const a = getProvider('github', {
        type: 'github',
        authInfo: { token: 'auth-tok' } as never,
      });
      // Same authInfo token => same cache key => cached.
      const b = getProvider('github', {
        type: 'github',
        authInfo: { token: 'auth-tok' } as never,
      });
      expect(b).toBe(a);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('treats no token and missing config as the default key', () => {
      const a = getProvider('github');
      const b = getProvider('github', { type: 'github' });
      expect(b).toBe(a);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });
  });

  describe('getProvider - baseUrl normalization', () => {
    it('normalizes trailing slashes and host casing to one cache key', () => {
      const a = getProvider('github', {
        type: 'github',
        baseUrl: 'https://GHE.Example.com/api/v3/',
        token: 't',
      });
      const b = getProvider('github', {
        type: 'github',
        baseUrl: 'https://ghe.example.com/api/v3',
        token: 't',
      });
      expect(b).toBe(a);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('preserves an explicit port in the cache key', () => {
      const a = getProvider('github', {
        type: 'github',
        baseUrl: 'https://ghe.example.com:8443/api',
        token: 't',
      });
      const b = getProvider('github', {
        type: 'github',
        baseUrl: 'https://ghe.example.com/api',
        token: 't',
      });
      expect(b).not.toBe(a);
      expect(constructorSpy).toHaveBeenCalledTimes(2);
    });

    it('falls back to trimming trailing slashes for an invalid url', () => {
      const a = getProvider('github', {
        type: 'github',
        baseUrl: 'not-a-url///',
        token: 't',
      });
      const b = getProvider('github', {
        type: 'github',
        baseUrl: 'not-a-url',
        token: 't',
      });
      expect(b).toBe(a);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('creates distinct instances for different baseUrls', () => {
      const a = getProvider('github', {
        type: 'github',
        baseUrl: 'https://one.example.com',
        token: 't',
      });
      const b = getProvider('github', {
        type: 'github',
        baseUrl: 'https://two.example.com',
        token: 't',
      });
      expect(a).not.toBe(b);
    });
  });

  describe('getProvider - TTL expiry', () => {
    it('recreates the provider after the cache entry expires', () => {
      vi.useFakeTimers();
      const first = getProvider('github', { type: 'github', token: 't' });
      // Advance beyond the 1 hour TTL.
      vi.advanceTimersByTime(60 * 60 * 1000 + 1);
      const second = getProvider('github', { type: 'github', token: 't' });
      expect(second).not.toBe(first);
      expect(constructorSpy).toHaveBeenCalledTimes(2);
    });

    it('refreshes lastAccessedAt on a cache hit within the TTL', () => {
      vi.useFakeTimers();
      const first = getProvider('github', { type: 'github', token: 't' });
      vi.advanceTimersByTime(30 * 60 * 1000);
      const second = getProvider('github', { type: 'github', token: 't' });
      expect(second).toBe(first);
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });
  });

  describe('getProvider - eviction', () => {
    it('evicts expired entries when over the instance limit', () => {
      vi.useFakeTimers();
      // Fill the cache with 20 distinct, soon-to-expire entries.
      for (let i = 0; i < 20; i++) {
        getProvider('github', { type: 'github', token: `expired-${i}` });
      }
      expect(constructorSpy).toHaveBeenCalledTimes(20);

      // Expire all of them.
      vi.advanceTimersByTime(60 * 60 * 1000 + 1);

      // 21st distinct token triggers eviction of expired entries first.
      const fresh = getProvider('github', { type: 'github', token: 'fresh' });
      expect(fresh).toBeInstanceOf(MockGitHubProvider);

      // The previously-expired tokens should have been removed, so requesting
      // one again constructs a new instance.
      constructorSpy.mockClear();
      getProvider('github', { type: 'github', token: 'expired-0' });
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });

    it('evicts least-recently-accessed entries when all are still valid', () => {
      vi.useFakeTimers();
      // Fill past the limit with valid entries, each with a distinct timestamp
      // so lastAccessedAt ordering is well-defined. Eviction only trims when
      // the cache size strictly exceeds the limit, so we seed 21 entries first.
      for (let i = 0; i < 21; i++) {
        getProvider('github', { type: 'github', token: `valid-${i}` });
        vi.advanceTimersByTime(1);
      }
      // One more distinct token forces LRU eviction of the oldest entry.
      getProvider('github', { type: 'github', token: 'overflow' });

      constructorSpy.mockClear();
      // valid-0 was the least-recently-accessed and should have been evicted.
      getProvider('github', { type: 'github', token: 'valid-0' });
      expect(constructorSpy).toHaveBeenCalledTimes(1);
    });
  });

  describe('clearProviderCache', () => {
    it('forces a new instance to be created after clearing', () => {
      const first = getProvider('github', { type: 'github', token: 't' });
      clearProviderCache();
      const second = getProvider('github', { type: 'github', token: 't' });
      expect(second).not.toBe(first);
      expect(constructorSpy).toHaveBeenCalledTimes(2);
    });
  });

  describe('initializeProviders', () => {
    it('returns an ok diagnostic when construction succeeds', async () => {
      const diagnostics = await initializeProviders();
      expect(diagnostics).toEqual([{ provider: 'github', ok: true }]);
    });

    it('returns a failed diagnostic and warns on construction error', async () => {
      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);
      constructorSpy.mockImplementationOnce(() => {
        throw new Error('boom');
      });

      const diagnostics = await initializeProviders();

      expect(diagnostics).toEqual([
        { provider: 'github', ok: false, error: 'boom' },
      ]);
      expect(stderrSpy).toHaveBeenCalledWith(
        expect.stringContaining('github provider failed to initialize: boom')
      );
      stderrSpy.mockRestore();
    });

    it('coerces a non-Error throw into a string message', async () => {
      const stderrSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);
      constructorSpy.mockImplementationOnce(() => {
        throw 'plain-failure';
      });

      const diagnostics = await initializeProviders();

      expect(diagnostics).toEqual([
        { provider: 'github', ok: false, error: 'plain-failure' },
      ]);
      stderrSpy.mockRestore();
    });
  });
});
