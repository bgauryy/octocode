import type { PromiseResult, PromiseExecutionOptions } from '../../types.js';
import {
  VALIDATION_ERRORS,
  PROMISE_ERRORS,
} from '../../errors/domainErrors.js';
import { logSessionError } from '../../session.js';
import { ignoreBestEffortFailure } from './bestEffort.js';

export async function executeWithErrorIsolation<T>(
  promises: Array<() => Promise<T>>,
  options: PromiseExecutionOptions = {}
): Promise<PromiseResult<T>[]> {
  if (!Array.isArray(promises)) {
    logSessionError(
      'promiseUtils',
      VALIDATION_ERRORS.PROMISES_NOT_ARRAY.code
    ).catch(ignoreBestEffortFailure('promise utility session logging'));
    throw new Error(VALIDATION_ERRORS.PROMISES_NOT_ARRAY.message);
  }

  if (promises.length === 0) {
    return [];
  }

  const { timeout = 30000, concurrency = promises.length, onError } = options;

  if (timeout <= 0) {
    logSessionError(
      'promiseUtils',
      VALIDATION_ERRORS.TIMEOUT_NOT_POSITIVE.code
    ).catch(ignoreBestEffortFailure('promise utility session logging'));
    throw new Error(VALIDATION_ERRORS.TIMEOUT_NOT_POSITIVE.message);
  }
  if (concurrency <= 0) {
    logSessionError(
      'promiseUtils',
      VALIDATION_ERRORS.CONCURRENCY_NOT_POSITIVE.code
    ).catch(ignoreBestEffortFailure('promise utility session logging'));
    throw new Error(VALIDATION_ERRORS.CONCURRENCY_NOT_POSITIVE.message);
  }

  const validPromises = promises.map((promiseFn, index) =>
    typeof promiseFn === 'function'
      ? promiseFn
      : () => {
          logSessionError(
            'promiseUtils',
            PROMISE_ERRORS.NOT_A_FUNCTION.code
          ).catch(ignoreBestEffortFailure('promise utility session logging'));
          return Promise.reject(
            new Error(PROMISE_ERRORS.NOT_A_FUNCTION.message(index))
          );
        }
  );

  if (concurrency < validPromises.length) {
    return executeWithConcurrencyLimit(
      validPromises,
      concurrency,
      timeout,
      onError
    );
  }

  const isolatedPromises = validPromises.map((promiseFn, index) =>
    createIsolatedPromise(promiseFn, index, timeout, onError)
  );

  const results = await Promise.allSettled(isolatedPromises);

  return results.map((result, index) => {
    if (result.status === 'fulfilled') {
      return result.value;
    } else {
      return {
        success: false,
        error:
          result.reason instanceof Error
            ? result.reason
            : new Error(String(result.reason)),
        index,
      };
    }
  });
}

async function createIsolatedPromise<T>(
  promiseFn: () => Promise<T>,
  index: number,
  timeout: number,
  onError?: (error: Error, index: number) => void
): Promise<PromiseResult<T>> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;

  /* c8 ignore start - cleanup's if branch always taken: timeoutId set synchronously */
  const cleanup = () => {
    if (timeoutId !== undefined) {
      clearTimeout(timeoutId);
      timeoutId = undefined;
    }
  };
  /* c8 ignore stop */

  try {
    const timeoutPromise = new Promise<never>((_, reject) => {
      timeoutId = setTimeout(() => {
        logSessionError('promiseUtils', PROMISE_ERRORS.TIMEOUT.code).catch(
          () => {}
        );
        reject(new Error(PROMISE_ERRORS.TIMEOUT.message(index, timeout)));
      }, timeout);
    });

    const data = await Promise.race([promiseFn(), timeoutPromise]);

    cleanup();

    return {
      success: true,
      data,
      index,
    };
  } catch (error) {
    cleanup();

    const errorObj = error instanceof Error ? error : new Error(String(error));

    if (onError) {
      try {
        onError(errorObj, index);
      } catch (handlerError) {
        void handlerError;
      }
    }

    return {
      success: false,
      error: errorObj,
      index,
    };
  }
}

async function executeWithConcurrencyLimit<T>(
  promiseFns: Array<() => Promise<T>>,
  concurrency: number,
  timeout: number,
  onError?: (error: Error, index: number) => void
): Promise<PromiseResult<T>[]> {
  const results: PromiseResult<T>[] = new Array(promiseFns.length);
  let nextIndex = 0;

  const executeNext = async (): Promise<void> => {
    while (nextIndex < promiseFns.length) {
      const currentIndex = nextIndex++;
      const promiseFn = promiseFns[currentIndex];

      if (!promiseFn) {
        logSessionError(
          'promiseUtils',
          PROMISE_ERRORS.FUNCTION_UNDEFINED.code
        ).catch(ignoreBestEffortFailure('promise utility session logging'));
        results[currentIndex] = {
          success: false,
          error: new Error(PROMISE_ERRORS.FUNCTION_UNDEFINED.message),
          index: currentIndex,
        };
        continue;
      }

      try {
        const result = await createIsolatedPromise(
          promiseFn,
          currentIndex,
          timeout,
          onError
        );
        results[currentIndex] = result;
        /* c8 ignore start - defensive: createIsolatedPromise always catches internally */
      } catch (error) {
        results[currentIndex] = {
          success: false,
          error: error instanceof Error ? error : new Error(String(error)),
          index: currentIndex,
        };
      }
      /* c8 ignore stop */
    }
  };

  const workers: Promise<void>[] = [];
  for (let i = 0; i < Math.min(concurrency, promiseFns.length); i++) {
    workers.push(executeNext());
  }

  await Promise.all(workers);

  return results;
}
