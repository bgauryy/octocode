import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { executeWithErrorIsolation } from '../../../octocode-tools-core/src/utils/core/promise.js';
import type { PromiseExecutionOptions } from '../../../octocode-tools-core/src/types/promise.js';
import { VALIDATION_ERRORS } from '../../../octocode-tools-core/src/errors/domainErrors.js';

describe('promiseUtils', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.clearAllTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('executeWithErrorIsolation', () => {
    it('should handle empty promise array', async () => {
      const result = await executeWithErrorIsolation([]);
      expect(result).toEqual([]);
    });

    it('should execute all promises successfully', async () => {
      const promises = [
        () => Promise.resolve('result1'),
        () => Promise.resolve('result2'),
        () => Promise.resolve('result3'),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(3);
      expect(results[0]).toEqual({
        success: true,
        data: 'result1',
        index: 0,
      });
      expect(results[1]).toEqual({
        success: true,
        data: 'result2',
        index: 1,
      });
      expect(results[2]).toEqual({
        success: true,
        data: 'result3',
        index: 2,
      });
    });

    it('should isolate errors and continue with other promises', async () => {
      const promises = [
        () => Promise.resolve('success1'),
        () => Promise.reject(new Error('error1')),
        () => Promise.resolve('success2'),
        () => Promise.reject(new Error('error2')),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(4);

      expect(results[0]).toEqual({
        success: true,
        data: 'success1',
        index: 0,
      });

      expect(results[1]).toEqual({
        success: false,
        error: expect.any(Error),
        index: 1,
      });
      expect(results[1]?.error?.message).toBe('error1');

      expect(results[2]).toEqual({
        success: true,
        data: 'success2',
        index: 2,
      });

      expect(results[3]).toEqual({
        success: false,
        error: expect.any(Error),
        index: 3,
      });
      expect(results[3]?.error?.message).toBe('error2');
    });

    it('should handle non-Error rejection reasons', async () => {
      const promises = [
        () => Promise.resolve('success'),
        () => Promise.reject('string error'),
        () => Promise.reject(123),
        () => Promise.reject({ custom: 'object' }),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(4);
      expect(results[0]?.success).toBe(true);

      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error).toBeInstanceOf(Error);
      expect(results[1]?.error?.message).toBe('string error');

      expect(results[2]?.success).toBe(false);
      expect(results[2]?.error).toBeInstanceOf(Error);
      expect(results[2]?.error?.message).toBe('123');

      expect(results[3]?.success).toBe(false);
      expect(results[3]?.error).toBeInstanceOf(Error);
      expect(results[3]?.error?.message).toContain('object');
    });

    it('should handle timeout correctly', async () => {
      vi.useFakeTimers();

      try {
        const promises = [
          () => new Promise(resolve => setTimeout(() => resolve('fast'), 500)),
          () => new Promise(resolve => setTimeout(() => resolve('slow'), 2000)),
        ];

        const options: PromiseExecutionOptions = { timeout: 1000 };
        const resultPromise = executeWithErrorIsolation(promises, options);

        vi.advanceTimersByTime(500);
        await vi.runAllTimersAsync();

        vi.advanceTimersByTime(600);
        await vi.runAllTimersAsync();

        const results = await resultPromise;

        expect(results).toHaveLength(2);
        expect(results[0]).toEqual({
          success: true,
          data: 'fast',
          index: 0,
        });
        expect(results[1]).toEqual({
          success: false,
          error: expect.any(Error),
          index: 1,
        });
        expect(results[1]?.error?.message).toContain('timed out after 1000ms');
      } finally {
        vi.useRealTimers();
      }
    });

    it('should call onError callback for failed promises', async () => {
      const onError = vi.fn();
      const promises = [
        () => Promise.resolve('success'),
        () => Promise.reject(new Error('test error')),
      ];

      const options: PromiseExecutionOptions = { onError };

      await executeWithErrorIsolation(promises, options);

      expect(onError).toHaveBeenCalledTimes(1);
      expect(onError).toHaveBeenCalledWith(expect.any(Error), 1);
      expect(onError.mock.calls[0]?.[0]?.message).toBe('test error');
    });

    it('should handle onError callback throwing error', async () => {
      const onError = vi.fn().mockImplementation(() => {
        throw new Error('callback error');
      });

      const promises = [() => Promise.reject(new Error('original error'))];

      const options: PromiseExecutionOptions = { onError };

      const results = await executeWithErrorIsolation(promises, options);

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error?.message).toBe('original error');
      expect(onError).toHaveBeenCalledTimes(1);
    });

    it('should handle non-Error rejection values', async () => {
      const promises = [
        () => Promise.reject('string error'),
        () => Promise.reject(null),
        () => Promise.reject(undefined),
        () => Promise.reject({ message: 'object error' }),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(4);
      results.forEach((result, index) => {
        expect(result.success).toBe(false);
        expect(result.error).toBeInstanceOf(Error);
        expect(result.index).toBe(index);
      });

      expect(results[0]?.error?.message).toBe('string error');
      expect(results[1]?.error?.message).toBe('null');
      expect(results[2]?.error?.message).toBe('undefined');
      expect(results[3]?.error?.message).toBe('[object Object]');
    });

    describe('Validation Errors', () => {
      it('should throw if promises argument is not an array', async () => {
        await expect(
          executeWithErrorIsolation(
            'not-array' as unknown as Parameters<
              typeof executeWithErrorIsolation
            >[0]
          )
        ).rejects.toThrow(VALIDATION_ERRORS.PROMISES_NOT_ARRAY.message);
      });

      it('should throw if timeout is not positive', async () => {
        const promises = [() => Promise.resolve(1)];
        await expect(
          executeWithErrorIsolation(promises, { timeout: 0 })
        ).rejects.toThrow(VALIDATION_ERRORS.TIMEOUT_NOT_POSITIVE.message);
      });

      it('should throw if concurrency is not positive', async () => {
        const promises = [() => Promise.resolve(1)];
        await expect(
          executeWithErrorIsolation(promises, { concurrency: 0 })
        ).rejects.toThrow(VALIDATION_ERRORS.CONCURRENCY_NOT_POSITIVE.message);
      });

      it('should handle non-function elements in promises array', async () => {
        const promises = [() => Promise.resolve(1), 'not-a-function'];

        const results = await executeWithErrorIsolation(
          promises as Parameters<typeof executeWithErrorIsolation>[0]
        );

        expect(results).toHaveLength(2);
        expect(results[0]?.success).toBe(true);
        expect(results[1]?.success).toBe(false);
        expect(results[1]?.error?.message).toContain(
          'Promise function at index 1 is not a function'
        );
      });
    });

    describe('Concurrency Limiting', () => {
      it('should handle concurrency limiting', async () => {
        vi.useFakeTimers();

        try {
          let activePromises = 0;
          let maxActivePromises = 0;

          const promises = Array.from(
            { length: 10 },
            (_, i) => () =>
              new Promise(resolve => {
                activePromises++;
                maxActivePromises = Math.max(maxActivePromises, activePromises);

                setTimeout(() => {
                  activePromises--;
                  resolve(`result${i}`);
                }, 100);
              })
          );

          const options: PromiseExecutionOptions = { concurrency: 3 };
          const resultPromise = executeWithErrorIsolation(promises, options);

          vi.advanceTimersByTime(1000);
          await vi.runAllTimersAsync();

          const results = await resultPromise;

          expect(results.length).toEqual(10);
          expect(maxActivePromises <= 3).toEqual(true);
          results.forEach((result, index) => {
            expect(result.success).toEqual(true);
            expect(result.data).toEqual(`result${index}`);
            expect(result.index).toEqual(index);
          });
        } finally {
          vi.useRealTimers();
        }
      });

      it('should handle concurrency with mixed success/failure', async () => {
        vi.useFakeTimers();

        try {
          const promises = Array.from(
            { length: 5 },
            (_, i) => () =>
              new Promise((resolve, reject) => {
                setTimeout(() => {
                  if (i % 2 === 0) {
                    resolve(`success${i}`);
                  } else {
                    reject(new Error(`error${i}`));
                  }
                }, 100);
              })
          );

          const options: PromiseExecutionOptions = { concurrency: 2 };
          const resultPromise = executeWithErrorIsolation(promises, options);

          vi.advanceTimersByTime(500);
          await vi.runAllTimersAsync();

          const results = await resultPromise;

          expect(results).toHaveLength(5);
          results.forEach((result, index) => {
            if (index % 2 === 0) {
              expect(result.success).toBe(true);
              expect(result.data).toBe(`success${index}`);
            } else {
              expect(result.success).toBe(false);
              expect(result.error?.message).toBe(`error${index}`);
            }
          });
        } finally {
          vi.useRealTimers();
        }
      });

      it('should handle undefined promise function in executeWithConcurrencyLimit', async () => {
        const promises = new Array(3);
        promises[0] = () => Promise.resolve(1);
        promises[2] = () => Promise.resolve(3);

        const results = await executeWithErrorIsolation(promises, {
          concurrency: 2,
        });

        expect(results).toHaveLength(3);
        expect(results[0]?.success).toBe(true);

        expect(results[1]?.success).toBe(false);
        expect(results[1]?.error?.message).toContain(
          PROMISE_ERRORS.FUNCTION_UNDEFINED.message
        );

        expect(results[2]?.success).toBe(true);
      });

      it('should handle timeout with concurrency limit', async () => {
        vi.useFakeTimers();

        try {
          const promises = Array.from(
            { length: 5 },
            (_, i) => () =>
              new Promise(resolve => {
                setTimeout(() => resolve(`result${i}`), i * 500);
              })
          );

          const options: PromiseExecutionOptions = {
            concurrency: 2,
            timeout: 1000,
          };
          const resultPromise = executeWithErrorIsolation(promises, options);

          vi.advanceTimersByTime(1500);
          await vi.runAllTimersAsync();

          const results = await resultPromise;

          expect(results).toHaveLength(5);
          expect(results[0]?.success).toBe(true);
          expect(results[4]?.success).toBe(false);
        } finally {
          vi.useRealTimers();
        }
      });
    });

    it('should handle default options', async () => {
      const promises = [() => Promise.resolve('test')];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(1);
      expect(results[0]).toEqual({
        success: true,
        data: 'test',
        index: 0,
      });
    });

    it('should handle non-Error rejection reasons from allSettled', async () => {
      const promises = [
        () => Promise.reject('string error'),
        () => Promise.reject(123),
        () => Promise.reject(null),
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(3);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error?.message).toBe('string error');
      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error?.message).toBe('123');
      expect(results[2]?.success).toBe(false);
      expect(results[2]?.error?.message).toBe('null');
    });

    it('should handle Promise.allSettled rejected status with non-Error reason', async () => {
      const promises = [
        () => Promise.resolve('success'),
        () => {
          throw 42;
        },
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(2);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error).toBeInstanceOf(Error);
    });

    it('should handle createIsolatedPromise catch block with non-Error', async () => {
      const promises = [
        () => {
          throw { custom: 'error object' };
        },
      ];

      const results = await executeWithErrorIsolation(promises);

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toBeInstanceOf(Error);
    });

    it('should handle errors in concurrency limit path', async () => {
      const promises = Array.from({ length: 10 }, (_, i) =>
        i % 2 === 0
          ? () => Promise.resolve(`success-${i}`)
          : () => Promise.reject(new Error(`error-${i}`))
      );

      const options: PromiseExecutionOptions = {
        concurrency: 2,
        timeout: 5000,
      };

      const results = await executeWithErrorIsolation(promises, options);

      expect(results).toHaveLength(10);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error?.message).toBe('error-1');
    });

    it('should handle undefined/null promise functions with concurrency', async () => {
      const promises: Array<(() => Promise<string>) | undefined> = [
        () => Promise.resolve('success-0'),
        undefined,
        () => Promise.resolve('success-2'),
        null as unknown as undefined,
        () => Promise.resolve('success-4'),
      ];

      const options: PromiseExecutionOptions = {
        concurrency: 2,
        timeout: 5000,
      };

      const results = await executeWithErrorIsolation(
        promises as Array<() => Promise<string>>,
        options
      );

      expect(results).toHaveLength(5);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error?.message).toContain('not a function');
      expect(results[2]?.success).toBe(true);
      expect(results[3]?.success).toBe(false);
      expect(results[4]?.success).toBe(true);
    });

    it('should handle onError callback being called', async () => {
      const errorCallback = vi.fn();
      const promises = [
        () => Promise.resolve('success'),
        () => Promise.reject(new Error('test error')),
        () => Promise.resolve('success2'),
      ];

      const options: PromiseExecutionOptions = {
        timeout: 5000,
        onError: errorCallback,
      };

      const results = await executeWithErrorIsolation(promises, options);

      expect(results).toHaveLength(3);
      expect(results[1]?.success).toBe(false);
      expect(errorCallback).toHaveBeenCalledTimes(1);
      expect(errorCallback).toHaveBeenCalledWith(expect.any(Error), 1);
    });
  });
});
