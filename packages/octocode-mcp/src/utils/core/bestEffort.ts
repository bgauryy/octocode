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
