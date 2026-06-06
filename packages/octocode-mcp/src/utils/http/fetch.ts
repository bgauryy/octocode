import { version } from '../../../package.json';
import { FETCH_ERRORS } from '../../errors/domainErrors.js';
import {
  logPackageRegistryFailure,
  logRateLimit,
  logSessionError,
} from '../../session.js';
import { ignoreBestEffortFailure } from '../core/bestEffort.js';
import {
  assertCircuitAvailable,
  recordCircuitFailure,
  recordCircuitSuccess,
} from './circuitBreaker.js';

interface ExtendedError extends Error {
  status?: number;
  headers?: Headers;
  retryable?: boolean;
}

const MAX_BACKOFF_DELAY_MS = 60000;

interface FetchWithRetriesOptions {
  maxRetries?: number;
  initialDelayMs?: number;
  maxDelayMs?: number;

  headers?: Record<string, string>;
  method?: string;
  includeVersion?: boolean;

  signal?: AbortSignal;

  rateLimitProvider?: string;

  packageRegistry?: string;
}

function parseRetryAfterSeconds(headers: Headers): number | undefined {
  const retryAfter = headers.get('Retry-After');
  if (!retryAfter) return undefined;

  const seconds = parseInt(retryAfter, 10);
  return !isNaN(seconds) ? seconds : undefined;
}

function recordFetchRateLimit(
  provider: string | undefined,
  method: string,
  url: string,
  headers: Headers
): void {
  if (!provider) return;

  void logRateLimit({
    limit_type: 'primary',
    retry_after_seconds: parseRetryAfterSeconds(headers),
    api_method: method,
    api_url: url,
    provider,
  }).catch(ignoreBestEffortFailure('fetch rate-limit session logging'));
}

function buildHttpResponseError(
  res: Response,
  method: string,
  finalUrl: string,
  packageRegistry: string | undefined,
  rateLimitProvider: string | undefined
): ExtendedError {
  res.body
    ?.cancel?.()
    .catch(ignoreBestEffortFailure('response body cancellation'));

  logSessionError('fetchWithRetries', FETCH_ERRORS.FETCH_HTTP_ERROR.code).catch(
    ignoreBestEffortFailure('fetch retry session logging')
  );
  const error = new Error(
    FETCH_ERRORS.FETCH_HTTP_ERROR.message(res.status, res.statusText)
  ) as ExtendedError;

  error.status = res.status;
  error.headers = res.headers;

  if (packageRegistry && res.status !== 404) {
    logPackageRegistryFailure(packageRegistry);
  }

  if (res.status === 429) {
    recordFetchRateLimit(rateLimitProvider, method, finalUrl, res.headers);
  }

  error.retryable =
    res.status === 429 ||
    res.status === 408 ||
    (res.status >= 500 && res.status < 600);

  return error;
}

function computeBackoffDelayMs(
  attempt: number,
  initialDelayMs: number,
  maxDelayMs: number,
  extendedError: ExtendedError | undefined
): number {
  let delayMs = Math.min(initialDelayMs * Math.pow(2, attempt - 1), maxDelayMs);
  delayMs += Math.floor(Math.random() * initialDelayMs);

  if (
    extendedError &&
    extendedError.headers &&
    typeof extendedError.headers.get === 'function'
  ) {
    const seconds = parseRetryAfterSeconds(extendedError.headers);
    if (seconds !== undefined) {
      delayMs = Math.min(seconds * 1000, maxDelayMs);
    }
  }

  return delayMs;
}

export async function fetchWithRetries(
  url: string,
  options: FetchWithRetriesOptions = {}
): Promise<unknown> {
  const {
    maxRetries = 3,
    initialDelayMs = 1000,
    maxDelayMs = MAX_BACKOFF_DELAY_MS,
    headers = {},
    method = 'GET',
    includeVersion = false,
    signal,
    rateLimitProvider,
    packageRegistry,
  } = options;

  let finalUrl = url;
  if (includeVersion) {
    const separator = url.includes('?') ? '&' : '?';
    finalUrl = `${url}${separator}version=${encodeURIComponent(version)}`;
  }

  const finalHeaders: Record<string, string> = {
    'User-Agent': `Octocode-MCP/${version}`,
    ...headers,
  };

  const f = globalThis.fetch;
  if (!f) {
    logSessionError(
      'fetchWithRetries',
      FETCH_ERRORS.FETCH_NOT_AVAILABLE.code
    ).catch(ignoreBestEffortFailure('fetch retry session logging'));
    throw new Error(FETCH_ERRORS.FETCH_NOT_AVAILABLE.message);
  }

  assertCircuitAvailable(finalUrl);

  let lastError: Error | undefined;
  const maxAttempts = maxRetries + 1;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    if (signal?.aborted) {
      throw new Error('Request aborted');
    }

    try {
      const res = await f(finalUrl, {
        method,
        headers: finalHeaders,
        signal,
      });

      if (!res.ok) {
        throw buildHttpResponseError(
          res,
          method,
          finalUrl,
          packageRegistry,
          rateLimitProvider
        );
      }

      if (res.status === 204) {
        recordCircuitSuccess(finalUrl);
        return null;
      }

      const json = await res.json();
      recordCircuitSuccess(finalUrl);
      return json;
    } catch (error: unknown) {
      const extendedError = error as ExtendedError;

      if (
        signal?.aborted ||
        (error instanceof Error && error.name === 'AbortError')
      ) {
        throw new Error('Request aborted');
      }

      if (extendedError && extendedError.retryable === false) {
        throw error;
      }

      lastError = error instanceof Error ? error : new Error(String(error));

      if (attempt === maxAttempts) {
        break;
      }

      const delayMs = computeBackoffDelayMs(
        attempt,
        initialDelayMs,
        maxDelayMs,
        extendedError
      );

      await new Promise(resolve => setTimeout(resolve, delayMs));
    }
  }

  recordCircuitFailure(finalUrl);

  await logSessionError(
    'fetchWithRetries',
    FETCH_ERRORS.FETCH_FAILED_AFTER_RETRIES.code
  );
  throw new Error(
    FETCH_ERRORS.FETCH_FAILED_AFTER_RETRIES.message(
      maxAttempts,
      lastError?.message || ''
    )
  );
}
