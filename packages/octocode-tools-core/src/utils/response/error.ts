import type { GitHubAPIError } from '../../github/githubAPI.js';
import { toToolError, isToolError } from '../../errors/ToolError.js';
import type { BaseQueryLocal } from '../../types/execution.js';
import { attachRawResponseChars } from './charSavings.js';

type PartialBaseQuery = Partial<BaseQueryLocal>;

export interface UnifiedErrorResult {
  status: 'error';

  error?: string | GitHubAPIError;

  errorCode?: string;

  [key: string]: unknown;
}

interface CreateErrorResultOptions {
  toolName?: string;

  extra?: Record<string, unknown>;

  rawResponse?: unknown;
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
  const { extra } = options;

  const result: UnifiedErrorResult = {
    status: 'error',
  };

  if (isGitHubApiError(error)) {
    result.error = error;
  } else if (isToolError(error)) {
    result.error = error.message;
    result.errorCode = error.errorCode;
  } else if (typeof error === 'string') {
    result.error = error;
  } else if (error instanceof Error) {
    const toolError = toToolError(error);
    result.error = toolError.message;
    result.errorCode = toolError.errorCode;
  } else {
    result.error = 'Unknown error occurred';
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

/**
 * Minimal shape of the parts of a Zod schema this helper relies on, kept
 * version-agnostic so it does not couple to a specific Zod release.
 */
interface SafeParseableSchema<T> {
  safeParse(input: unknown):
    | { success: true; data: T; error?: never }
    | {
        success: false;
        data?: never;
        error: { issues: Array<{ message: string }> };
      };
}

export type SafeParseOutcome<T> =
  { ok: true; data: T } | { ok: false; error: UnifiedErrorResult };

/**
 * Validate `input` against `schema`, returning either the parsed data or a
 * structured {@link UnifiedErrorResult} built via {@link createErrorResult}.
 *
 * Replaces per-tool issue joining. Messages are direct by default; pass
 * `prefix: true` only when a caller needs the legacy "Validation error: " label.
 */
export function safeParseOrError<T>(
  schema: SafeParseableSchema<T>,
  query: PartialBaseQuery,
  options: { toolName?: string; prefix?: boolean } = {}
): SafeParseOutcome<T> {
  const result = schema.safeParse(query);
  if (result.success) {
    return { ok: true, data: result.data };
  }

  const messages = result.error.issues.map(i => i.message).join('; ');
  const text =
    options.prefix === true ? `Validation error: ${messages}` : messages;

  return {
    ok: false,
    error: createErrorResult(text, query, { toolName: options.toolName }),
  };
}
