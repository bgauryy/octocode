/**
 * Promise execution & batching types used by `utils/core/promise.ts`.
 *
 * @module types/promise
 */

/** Result of a promise with error isolation. */
export interface PromiseResult<T> {
  success: boolean;
  data?: T;
  error?: Error;
  index: number;
}

/** Options for batch promise execution. */
export interface PromiseExecutionOptions {
  timeout?: number;
  continueOnError?: boolean;
  concurrency?: number;
  onError?: (error: Error, index: number) => void;
}
