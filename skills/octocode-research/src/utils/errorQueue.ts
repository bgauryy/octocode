interface QueuedError {
  timestamp: Date;
  error: Error;
  context?: string;
}


class ErrorQueue {
  private errors: QueuedError[] = [];
  private readonly maxSize: number;

  constructor(maxSize = 100) {
    this.maxSize = maxSize;
  }

  
  push(error: unknown, context?: string): void {
    const normalizedError =
      error instanceof Error ? error : new Error(String(error));

    this.errors.push({
      timestamp: new Date(),
      error: normalizedError,
      context,
    });

    if (this.errors.length > this.maxSize) {
      this.errors.shift();
    }
  }

  
  getRecent(count = 10): QueuedError[] {
    return this.errors.slice(-count);
  }

  
  clear(): void {
    this.errors = [];
  }

  
  get size(): number {
    return this.errors.length;
  }
}


export const errorQueue = new ErrorQueue();
