export interface PromiseResult<T> {
  success: boolean;
  data?: T;
  error?: Error;
  index: number;
}

export interface PromiseExecutionOptions {
  timeout?: number;
  continueOnError?: boolean;
  concurrency?: number;
  onError?: (error: Error, index: number) => void;
}
