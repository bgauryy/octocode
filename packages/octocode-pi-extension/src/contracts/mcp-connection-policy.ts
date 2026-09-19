/** Shared bounds for persisted MCP startup retry policy. */
export const DEFAULT_MCP_STARTUP_RETRIES = 1;
export const DEFAULT_MCP_RETRY_DELAY_MS = 100;
export const MAX_MCP_STARTUP_RETRIES = 5;
export const MAX_MCP_RETRY_DELAY_MS = 10_000;

export function validateMcpRetryPolicy(values: {
  startupRetries?: unknown;
  retryDelayMs?: unknown;
}): string[] {
  const errors: string[] = [];
  const { startupRetries, retryDelayMs } = values;
  if (startupRetries !== undefined && (
    typeof startupRetries !== 'number' || !Number.isSafeInteger(startupRetries) ||
    startupRetries < 0 || startupRetries > MAX_MCP_STARTUP_RETRIES
  )) errors.push(`startupRetries must be an integer between 0 and ${MAX_MCP_STARTUP_RETRIES}`);
  if (retryDelayMs !== undefined && (
    typeof retryDelayMs !== 'number' || !Number.isSafeInteger(retryDelayMs) ||
    retryDelayMs < 0 || retryDelayMs > MAX_MCP_RETRY_DELAY_MS
  )) errors.push(`retryDelayMs must be an integer between 0 and ${MAX_MCP_RETRY_DELAY_MS}`);
  return errors;
}
