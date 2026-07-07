import { RequestError } from 'octokit';
import type { GitHubAPIError } from './githubAPI.js';
import {
  ERROR_CODES,
  ERROR_MESSAGES,
  STATUS_TO_ERROR_CODE,
  NETWORK_ERROR_PATTERNS,
  RATE_LIMIT_PATTERNS,
  RATE_LIMIT_CONFIG,
  type ErrorCode,
} from './errorConstants.js';
import { recordRateLimit } from '../session.js';

const NO_RESULTS_SEARCH_PHRASES = [
  'cannot be searched',
  'do not exist',
  'does not exist',
  'could not be found',
  'cannot be found',
];

export function isNoResultsSearchError(error: unknown): boolean {
  if (!(error instanceof RequestError)) return false;
  if (error.status !== 422) return false;

  const errors = (
    error.response?.data as
      { errors?: Array<{ message?: unknown }> } | undefined
  )?.errors;
  if (!Array.isArray(errors) || errors.length === 0) return false;

  return errors.some(entry => {
    const message =
      typeof entry?.message === 'string' ? entry.message.toLowerCase() : '';
    return NO_RESULTS_SEARCH_PHRASES.some(phrase => message.includes(phrase));
  });
}

export function handleGitHubAPIError(error: unknown): GitHubAPIError {
  if (error instanceof RequestError) {
    return handleRequestError(error);
  }

  if (error instanceof Error) {
    return handleJavaScriptError(error);
  }

  return {
    error:
      typeof error === 'string'
        ? error
        : ERROR_MESSAGES[ERROR_CODES.UNKNOWN].message,
    type: 'unknown',
  };
}

function handleRequestError(error: RequestError): GitHubAPIError {
  const { status, message, response } = error;

  if (status === 403) {
    return handle403Error(message, response);
  }

  if (status === 429) {
    return handle429RateLimit(message, response);
  }

  const errorCode = STATUS_TO_ERROR_CODE[status];
  if (errorCode) {
    return handleKnownHttpError(errorCode, status);
  }

  return createErrorResponse(ERROR_CODES.UNKNOWN, {
    error: message || ERROR_MESSAGES[ERROR_CODES.UNKNOWN].message,
    status,
  });
}

function parseHeaderInteger(
  headers: Record<string, unknown> | undefined,
  key: string
): number | undefined {
  const rawValue = headers?.[key];
  const parsed = rawValue === undefined ? NaN : parseInt(String(rawValue), 10);
  return !isNaN(parsed) ? parsed : undefined;
}

function handle429RateLimit(
  message: string,
  response?: RequestError['response']
): GitHubAPIError {
  const headers = response?.headers;
  const retryAfter = parseHeaderInteger(headers, 'retry-after');
  const resetValue = parseHeaderInteger(headers, 'x-ratelimit-reset');
  const remaining = parseHeaderInteger(headers, 'x-ratelimit-remaining') ?? 0;
  const resetTime = resetValue ? new Date(resetValue * 1000) : null;
  const retryAfterSeconds =
    retryAfter ??
    (resetTime
      ? Math.max(
          Math.ceil((resetTime.getTime() - Date.now()) / 1000) +
            RATE_LIMIT_CONFIG.RESET_BUFFER_SECONDS,
          0
        )
      : undefined);

  recordRateLimit({
    limit_type: 'primary',
    retry_after_seconds: retryAfterSeconds,
    rate_limit_remaining: remaining,
    rate_limit_reset_ms: resetTime ? resetTime.getTime() : undefined,
    provider: 'github',
  });

  return createErrorResponse(ERROR_CODES.RATE_LIMIT_PRIMARY, {
    error:
      message ||
      ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_PRIMARY].messageWithoutTime,
    status: 429,
    rateLimitRemaining: remaining,
    rateLimitReset: resetTime ? resetTime.getTime() : undefined,
    retryAfter: retryAfterSeconds,
    scopesSuggestion: ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_PRIMARY].suggestion,
  });
}

function handle403Error(
  message: string,
  response?: RequestError['response']
): GitHubAPIError {
  const headers = response?.headers;

  if (RATE_LIMIT_PATTERNS.SECONDARY.test(message)) {
    return handleSecondaryRateLimit(headers);
  }

  const remaining = headers?.['x-ratelimit-remaining'];
  const isGraphQLRateLimited = checkGraphQLRateLimit(response);

  if (
    (remaining !== undefined && String(remaining) === '0') ||
    isGraphQLRateLimited
  ) {
    return handlePrimaryRateLimit(headers);
  }

  return handlePermissionError(headers);
}

function handleSecondaryRateLimit(
  headers?: Record<string, unknown>
): GitHubAPIError {
  const parsed = Number(headers?.['retry-after']);
  const retryAfter = !isNaN(parsed)
    ? parsed
    : RATE_LIMIT_CONFIG.SECONDARY_FALLBACK_SECONDS;

  recordRateLimit({
    limit_type: 'secondary',
    retry_after_seconds: retryAfter,
    provider: 'github',
  });

  return createErrorResponse(ERROR_CODES.RATE_LIMIT_SECONDARY, {
    error: ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_SECONDARY].message(retryAfter),
    status: 403,
    rateLimitRemaining: 0,
    retryAfter,
    scopesSuggestion:
      ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_SECONDARY].suggestion,
  });
}

function handlePrimaryRateLimit(
  headers?: Record<string, unknown>
): GitHubAPIError {
  const reset = headers?.['x-ratelimit-reset'];
  const resetValue = reset ? parseInt(String(reset), 10) : NaN;
  const resetTime = !isNaN(resetValue) ? new Date(resetValue * 1000) : null;

  const retryAfterSeconds = resetTime
    ? Math.max(
        Math.ceil((resetTime.getTime() - Date.now()) / 1000) +
          RATE_LIMIT_CONFIG.RESET_BUFFER_SECONDS,
        0
      )
    : undefined;

  const errorMessage = resetTime
    ? ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_PRIMARY].messageWithTime(
        resetTime,
        retryAfterSeconds!
      )
    : ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_PRIMARY].messageWithoutTime;

  recordRateLimit({
    limit_type: 'primary',
    retry_after_seconds: retryAfterSeconds,
    rate_limit_remaining: 0,
    rate_limit_reset_ms: resetTime ? resetTime.getTime() : undefined,
    provider: 'github',
  });

  return createErrorResponse(ERROR_CODES.RATE_LIMIT_PRIMARY, {
    error: errorMessage,
    status: 403,
    rateLimitRemaining: 0,
    rateLimitReset: resetTime ? resetTime.getTime() : undefined,
    retryAfter: retryAfterSeconds,
    scopesSuggestion: ERROR_MESSAGES[ERROR_CODES.RATE_LIMIT_PRIMARY].suggestion,
  });
}

function handlePermissionError(
  headers?: Record<string, unknown>
): GitHubAPIError {
  const acceptedScopes = headers?.['x-accepted-oauth-scopes'];
  const tokenScopes = headers?.['x-oauth-scopes'];

  let scopesSuggestion: string =
    ERROR_MESSAGES[ERROR_CODES.FORBIDDEN_PERMISSIONS].suggestion;

  if (acceptedScopes && tokenScopes) {
    scopesSuggestion = generateScopesSuggestion(
      String(acceptedScopes),
      String(tokenScopes)
    );
  }

  return createErrorResponse(ERROR_CODES.FORBIDDEN_PERMISSIONS, {
    error: ERROR_MESSAGES[ERROR_CODES.FORBIDDEN_PERMISSIONS].message,
    status: 403,
    scopesSuggestion,
  });
}

function checkGraphQLRateLimit(response?: RequestError['response']): boolean {
  const errors = (
    response?.data as {
      errors?: Array<{ type?: string }>;
    }
  )?.errors;

  return (
    errors?.some(err => err.type === RATE_LIMIT_PATTERNS.GRAPHQL_TYPE) ?? false
  );
}

function handleKnownHttpError(
  errorCode: ErrorCode,
  status: number
): GitHubAPIError {
  const errorDef = ERROR_MESSAGES[errorCode];

  return createErrorResponse(errorCode, {
    error: errorDef.message as string,
    status,
    ...('suggestion' in errorDef && { scopesSuggestion: errorDef.suggestion }),
  });
}

function handleJavaScriptError(error: Error): GitHubAPIError {
  if (
    NETWORK_ERROR_PATTERNS.CONNECTION_FAILED.some(pattern =>
      error.message.includes(pattern)
    )
  ) {
    return {
      error: ERROR_MESSAGES[ERROR_CODES.NETWORK_CONNECTION_FAILED].message,
      type: 'network',
      scopesSuggestion:
        ERROR_MESSAGES[ERROR_CODES.NETWORK_CONNECTION_FAILED].suggestion,
    };
  }

  if (
    NETWORK_ERROR_PATTERNS.TIMEOUT.some(pattern =>
      error.message.includes(pattern)
    )
  ) {
    return {
      error: ERROR_MESSAGES[ERROR_CODES.REQUEST_TIMEOUT].message,
      type: 'network',
      scopesSuggestion: ERROR_MESSAGES[ERROR_CODES.REQUEST_TIMEOUT].suggestion,
    };
  }

  return {
    error: error.message,
    type: 'unknown',
  };
}

function createErrorResponse(
  _errorCode: ErrorCode,
  overrides: Partial<GitHubAPIError> & { error: string }
): GitHubAPIError {
  return {
    type: 'http',
    ...overrides,
  } as GitHubAPIError;
}

function generateScopesSuggestion(
  acceptedScopes: string,
  tokenScopes: string
): string {
  const accepted = acceptedScopes
    .split(',')
    .map(s => s.trim())
    .filter(Boolean);
  const current = tokenScopes
    .split(',')
    .map(s => s.trim())
    .filter(Boolean);

  const missing = accepted.filter(scope => !current.includes(scope));

  if (missing.length > 0) {
    return ERROR_MESSAGES[
      ERROR_CODES.FORBIDDEN_PERMISSIONS
    ].suggestionWithScopes(missing);
  }

  return ERROR_MESSAGES[ERROR_CODES.FORBIDDEN_PERMISSIONS].fallbackSuggestion;
}
