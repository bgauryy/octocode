import type { GitHubAPIError } from '../../github/githubAPI.js';
import {
  toToolError,
  isToolError,
  type ToolError,
} from '../../errors/ToolError.js';
import { getHints } from '../../hints/index.js';
import type { BaseQueryLocal } from '@octocodeai/octocode-core/extra-types';
import { attachRawResponseChars } from './charSavings.js';

type PartialBaseQuery = Partial<BaseQueryLocal>;

export interface UnifiedErrorResult {
  status: 'error';

  error?: string | GitHubAPIError;

  errorCode?: string;

  hints?: string[];

  [key: string]: unknown;
}

interface CreateErrorResultOptions {
  toolName?: string;

  hintContext?: Record<string, unknown>;

  extra?: Record<string, unknown>;

  customHints?: string[];

  hintSourceError?: GitHubAPIError;

  rawResponse?: unknown;
}

function extractProviderApiHints(apiError: GitHubAPIError): string[] {
  const hints: string[] = [];

  hints.push(`API Error: ${apiError.error}`);

  if (apiError.scopesSuggestion) {
    hints.push(apiError.scopesSuggestion);
  }

  if (
    apiError.rateLimitRemaining !== undefined &&
    apiError.rateLimitReset !== undefined
  ) {
    const resetMs = apiError.rateLimitReset;
    if (!isNaN(resetMs)) {
      const resetDate = new Date(resetMs);
      hints.push(
        `Rate limit: ${apiError.rateLimitRemaining} remaining, resets at ${resetDate.toISOString()}`
      );
    }
  }

  if (apiError.retryAfter !== undefined) {
    hints.push(`Retry after ${apiError.retryAfter} seconds`);
  }

  return hints;
}

function isGitHubApiError(error: unknown): error is GitHubAPIError {
  return (
    typeof error === 'object' &&
    error !== null &&
    'error' in error &&
    typeof (error as GitHubAPIError).error === 'string' &&
    ('type' in error || 'status' in error || 'scopesSuggestion' in error)
  );
}

export function createErrorResult(
  error: unknown,
  _query: PartialBaseQuery,
  options: CreateErrorResultOptions = {}
): UnifiedErrorResult {
  const { toolName, hintContext, extra, customHints, hintSourceError } =
    options;

  const result: UnifiedErrorResult = {
    status: 'error',
  };

  const hints: string[] = [];

  if (hintSourceError) {
    hints.push(...extractProviderApiHints(hintSourceError));
  }

  if (isGitHubApiError(error)) {
    result.error = error;
    if (!hintSourceError) {
      hints.push(...extractProviderApiHints(error));
    }
  } else if (isToolError(error)) {
    result.error = error.message;
    result.errorCode = error.errorCode;

    if (toolName) {
      const toolHints = getHints(toolName, 'error', {
        originalError: error.message,
        errorType: getErrorTypeFromToolError(error),
        ...hintContext,
      });
      hints.push(...toolHints);
    }
  } else if (typeof error === 'string') {
    result.error = error;
  } else if (error instanceof Error) {
    const toolError = toToolError(error);
    result.error = toolError.message;
    result.errorCode = toolError.errorCode;

    if (toolName) {
      const toolHints = getHints(toolName, 'error', {
        originalError: toolError.message,
        ...hintContext,
      });
      hints.push(...toolHints);
    }
  } else {
    result.error = 'Unknown error occurred';
  }

  if (customHints && customHints.length > 0) {
    hints.push(...customHints);
  }

  if (extra?.hints && Array.isArray(extra.hints)) {
    hints.push(...(extra.hints as string[]));
  }

  const filteredHints = hints.filter(
    h => typeof h === 'string' && h.trim().length > 0
  );
  if (filteredHints.length > 0) {
    result.hints = filteredHints;
  }

  if (extra) {
    const { hints: _hints, ...restExtra } = extra;
    void _hints;
    Object.assign(result, restExtra);
  }

  return options.rawResponse === undefined
    ? result
    : attachRawResponseChars(result, options.rawResponse);
}

function getErrorTypeFromToolError(
  error: ToolError
): 'size_limit' | 'not_found' | 'permission' | undefined {
  switch (error.errorCode) {
    case 'fileTooLarge':
    case 'outputTooLarge':
      return 'size_limit';
    case 'fileAccessFailed':
    case 'fileReadFailed':
      return 'not_found';
    case 'pathValidationFailed':
      return 'permission';
    default:
      return undefined;
  }
}
