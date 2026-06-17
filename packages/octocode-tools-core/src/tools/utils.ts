import type { GitHubAPIError } from '../github/githubAPI';
import type {
  ToolErrorResult,
  ToolSuccessResult,
  ToolInvocationCallback,
} from '../types/toolResults.js';
import type { HintContext } from '../types/metadata.js';
import type { ProviderResponse } from '../providers/types.js';
import { getHints } from '../hints/index.js';
import { logSessionError } from '../session.js';
import { TOOL_ERRORS } from '../errors/domainErrors.js';
import { createErrorResult } from '../utils/response/error.js';
import { attachRawResponseChars } from '../utils/response/charSavings.js';

export { createErrorResult };

export async function invokeCallbackSafely(
  callback: ToolInvocationCallback | undefined,
  toolName: string,
  queries: unknown[]
): Promise<void> {
  if (!callback) return;
  try {
    await callback(toolName, queries);
  } catch {
    logSessionError(toolName, TOOL_ERRORS.EXECUTION_FAILED.code).catch(() => {
      /* Secondary log failure is non-fatal */
    });
  }
}

interface SuccessResultOptions {
  hintContext?: HintContext;

  prefixHints?: string[];

  extraHints?: string[];

  rawResponse?: unknown;
}

export function createSuccessResult<T extends object>(
  _query: {
    mainResearchGoal?: string;
    researchGoal?: string;
    reasoning?: string;
  },
  data: T,
  hasContent: boolean,
  toolName: string,
  options?: SuccessResultOptions
): ToolSuccessResult & T {
  const status = hasContent ? undefined : ('empty' as const);

  const result: ToolSuccessResult & T = {
    ...(status !== undefined ? { status } : {}),
    ...data,
  } as ToolSuccessResult & T;

  const hints =
    status === 'empty' ? getHints(toolName, 'empty', options?.hintContext) : [];
  const prefixHints = options?.prefixHints || [];
  const extraHints = options?.extraHints || [];

  const allHints = [
    ...new Set([...prefixHints, ...hints, ...extraHints]),
  ].filter((h): h is string => typeof h === 'string' && h.trim().length > 0);

  if (allHints.length > 0) {
    result.hints = allHints;
  }

  return options?.rawResponse === undefined
    ? result
    : attachRawResponseChars(result, options.rawResponse);
}

export function handleProviderError(
  apiResult: ProviderResponse<unknown>,
  query: {
    mainResearchGoal?: string;
    researchGoal?: string;
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

  const externalHints = Array.isArray(apiResult.hints) ? apiResult.hints : [];

  const errorResult = createErrorResult(apiError, query, {
    hintSourceError: apiError,
    customHints: externalHints,
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
    mainResearchGoal?: string;
    researchGoal?: string;
    reasoning?: string;
  },
  contextMessage?: string,
  toolName?: string
): ToolErrorResult {
  const errorMessage =
    error instanceof Error ? error.message : 'Unknown error occurred';
  const fullErrorMessage = contextMessage
    ? `${contextMessage}: ${errorMessage}`
    : errorMessage;

  const logToolName = toolName || contextMessage || 'unknown_tool';
  logSessionError(logToolName, TOOL_ERRORS.EXECUTION_FAILED.code).catch(() => {
    /* Session log failure is non-fatal */
  });

  return createErrorResult(fullErrorMessage, query) as ToolErrorResult;
}
