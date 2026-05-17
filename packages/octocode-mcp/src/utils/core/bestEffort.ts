/**
 * Named sinks for intentionally non-critical async cleanup/logging failures.
 * Keeping these call sites explicit avoids silent empty catch handlers.
 */
export function ignoreBestEffortFailure(
  reason: string
): (error: unknown) => void {
  return error => {
    void reason;
    void error;
  };
}

export function fallbackOnBestEffortFailure<T>(
  reason: string,
  fallback: T
): (error: unknown) => T {
  return error => {
    void reason;
    void error;
    return fallback;
  };
}
