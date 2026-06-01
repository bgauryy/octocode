const DEBUG = process.env.OCTOCODE_DEBUG === '1';

export function trySafe<T>(fn: () => T, fallback: T): T {
  try {
    return fn();
  } catch (error) {
    if (DEBUG) {
      console.error(
        '[trySafe]',
        error instanceof Error ? error.message : error
      );
    }
    return fallback;
  }
}

export async function trySafeAsync<T>(
  fn: () => Promise<T>,
  fallback: T
): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    if (DEBUG) {
      console.error(
        '[trySafeAsync]',
        error instanceof Error ? error.message : error
      );
    }
    return fallback;
  }
}
