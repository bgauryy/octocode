import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { fetchWithRetries } from '../../../octocode-tools-core/src/utils/http/fetch.js';

describe('fetchWithRetries - Additional Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('Non-Error Rejection Handling', () => {
    it('should convert non-Error rejections to Error objects', async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValueOnce('String error')
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({ success: true }),
        });

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data', {
        maxRetries: 1,
      });

      await vi.runAllTimersAsync();
      const result = await promise;

      expect(result).toEqual({ success: true });
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should handle number rejection', async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValueOnce(42)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({ success: true }),
        });

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data', {
        maxRetries: 1,
      });

      await vi.runAllTimersAsync();
      await promise;

      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should handle object rejection', async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValueOnce({ message: 'Object error' })
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({ success: true }),
        });

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data', {
        maxRetries: 1,
      });

      await vi.runAllTimersAsync();
      await promise;

      expect(mockFetch).toHaveBeenCalledTimes(2);
    });
  });

  describe('Retry-After Header Edge Cases', () => {
    it('should handle invalid Retry-After header (non-numeric)', async () => {
      const mockData = { success: true };
      const headers = new Headers();
      headers.set('Retry-After', 'invalid');

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

      const promise = fetchWithRetries('https://example.com/data', {
        initialDelayMs: 1000,
      });

      await vi.runAllTimersAsync();
      const result = await promise;

      expect(result).toEqual(mockData);
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should handle Retry-After header with decimal value', async () => {
      const mockData = { success: true };
      const headers = new Headers();
      headers.set('Retry-After', '2.5');

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

      await vi.runAllTimersAsync();
      await promise;

      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should handle missing headers object', async () => {
      const mockData = { success: true };

      const mockFetch = vi
        .fn()
        .mockResolvedValueOnce({
          ok: false,
          status: 500,
          statusText: 'Internal Server Error',
          headers: undefined,
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
    });

    it('should handle headers without get method', async () => {
      const mockData = { success: true };

      const mockFetch = vi
        .fn()
        .mockResolvedValueOnce({
          ok: false,
          status: 500,
          statusText: 'Internal Server Error',
          headers: { 'Retry-After': '5' } as unknown as Headers,
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
    });
  });

  describe('Final Error After Retries', () => {
    it('should throw with last error message after all retries exhausted', async () => {
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

    it('should include error message in final throw', async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValue(new Error('Network connection failed'));

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data', {
        maxRetries: 1,
      });

      const assertion = expect(promise).rejects.toThrow(
        'Failed to fetch after 2 attempts'
      );

      await vi.runAllTimersAsync();
      await assertion;
    });

    it('should handle empty error message in last error', async () => {
      const errorWithoutMessage = new Error();
      errorWithoutMessage.message = '';

      const mockFetch = vi.fn().mockRejectedValue(errorWithoutMessage);

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data', {
        maxRetries: 0,
      });

      const assertion = expect(promise).rejects.toThrow(
        'Failed to fetch after 1 attempts'
      );

      await vi.runAllTimersAsync();
      await assertion;
    });
  });

  describe('Retryable Flag Handling', () => {
    it('should NOT retry when retryable is explicitly false', async () => {
      const error = new Error('Client error') as {
        message: string;
        retryable?: boolean;
      };
      error.retryable = false;

      const mockFetch = vi.fn().mockRejectedValue(error);

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data', {
        maxRetries: 3,
      });

      const assertion = expect(promise).rejects.toThrow('Client error');

      await vi.runAllTimersAsync();
      await assertion;

      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('should retry when retryable is undefined (default behavior)', async () => {
      const mockData = { success: true };
      const mockFetch = vi
        .fn()
        .mockRejectedValueOnce(new Error('Temporary error'))
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

    it('should retry when retryable is true', async () => {
      const mockData = { success: true };
      const error = new Error('Retryable error') as {
        message: string;
        retryable?: boolean;
      };
      error.retryable = true;

      const mockFetch = vi
        .fn()
        .mockRejectedValueOnce(error)
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
  });

  describe('Edge Cases with Different Status Codes', () => {
    it('should retry on 503 Service Unavailable', async () => {
      const mockData = { success: true };
      const mockFetch = vi
        .fn()
        .mockResolvedValueOnce({
          ok: false,
          status: 503,
          statusText: 'Service Unavailable',
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

    it('should NOT retry on 400 Bad Request', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 400,
        statusText: 'Bad Request',
        headers: new Headers(),
      });

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data');

      await expect(promise).rejects.toThrow(
        'Failed to fetch (400 Bad Request)'
      );

      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('should NOT retry on 403 Forbidden', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 403,
        statusText: 'Forbidden',
        headers: new Headers(),
      });

      (globalThis as { fetch?: typeof fetch }).fetch = mockFetch;

      const promise = fetchWithRetries('https://example.com/data');

      await expect(promise).rejects.toThrow('Failed to fetch (403 Forbidden)');

      expect(mockFetch).toHaveBeenCalledTimes(1);
    });
  });
});
