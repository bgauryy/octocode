import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  isCircuitOpen,
  resetCircuitBreaker,
} from '../../src/utils/http/circuitBreaker.js';
import { fetchWithRetries } from '../../src/utils/http/fetch.js';

const originalFetch = globalThis.fetch;

describe('fetchWithRetries', () => {
  beforeEach(() => {
    resetCircuitBreaker();
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
    resetCircuitBreaker();
  });

  it('adds version and caller headers and returns parsed JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      })
    );
    globalThis.fetch = fetchMock;

    await expect(
      fetchWithRetries('https://registry.example.test/search?q=zod', {
        includeVersion: true,
        method: 'POST',
        headers: { Authorization: 'Bearer token' },
      })
    ).resolves.toEqual({ ok: true });

    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringMatching(
        /^https:\/\/registry\.example\.test\/search\?q=zod&version=/
      ),
      expect.objectContaining({
        method: 'POST',
        headers: expect.objectContaining({
          Authorization: 'Bearer token',
          'User-Agent': expect.stringMatching(/^Octocode-MCP\//),
        }),
      })
    );
  });

  it('returns null for a successful empty response', async () => {
    globalThis.fetch = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));

    await expect(
      fetchWithRetries('https://api.example.test/no-content')
    ).resolves.toBeNull();
  });

  it('retries retryable failures and then succeeds', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response('busy', { status: 503 }))
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ recovered: true }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      );
    globalThis.fetch = fetchMock;

    await expect(
      fetchWithRetries('https://api.example.test/retry', {
        maxRetries: 1,
        initialDelayMs: 0,
        maxDelayMs: 0,
      })
    ).resolves.toEqual({ recovered: true });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('does not retry a non-retryable HTTP response', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response('bad request', { status: 400 }));
    globalThis.fetch = fetchMock;

    await expect(
      fetchWithRetries('https://api.example.test/bad', {
        maxRetries: 3,
        initialDelayMs: 0,
      })
    ).rejects.toThrow(/400/);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('honors an already-aborted signal without issuing a request', async () => {
    const controller = new AbortController();
    controller.abort();
    const fetchMock = vi.fn();
    globalThis.fetch = fetchMock;

    await expect(
      fetchWithRetries('https://api.example.test/aborted', {
        signal: controller.signal,
      })
    ).rejects.toThrow('Request aborted');
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('opens the host circuit after repeated exhausted failures', async () => {
    const url = 'https://api.example.test/failing';
    globalThis.fetch = vi
      .fn()
      .mockRejectedValue(Object.assign(new Error('offline'), { retryable: true }));

    for (let attempt = 0; attempt < 5; attempt++) {
      await expect(
        fetchWithRetries(url, { maxRetries: 0 })
      ).rejects.toThrow(/failed to fetch after 1 attempts/i);
    }

    expect(isCircuitOpen(url)).toBe(true);
    await expect(fetchWithRetries(url, { maxRetries: 0 })).rejects.toThrow(
      /circuit open/i
    );
  });
});
