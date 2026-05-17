import type { GitHubAPIError } from '../github/githubAPI';
import type {
  ToolErrorResult,
  ToolSuccessResult,
  ToolInvocationCallback,
} from '../types.js';
import type { HintContext } from '../types/metadata.js';
import type { ProviderResponse } from '../providers/types.js';
import { getHints } from '../hints/index.js';
import { logSessionError } from '../session.js';
import { TOOL_ERRORS } from '../errors/domainErrors.js';
import { createErrorResult } from '../utils/response/error.js';
import { attachRawResponseChars } from '../utils/response/charSavings.js';

export { createErrorResult };

/**
 * Safely invoke a tool invocation callback with error logging.
 * Errors are logged but not thrown - callback failures shouldn't block tool execution.
 */
export async function invokeCallbackSafely(
  callback: ToolInvocationCallback | undefined,
  toolName: string,
  queries: unknown[]
): Promise<void> {
  if (!callback) return;
  try {
    await callback(toolName, queries);
  } catch {
    // Log callback failure to session for monitoring
    logSessionError(toolName, TOOL_ERRORS.EXECUTION_FAILED.code).catch(() => {
      /* Secondary log failure is non-fatal */
    });
  }
}

/**
 * Options for createSuccessResult hint generation
 */
interface SuccessResultOptions {
  /** Context for generating dynamic hints */
  hintContext?: HintContext;
  /** High-priority hints prepended before all other hints (e.g., critical warnings) */
  prefixHints?: string[];
  /** Additional custom hints to append (e.g., pagination hints) */
  extraHints?: string[];
  /** Raw source response or character count used for local savings stats */
  rawResponse?: unknown;
}

/**
 * Create a success result with unified hint generation.
 * Uses getHints() to combine static hints from metadata + dynamic context-aware hints.
 *
 * @param query - The original query with research context
 * @param data - The result data
 * @param hasContent - Whether the result has content (determines hasResults vs empty status)
 * @param toolName - The tool name for hint lookup
 * @param options - Options for hint generation (context and extra hints)
 * @returns Formatted success result with hints
 *
 * @example
 * // Basic usage (static hints only)
 * createSuccessResult(query, data, true, 'githubSearchCode');
 *
 * @example
 * // With context for dynamic hints
 * createSuccessResult(query, data, true, 'githubSearchCode', {
 *   hintContext: { hasOwnerRepo: true, match: 'file' }
 * });
 *
 * @example
 * // With extra hints (e.g., pagination)
 * createSuccessResult(query, data, true, 'githubSearchCode', {
 *   hintContext: { hasOwnerRepo: true },
 *   extraHints: ['Page 1/5', 'Next: page=2']
 * });
 */
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
  const status = hasContent ? ('hasResults' as const) : ('empty' as const);

  const result: ToolSuccessResult & T = {
    status,
    ...data,
  };

  // Use unified getHints() which combines static + dynamic hints
  const hints = getHints(toolName, status, options?.hintContext);
  const prefixHints = options?.prefixHints || [];
  const extraHints = options?.extraHints || [];

  // prefixHints → tool hints → extraHints; deduplicate and filter empty
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

/**
 * Handle a failed ProviderResponse by converting it into a ToolErrorResult.
 * Preserves rate limit data, status, and provider hints that would otherwise
 * be lost if the error were wrapped in `new Error(string)`.
 */
export function handleProviderError(
  apiResult: ProviderResponse<unknown>,
  query: {
    mainResearchGoal?: string;
    researchGoal?: string;
    reasoning?: string;
  }
): ToolErrorResult {
  // Map ProviderResponse fields to GitHubAPIError shape
  // (which createErrorResult already knows how to format)
  const apiError: GitHubAPIError = {
    error: apiResult.error || 'Provider error',
    type: 'http',
    status: apiResult.status,
    rateLimitRemaining: apiResult.rateLimit?.remaining,
    // Convert reset from seconds to ms (GitHubAPIError uses ms)
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

  // Log the error to session for monitoring
  const logToolName = toolName || contextMessage || 'unknown_tool';
  logSessionError(logToolName, TOOL_ERRORS.EXECUTION_FAILED.code).catch(() => {
    /* Session log failure is non-fatal */
  });

  return createErrorResult(fullErrorMessage, query) as ToolErrorResult;
}
