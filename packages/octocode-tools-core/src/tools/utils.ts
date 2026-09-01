import type { GitHubAPIError } from '../github/githubAPI';
import type {
  ToolErrorResult,
  ToolSuccessResult,
  ToolInvocationCallback,
} from '../types/toolResults.js';
import type { ProviderResponse } from '../providers/types.js';
import {
  createErrorResult,
  safeParseOrError,
} from '../utils/response/error.js';
import { attachRawResponseChars } from '../utils/response/charSavings.js';

export { createErrorResult, safeParseOrError };

export async function invokeCallbackSafely(
  callback: ToolInvocationCallback | undefined,
  toolName: string,
  queries: unknown[]
): Promise<void> {
  if (!callback) return;
  try {
    await callback(toolName, queries);
  } catch {
    return;
  }
}

interface SuccessResultOptions {
  rawResponse?: unknown;
}

export function createSuccessResult<T extends object>(
  _query: {
    goal?: string;
    reasoning?: string;
  },
  data: T,
  hasContent: boolean,
  _toolName: string,
  options?: SuccessResultOptions
): ToolSuccessResult & T {
  const status = hasContent ? undefined : ('empty' as const);

  const result: ToolSuccessResult & T = {
    ...(status !== undefined ? { status } : {}),
    ...data,
  } as ToolSuccessResult & T;

  return options?.rawResponse === undefined
    ? result
    : attachRawResponseChars(result, options.rawResponse);
}

export function handleProviderError(
  apiResult: ProviderResponse<unknown>,
  query: {
    goal?: string;
    reasoning?: string;
  }
): ToolErrorResult {
  const apiError: GitHubAPIError = {
    error: apiResult.error || 'Provider error',
    type: 'http',
    status: apiResult.status,
    rateLimitRemaining: apiResult.rateLimit?.remaining,
    rateLimitReset: apiResult.rateLimit?.reset
      ? apiResult.rateLimit.reset * 1000
      : undefined,
    retryAfter: apiResult.rateLimit?.retryAfter,
  };

  const errorResult = createErrorResult(apiError, query, {
    rawResponse:
      apiResult.rawResponseChars ??
      apiResult.data ??
      (apiResult.error ? apiResult : undefined),
  });

  return errorResult as ToolErrorResult;
}

export function handleCatchError(
  error: unknown,
  query: {
    goal?: string;
    reasoning?: string;
  },
  contextMessage?: string,
  toolName?: string
): ToolErrorResult {
  const result = createErrorResult(error, query, {
    toolName,
  }) as ToolErrorResult;
  if (contextMessage && typeof result.error === 'string') {
    result.error = `${contextMessage}: ${result.error}`;
  }
  return result;
}
