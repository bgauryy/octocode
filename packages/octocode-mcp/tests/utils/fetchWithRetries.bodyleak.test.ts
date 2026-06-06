import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { fetchWithRetries } from '../../src/utils/http/fetch.js';

describe('fetchWithRetries - Response body cleanup', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should consume response body on non-retryable HTTP error to prevent socket leak', async () => {
    const bodyCancel = vi.fn().mockResolvedValue(undefined);
    const mockBody = { cancel: bodyCancel };

    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      headers: new Headers(),
      body: mockBody,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    await expect(
      fetchWithRetries('https://example.com/data')
    ).rejects.toThrow();

    expect(bodyCancel).toHaveBeenCalled();
  });

  it('should consume response body on retryable HTTP error before retry', async () => {
    const bodyCancelFirst = vi.fn().mockResolvedValue(undefined);
    const bodyCancelSecond = vi.fn().mockResolvedValue(undefined);

    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers(),
        body: { cancel: bodyCancelFirst },
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true }),
        body: { cancel: bodyCancelSecond },
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    await vi.runAllTimersAsync();
    await promise;

    expect(bodyCancelFirst).toHaveBeenCalled();
    expect(bodyCancelSecond).not.toHaveBeenCalled();
  });

  it('should handle response with null body gracefully', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      headers: new Headers(),
      body: null,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    await expect(fetchWithRetries('https://example.com/data')).rejects.toThrow(
      'Failed to fetch (404 Not Found)'
    );
  });

  it('should handle response with body that has no cancel method', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      headers: new Headers(),
      body: {},
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    await expect(fetchWithRetries('https://example.com/data')).rejects.toThrow(
      'Failed to fetch (404 Not Found)'
    );
  });

  it('should handle body.cancel() that rejects', async () => {
    const bodyCancel = vi.fn().mockRejectedValue(new Error('cancel failed'));
    const mockBody = { cancel: bodyCancel };

    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 403,
      statusText: 'Forbidden',
      headers: new Headers(),
      body: mockBody,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    await expect(fetchWithRetries('https://example.com/data')).rejects.toThrow(
      'Failed to fetch (403 Forbidden)'
    );

    expect(bodyCancel).toHaveBeenCalled();
  });

  it('should handle response with undefined body', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      headers: new Headers(),
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    await expect(fetchWithRetries('https://example.com/data')).rejects.toThrow(
      'Failed to fetch (404 Not Found)'
    );
  });
});
