import { errorQueue } from './errorQueue.js';


const DEFAULT_TIMEOUT_MS = 5000;

export function fireAndForgetWithTimeout(
  operation: () => Promise<unknown>,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  context = 'fireAndForget'
): void {
  const controller = new AbortController();
  const { signal } = controller;

  const timeoutPromise = new Promise<never>((_, reject) => {
    const timeoutId = setTimeout(() => {
      controller.abort();
      reject(new Error(`Operation timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    signal.addEventListener('abort', () => clearTimeout(timeoutId), { once: true });
  });

  Promise.race([operation(), timeoutPromise])
    .catch((err: unknown) => {
      errorQueue.push(
        err instanceof Error ? err : new Error(String(err)),
        context
      );
    });
}

export async function withTimeout<T>(
  operation: () => Promise<T>,
  timeoutMs: number,
  context = 'withTimeout'
): Promise<T> {
  const controller = new AbortController();
  const { signal } = controller;

  const timeoutPromise = new Promise<never>((_, reject) => {
    const timeoutId = setTimeout(() => {
      controller.abort();
      reject(new Error(`${context}: Operation timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    signal.addEventListener('abort', () => clearTimeout(timeoutId), { once: true });
  });

  try {
    return await Promise.race([operation(), timeoutPromise]);
  } finally {
    controller.abort();
  }
}
