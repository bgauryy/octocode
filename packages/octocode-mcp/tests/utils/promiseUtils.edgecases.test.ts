import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { executeWithErrorIsolation } from '../../src/utils/core/promise.js';

vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('promiseUtils - Edge Cases and Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.clearAllTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('Promise.allSettled edge cases', () => {
    it('should handle rejected promises that are not errors', async () => {
      const promises = [
        () => Promise.resolve('success'),
        () => Promise.reject('string rejection'),
        () => Promise.reject(42),
        () => Promise.reject(null),
        () => Promise.reject(undefined),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(5);

      expect(results[0]).toEqual({
        success: true,
        data: 'success',
        index: 0,
      });

      expect(results[1]).toMatchObject({
        success: false,
        index: 1,
      });
      expect(results[1]?.error).toBeInstanceOf(Error);

      expect(results[2]).toMatchObject({
        success: false,
        index: 2,
      });
      expect(results[2]?.error).toBeInstanceOf(Error);

      expect(results[3]).toMatchObject({
        success: false,
        index: 3,
      });
      expect(results[3]?.error).toBeInstanceOf(Error);

      expect(results[4]).toMatchObject({
        success: false,
        index: 4,
      });
      expect(results[4]?.error).toBeInstanceOf(Error);
    });

    it('should handle promise rejection in allSettled path', async () => {
      const promises = [
        () => Promise.resolve('ok'),
        () => Promise.reject(new Error('failed')),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results[0]).toEqual({
        success: true,
        data: 'ok',
        index: 0,
      });

      expect(results[1]).toMatchObject({
        success: false,
        index: 1,
      });
      expect(results[1]?.error?.message).toBe('failed');
    });
  });

  describe('Concurrency limit edge cases', () => {
    it('should handle undefined promise function in concurrency path', async () => {
      const promises: Array<() => Promise<string>> = [
        () => Promise.resolve('first'),
        undefined as never,
        () => Promise.resolve('third'),
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 1,
      });

      expect(results).toHaveLength(3);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[2]?.success).toBe(true);
    });

    it('should handle error in createIsolatedPromise within concurrency limit', async () => {
      const promises = [
        () => Promise.resolve('first'),
        () => Promise.reject(new Error('second failed')),
        () => Promise.resolve('third'),
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 1,
      });

      expect(results).toHaveLength(3);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error?.message).toBe('second failed');
      expect(results[2]?.success).toBe(true);
    });

    it('should handle non-Error rejection in concurrency path', async () => {
      const promises = [
        () => Promise.resolve('ok'),
        () => Promise.reject('string error'),
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 1,
      });

      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error).toBeInstanceOf(Error);
      expect(results[1]?.error?.message).toBe('string error');
    });

    it('should handle catch block in executeWithConcurrencyLimit', async () => {
      const promises = [
        () => Promise.resolve('first'),
        () => {
          throw new Error('Synchronous error');
        },
        () => Promise.resolve('third'),
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 1,
      });

      expect(results).toHaveLength(3);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[2]?.success).toBe(true);
    });
  });

  describe('onError callback edge cases', () => {
    it('should call onError with Error when promise rejects', async () => {
      const onError = vi.fn();
      const error = new Error('test error');

      const promises = [
        () => Promise.reject(error),
        () => Promise.resolve('ok'),
      ];

      await executeWithErrorIsolation(promises, { onError });

      expect(onError).toHaveBeenCalledWith(error, 0);
    });

    it('should convert non-Error rejections to Error before calling onError', async () => {
      const onError = vi.fn();

      const promises = [() => Promise.reject('string error')];

      await executeWithErrorIsolation(promises, { onError });

      expect(onError).toHaveBeenCalledWith(expect.any(Error), 0);
      expect(onError.mock.calls[0]?.[0]?.message).toBe('string error');
    });

    it('should handle onError callback throwing an error', async () => {
      const onError = vi.fn(() => {
        throw new Error('onError failed');
      });

      const promises = [() => Promise.reject(new Error('original error'))];

      const results = await executeWithErrorIsolation(promises, { onError });

      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error?.message).toBe('original error');
      expect(onError).toHaveBeenCalled();
    });

    it('should call onError for each failed promise', async () => {
      const onError = vi.fn();

      const promises = [
        () => Promise.reject(new Error('error1')),
        () => Promise.resolve('ok'),
        () => Promise.reject(new Error('error2')),
      ];

      await executeWithErrorIsolation(promises, { onError });

      expect(onError).toHaveBeenCalledTimes(2);
      expect(onError).toHaveBeenCalledWith(expect.any(Error), 0);
      expect(onError).toHaveBeenCalledWith(expect.any(Error), 2);
    });

    it('should call onError in concurrency-limited execution', async () => {
      const onError = vi.fn();

      const promises = [
        () => Promise.reject(new Error('error1')),
        () => Promise.resolve('ok'),
        () => Promise.reject(new Error('error2')),
      ];

      await executeWithErrorIsolation(promises, { concurrency: 1, onError });

      expect(onError).toHaveBeenCalledTimes(2);
    });
  });

  describe('Timeout cleanup edge cases', () => {
    it('should cleanup timeout when promise succeeds', async () => {
      vi.useFakeTimers();

      const promises = [() => Promise.resolve('quick success')];

      const resultsPromise = executeWithErrorIsolation(promises, {
        timeout: 5000,
      });

      await vi.runAllTimersAsync();
      const results = await resultsPromise;

      expect(results[0]?.success).toBe(true);

      vi.useRealTimers();
    });

    it('should cleanup timeout when promise fails', async () => {
      vi.useFakeTimers();

      const promises = [() => Promise.reject(new Error('quick failure'))];

      const resultsPromise = executeWithErrorIsolation(promises, {
        timeout: 5000,
      });

      await vi.runAllTimersAsync();
      const results = await resultsPromise;

      expect(results[0]?.success).toBe(false);

      vi.useRealTimers();
    });
  });

  describe('Complex scenarios', () => {
    it('should handle mixed success, failure, and timeout with concurrency', async () => {
      vi.useFakeTimers();

      const promises = [
        () => Promise.resolve('fast'),
        () => Promise.reject(new Error('failed')),
        () => new Promise(resolve => setTimeout(() => resolve('slow'), 10000)),
        () => Promise.resolve('another fast'),
      ];

      const resultsPromise = executeWithErrorIsolation(promises, {
        timeout: 5000,
        concurrency: 2,
      });

      await vi.advanceTimersByTimeAsync(5001);

      const results = await resultsPromise;

      expect(results).toHaveLength(4);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[2]?.success).toBe(false);
      expect(results[3]?.success).toBe(true);

      vi.useRealTimers();
    });

    it('should handle all promises failing with different error types', async () => {
      const promises = [
        () => Promise.reject(new Error('Error object')),
        () => Promise.reject('String error'),
        () => Promise.reject(404),
        () => Promise.reject({ code: 'CUSTOM_ERROR' }),
        () => Promise.reject(null),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(5);

      results.forEach((result, index) => {
        expect(result.success).toBe(false);
        expect(result.error).toBeInstanceOf(Error);
        expect(result.index).toBe(index);
      });
    });

    it('should preserve promise execution order in results with concurrency', async () => {
      const executionOrder: number[] = [];

      const promises = [
        () => {
          executionOrder.push(0);
          return Promise.resolve('0');
        },
        () => {
          executionOrder.push(1);
          return Promise.resolve('1');
        },
        () => {
          executionOrder.push(2);
          return Promise.resolve('2');
        },
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 1,
      });

      expect(results).toHaveLength(3);
      expect(results[0]?.data).toBe('0');
      expect(results[1]?.data).toBe('1');
      expect(results[2]?.data).toBe('2');
      expect(executionOrder).toEqual([0, 1, 2]);
    });
  });

  describe('Error wrapping edge cases', () => {
    it('should wrap non-standard error types correctly', async () => {
      const promises = [
        () => Promise.reject({ message: 'object with message' }),
        () => Promise.reject([1, 2, 3]),
        () => Promise.reject(false),
        () => Promise.reject(Symbol('symbol error')),
      ];

      const results = await executeWithErrorIsolation(promises);

      results.forEach(result => {
        expect(result.success).toBe(false);
        expect(result.error).toBeInstanceOf(Error);
      });
    });
  });
});
