import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { executeWithErrorIsolation } from '../../src/utils/core/promise.js';

describe('promiseUtils - Concurrency and Timeout Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Concurrency Limiting', () => {
    it('should execute promises with concurrency limit', async () => {
      const executionOrder: number[] = [];
      const activeCount: number[] = [];
      let currentActive = 0;

      const createPromise = (id: number, delay: number) => async () => {
        currentActive++;
        activeCount.push(currentActive);
        executionOrder.push(id);

        await new Promise(resolve => setTimeout(resolve, delay));

        currentActive--;
        return id;
      };

      const promises = [
        createPromise(1, 50),
        createPromise(2, 50),
        createPromise(3, 50),
        createPromise(4, 50),
        createPromise(5, 50),
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 2,
        timeout: 5000,
      });

      expect(results).toHaveLength(5);
      expect(results.every(r => r.success)).toBe(true);

      expect(Math.max(...activeCount)).toBeLessThanOrEqual(2);
    });

    it('should handle concurrency = 1 (sequential execution)', async () => {
      const executionOrder: number[] = [];

      const createPromise = (id: number) => async () => {
        executionOrder.push(id);
        await new Promise(resolve => setTimeout(resolve, 10));
        return id;
      };

      const promises = [
        createPromise(1),
        createPromise(2),
        createPromise(3),
        createPromise(4),
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 1,
        timeout: 5000,
      });

      expect(results).toHaveLength(4);
      expect(results.every(r => r.success)).toBe(true);

      expect(executionOrder).toEqual([1, 2, 3, 4]);
    });

    it('should handle concurrency larger than array length', async () => {
      const promises = [async () => 1, async () => 2, async () => 3];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 10,
        timeout: 5000,
      });

      expect(results).toHaveLength(3);
      expect(results.every(r => r.success)).toBe(true);
    });

    it('should handle errors with concurrency limiting', async () => {
      const createPromise = (id: number, shouldFail: boolean) => async () => {
        await new Promise(resolve => setTimeout(resolve, 10));
        if (shouldFail) {
          throw new Error(`Promise ${id} failed`);
        }
        return id;
      };

      const promises = [
        createPromise(1, false),
        createPromise(2, true),
        createPromise(3, false),
        createPromise(4, true),
        createPromise(5, false),
      ];

      const errors: Array<{ error: Error; index: number }> = [];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 2,
        timeout: 5000,
        onError: (error, index) => {
          errors.push({ error, index });
        },
      });

      expect(results).toHaveLength(5);

      const successCount = results.filter(r => r.success).length;
      const failureCount = results.filter(r => !r.success).length;

      expect(successCount).toBe(3);
      expect(failureCount).toBe(2);
      expect(errors).toHaveLength(2);
    });

    it('should handle undefined promise functions in concurrency mode', async () => {
      const promises = [
        async () => 1,
        undefined as unknown as () => Promise<number>,
        async () => 3,
      ];

      const results = await executeWithErrorIsolation(promises, {
        concurrency: 2,
        timeout: 5000,
      });

      expect(results).toHaveLength(3);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[1]?.error?.message).toContain('not a function');
      expect(results[2]?.success).toBe(true);
    });
  });

  describe('Timeout Scenarios', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('should timeout slow promises', async () => {
      const slowPromise = async () => {
        await new Promise(resolve => setTimeout(resolve, 1000));
        return 'completed';
      };

      const fastPromise = async () => {
        await new Promise(resolve => setTimeout(resolve, 10));
        return 'fast';
      };

      const resultP = executeWithErrorIsolation([slowPromise, fastPromise], {
        timeout: 50,
      });
      await vi.advanceTimersByTimeAsync(10);
      await vi.advanceTimersByTimeAsync(50);
      const results = await resultP;

      expect(results).toHaveLength(2);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error?.message).toContain('timed out');
      expect(results[1]?.success).toBe(true);
      expect(results[1]?.data).toBe('fast');
    });

    it('should call onError when promise times out', async () => {
      const timeoutErrors: Array<{ error: Error; index: number }> = [];

      const slowPromise = async () => {
        await new Promise(resolve => setTimeout(resolve, 1000));
        return 'done';
      };

      const resultP = executeWithErrorIsolation([slowPromise], {
        timeout: 50,
        onError: (error, index) => {
          timeoutErrors.push({ error, index });
        },
      });
      await vi.advanceTimersByTimeAsync(50);
      await resultP;

      expect(timeoutErrors).toHaveLength(1);
      expect(timeoutErrors[0]?.error?.message).toContain('timed out');
      expect(timeoutErrors[0]?.index).toBe(0);
    });

    it('should properly cleanup timeouts for fast promises', async () => {
      const fastPromises = Array(10)
        .fill(null)
        .map((_, i) => async () => {
          await new Promise(resolve => setTimeout(resolve, 5));
          return i;
        });

      const resultP = executeWithErrorIsolation(fastPromises, {
        timeout: 5000,
      });
      await vi.advanceTimersByTimeAsync(5);
      const results = await resultP;

      expect(results).toHaveLength(10);
      expect(results.every(r => r.success)).toBe(true);
    });

    it('should handle mixed timeout and success scenarios', async () => {
      const promises = [
        async () => {
          await new Promise(resolve => setTimeout(resolve, 10));
          return 'fast-1';
        },
        async () => {
          await new Promise(resolve => setTimeout(resolve, 200));
          return 'slow-1';
        },
        async () => {
          await new Promise(resolve => setTimeout(resolve, 10));
          return 'fast-2';
        },
        async () => {
          await new Promise(resolve => setTimeout(resolve, 200));
          return 'slow-2';
        },
      ];

      const resultP = executeWithErrorIsolation(promises, {
        timeout: 50,
        concurrency: 2,
      });
      await vi.advanceTimersByTimeAsync(100);
      const results = await resultP;

      expect(results).toHaveLength(4);
      const successCount = results.filter(r => r.success).length;
      const timeoutCount = results.filter(r => !r.success).length;
      expect(successCount).toBe(2);
      expect(timeoutCount).toBe(2);
    });

    it('should handle all promises timing out', async () => {
      const promises = [
        async () => {
          await new Promise(resolve => setTimeout(resolve, 1000));
          return 'done-1';
        },
        async () => {
          await new Promise(resolve => setTimeout(resolve, 1000));
          return 'done-2';
        },
      ];

      const resultP = executeWithErrorIsolation(promises, { timeout: 50 });
      await vi.advanceTimersByTimeAsync(50);
      const results = await resultP;

      expect(results).toHaveLength(2);
      expect(results.every(r => !r.success)).toBe(true);
      expect(results.every(r => r.error?.message.includes('timed out'))).toBe(
        true
      );
    });
  });

  describe('Error Handler Edge Cases', () => {
    it('should handle errors thrown in onError callback', async () => {
      const errorPromise = async () => {
        throw new Error('Original error');
      };

      const onError = vi.fn().mockImplementation(() => {
        throw new Error('Error in error handler');
      });

      const results = await executeWithErrorIsolation([errorPromise], {
        timeout: 5000,
        onError,
      });

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error?.message).toBe('Original error');

      expect(onError).toHaveBeenCalled();
    });

    it('should preserve error index when onError is provided', async () => {
      const errors: Array<{ error: Error; index: number }> = [];

      const promises = [
        async () => 'success',
        async () => {
          throw new Error('Error at index 1');
        },
        async () => 'success',
        async () => {
          throw new Error('Error at index 3');
        },
      ];

      const results = await executeWithErrorIsolation(promises, {
        timeout: 5000,
        onError: (error, index) => {
          errors.push({ error, index });
        },
      });

      expect(results).toHaveLength(4);
      expect(errors).toHaveLength(2);

      expect(errors[0]?.index).toBe(1);
      expect(errors[0]?.error.message).toBe('Error at index 1');

      expect(errors[1]?.index).toBe(3);
      expect(errors[1]?.error.message).toBe('Error at index 3');
    });

    it('should convert non-Error objects to Error in onError', async () => {
      const receivedErrors: Error[] = [];

      const promises = [
        async () => {
          throw 'String error';
        },
        async () => {
          throw { message: 'Object error' };
        },
      ];

      await executeWithErrorIsolation(promises, {
        timeout: 5000,
        onError: error => {
          receivedErrors.push(error);
        },
      });

      expect(receivedErrors).toHaveLength(2);
      expect(receivedErrors[0]).toBeInstanceOf(Error);
      expect(receivedErrors[1]).toBeInstanceOf(Error);
    });
  });

  describe('Validation Edge Cases', () => {
    it('should throw on negative timeout with non-empty array', async () => {
      await expect(
        executeWithErrorIsolation([async () => 1], { timeout: -100 })
      ).rejects.toThrow('timeout must be positive');
    });

    it('should throw on zero timeout with non-empty array', async () => {
      await expect(
        executeWithErrorIsolation([async () => 1], { timeout: 0 })
      ).rejects.toThrow('timeout must be positive');
    });

    it('should throw on negative concurrency with non-empty array', async () => {
      await expect(
        executeWithErrorIsolation([async () => 1], { concurrency: -5 })
      ).rejects.toThrow('concurrency must be positive');
    });

    it('should throw on zero concurrency with non-empty array', async () => {
      await expect(
        executeWithErrorIsolation([async () => 1], { concurrency: 0 })
      ).rejects.toThrow('concurrency must be positive');
    });

    it('should throw on non-array input', async () => {
      await expect(
        executeWithErrorIsolation(
          'not an array' as unknown as Array<() => Promise<unknown>>,
          {}
        )
      ).rejects.toThrow('promises must be an array');
    });

    it('should throw on null input', async () => {
      await expect(
        executeWithErrorIsolation(
          null as unknown as Array<() => Promise<unknown>>,
          {}
        )
      ).rejects.toThrow('promises must be an array');
    });

    it('should throw on undefined input', async () => {
      await expect(
        executeWithErrorIsolation(
          undefined as unknown as Array<() => Promise<unknown>>,
          {}
        )
      ).rejects.toThrow('promises must be an array');
    });
  });

  describe('Complex Scenarios', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('should handle large number of promises with concurrency', async () => {
      const promises = Array(100)
        .fill(null)
        .map((_, i) => async () => {
          await new Promise(resolve => setTimeout(resolve, 1));
          return i;
        });

      const resultP = executeWithErrorIsolation(promises, {
        concurrency: 10,
        timeout: 5000,
      });
      await vi.advanceTimersByTimeAsync(50);
      const results = await resultP;

      expect(results).toHaveLength(100);
      expect(results.every(r => r.success)).toBe(true);

      const indices = results.map(r => r.index).sort((a, b) => a - b);
      expect(indices).toEqual(Array.from({ length: 100 }, (_, i) => i));
    });

    it('should handle promise that resolves after timeout starts but before it fires', async () => {
      const promises = [
        async () => {
          await new Promise(resolve => setTimeout(resolve, 20));
          return 'completed';
        },
      ];

      const resultP = executeWithErrorIsolation(promises, {
        timeout: 100,
      });
      await vi.advanceTimersByTimeAsync(20);
      const results = await resultP;

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(true);
      expect(results[0]?.data).toBe('completed');
    });

    it('should handle continueOnError option', async () => {
      const promises = [
        async () => 'success-1',
        async () => {
          throw new Error('Failure');
        },
        async () => 'success-2',
      ];

      const results = await executeWithErrorIsolation(promises, {
        timeout: 5000,
        continueOnError: true,
      });

      expect(results).toHaveLength(3);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
      expect(results[2]?.success).toBe(true);
    });

    it('should handle all promises failing', async () => {
      const promises = [
        async () => {
          throw new Error('Error 1');
        },
        async () => {
          throw new Error('Error 2');
        },
        async () => {
          throw new Error('Error 3');
        },
      ];

      const results = await executeWithErrorIsolation(promises, {
        timeout: 5000,
      });

      expect(results).toHaveLength(3);
      expect(results.every(r => !r.success)).toBe(true);
    });

    it('should handle all promises timing out', async () => {
      const promises = [
        async () => {
          await new Promise(resolve => setTimeout(resolve, 1000));
          return 'done-1';
        },
        async () => {
          await new Promise(resolve => setTimeout(resolve, 1000));
          return 'done-2';
        },
      ];

      const resultP = executeWithErrorIsolation(promises, { timeout: 50 });
      await vi.advanceTimersByTimeAsync(50);
      const results = await resultP;

      expect(results).toHaveLength(2);
      expect(results.every(r => !r.success)).toBe(true);
      expect(results.every(r => r.error?.message.includes('timed out'))).toBe(
        true
      );
    });
  });

  describe('Promise Rejection Handling', () => {
    it('should handle Promise.allSettled fulfilled status', async () => {
      const promises = [async () => 'success'];

      const results = await executeWithErrorIsolation(promises, {
        timeout: 5000,
      });

      expect(results[0]?.success).toBe(true);
      expect(results[0]?.data).toBe('success');
    });

    it('should handle Promise.allSettled rejected status (edge case)', async () => {
      const promises = [
        async () => {
          return Promise.reject(new Error('Unusual rejection'));
        },
      ];

      const results = await executeWithErrorIsolation(promises, {
        timeout: 5000,
      });

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
    });

    it('should handle non-Error rejection reasons', async () => {
      const promises = [
        async () => {
          throw 'String rejection';
        },
        async () => {
          throw 123;
        },
        async () => {
          throw null;
        },
      ];

      const results = await executeWithErrorIsolation(promises, {
        timeout: 5000,
      });

      expect(results).toHaveLength(3);
      expect(results.every(r => !r.success)).toBe(true);
      expect(results.every(r => r.error instanceof Error)).toBe(true);
    });
  });
});
