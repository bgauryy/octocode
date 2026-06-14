import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { fetchWithRetries } from '../../../octocode-tools-core/src/utils/http/fetch.js';

describe('fetchWithRetries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should successfully fetch on first attempt', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    await vi.runAllTimersAsync();
    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it('should retry on server error (5xx)', async () => {
    const mockData = { success: true };
    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers(),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    await vi.runAllTimersAsync();
    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it('should retry on rate limit (429)', async () => {
    const mockData = { success: true };
    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 429,
        statusText: 'Too Many Requests',
        headers: new Headers(),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    await vi.runAllTimersAsync();
    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it('should respect Retry-After header', async () => {
    const mockData = { success: true };
    const headers = new Headers();
    headers.set('Retry-After', '5');

    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 429,
        statusText: 'Too Many Requests',
        headers: headers,
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');

    await vi.advanceTimersByTimeAsync(4000);
    expect(mockFetch).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1001);
    expect(mockFetch).toHaveBeenCalledTimes(2);

    await promise;
  });

  it('should NOT retry on client error (4xx except 429)', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      headers: new Headers(),
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');

    await expect(promise).rejects.toThrow('Failed to fetch (404 Not Found)');

    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it('should use exponential backoff with jitter', async () => {
    const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0.5);

    const mockData = { success: true };
    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers(),
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers(),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      initialDelayMs: 1000,
    });

    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(1500);

    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(2500);

    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(3);

    randomSpy.mockRestore();
  });

  it('should add jitter in range [0, initialDelayMs)', async () => {
    const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0.99);

    const mockData = { success: true };
    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers(),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      initialDelayMs: 1000,
    });

    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(1989);
    expect(mockFetch).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    expect(mockFetch).toHaveBeenCalledTimes(2);

    await promise;

    randomSpy.mockRestore();
  });

  it('should throw error after max retries', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      headers: new Headers(),
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      maxRetries: 2,
    });

    const assertion = expect(promise).rejects.toThrow(
      'Failed to fetch after 3 attempts'
    );

    await vi.runAllTimersAsync();
    await assertion;

    expect(mockFetch).toHaveBeenCalledTimes(3);
  });

  it('should allow 0 retries (1 attempt)', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      headers: new Headers(),
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      maxRetries: 0,
    });

    await expect(promise).rejects.toThrow('Failed to fetch after 1 attempts');
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it('should include custom headers', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      headers: {
        'User-Agent': 'TestApp/1.0',
        Authorization: 'Bearer token',
      },
    });

    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith('https://example.com/data', {
      method: 'GET',
      headers: {
        'User-Agent': 'TestApp/1.0',
        Authorization: 'Bearer token',
      },
    });
  });

  it('should use custom method', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      method: 'POST',
    });

    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith('https://example.com/data', {
      method: 'POST',
      headers: expect.objectContaining({
        'User-Agent': expect.stringMatching(/^Octocode-MCP\//),
      }),
    });
  });

  it('should throw error if fetch is not available', async () => {
    const originalFetch = (globalThis as { fetch?: typeof fetch }).fetch;
    delete (globalThis as { fetch?: typeof fetch }).fetch;

    await expect(fetchWithRetries('https://example.com/data')).rejects.toThrow(
      'Global fetch is not available in this environment'
    );

    (globalThis as { fetch?: typeof fetch }).fetch = originalFetch;
  });

  it('should handle network errors with retries', async () => {
    const mockData = { success: true };
    const mockFetch = vi
      .fn()
      .mockRejectedValueOnce(new Error('Network error'))
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    await vi.runAllTimersAsync();
    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it('should return null for 204 No Content', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 204,
      statusText: 'No Content',
      json: async () => {
        throw new Error('Should not call json() on 204');
      },
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    const result = await promise;

    expect(result).toBeNull();
  });

  it('should handle Retry-After header with valid seconds', async () => {
    const mockData = { success: true };
    const headers = new Headers();
    headers.set('Retry-After', '2');

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
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      maxRetries: 1,
    });
    await vi.runAllTimersAsync();
    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it('should handle Retry-After header with invalid value (NaN)', async () => {
    const mockData = { success: true };
    const headers = new Headers();
    headers.set('Retry-After', 'invalid-number');

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
        json: async () => mockData,
      });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      maxRetries: 1,
      initialDelayMs: 100,
    });
    await vi.runAllTimersAsync();
    const result = await promise;

    expect(result).toEqual(mockData);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it('should handle lastError assignment in retry loop', async () => {
    const mockFetch = vi.fn().mockRejectedValue(new Error('Network error'));

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      maxRetries: 2,
      initialDelayMs: 10,
    });

    const testPromise = Promise.all([
      vi.runAllTimersAsync(),
      promise.catch(e => e),
    ]);

    const [, error] = await testPromise;

    expect(error).toBeInstanceOf(Error);
    expect((error as Error).message).toMatch(
      /Failed to fetch after 3 attempts/
    );
    expect(mockFetch).toHaveBeenCalledTimes(3);
  });

  it('should include default User-Agent header', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data');
    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith('https://example.com/data', {
      method: 'GET',
      headers: expect.objectContaining({
        'User-Agent': expect.stringMatching(/^Octocode-MCP\/\d+\.\d+\.\d+$/),
      }),
    });
  });

  it('should allow custom User-Agent to override default', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      headers: {
        'User-Agent': 'CustomApp/2.0',
      },
    });
    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith('https://example.com/data', {
      method: 'GET',
      headers: {
        'User-Agent': 'CustomApp/2.0',
      },
    });
  });

  it('should append version query parameter when includeVersion is true', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      includeVersion: true,
    });
    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringMatching(
        /^https:\/\/example\.com\/data\?version=\d+\.\d+\.\d+$/
      ),
      expect.any(Object)
    );
  });

  it('should append version with & when URL already has query params', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data?foo=bar', {
      includeVersion: true,
    });
    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringMatching(
        /^https:\/\/example\.com\/data\?foo=bar&version=\d+\.\d+\.\d+$/
      ),
      expect.any(Object)
    );
  });

  it('should not append version query parameter when includeVersion is false', async () => {
    const mockData = { success: true };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockData,
    });

    (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

    const promise = fetchWithRetries('https://example.com/data', {
      includeVersion: false,
    });
    await vi.runAllTimersAsync();
    await promise;

    expect(mockFetch).toHaveBeenCalledWith(
      'https://example.com/data',
      expect.any(Object)
    );
  });
});
