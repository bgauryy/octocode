import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { incrementRateLimits, updateSessionStats } from 'octocode-shared';
import { initializeSession, resetSessionManager } from '../../src/session.js';
import { fetchWithRetries } from '../../src/utils/http/fetch.js';

describe('fetchWithRetries rate limit stats', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    resetSessionManager();
    initializeSession();
  });

  afterEach(() => {
    vi.useRealTimers();
    resetSessionManager();
  });

  it('counts package registry 429 responses separately from rate limits', async () => {
    const headers = new Headers();
    headers.set('Retry-After', '7');

    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 429,
        statusText: 'Too Many Requests',
        headers,
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ ok: true }),
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://registry.npmjs.org/foo', {
      packageRegistry: 'npm',
    });

    await vi.advanceTimersByTimeAsync(7000);
    await expect(promise).resolves.toEqual({ ok: true });

    expect(updateSessionStats).toHaveBeenCalledWith({
      packageRegistryFailures: {
        npm: 1,
      },
    });
    expect(incrementRateLimits).not.toHaveBeenCalled();
  });
});
