/**
 * Shared GitLab provider utilities.
 *
 * @module providers/gitlab/utils
 */

import type { ProviderResponse } from '../types.js';
import type { GitLabAPIError, GitLabAPIResponse } from '../../gitlab/types.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';
import { logRateLimit } from '../../session.js';

type GitLabRepoSortField =
  | 'id'
  | 'name'
  | 'path'
  | 'created_at'
  | 'updated_at'
  | 'last_activity_at'
  | 'similarity'
  | 'star_count';

type GitLabMRState = 'opened' | 'closed' | 'merged' | 'all';

interface HandleGitLabAPIResponseOptions {
  stringifyError?: boolean;
  noDataMessage?: string;
}

/**
 * Parse a unified projectId into GitLab format.
 * GitLab accepts: numeric ID or URL-encoded path (e.g., "group%2Fproject").
 */
export function parseGitLabProjectId(projectId?: string): number | string {
  if (!projectId) {
    throw new Error('Project ID is required');
  }

  const numId = parseInt(projectId, 10);
  if (!isNaN(numId) && String(numId) === projectId) {
    return numId;
  }

  return encodeURIComponent(projectId);
}

/**
 * Map unified repo sort fields to GitLab sort fields.
 */
export function mapGitLabRepoSortField(
  sort?: string
): GitLabRepoSortField | undefined {
  const mapping: Record<string, GitLabRepoSortField> = {
    stars: 'star_count',
    updated: 'updated_at',
    created: 'created_at',
  };

  return sort ? mapping[sort] : undefined;
}

/**
 * Map unified PR states to GitLab MR states.
 */
export function mapGitLabMRState(state?: string): GitLabMRState | undefined {
  const mapping: Record<string, GitLabMRState> = {
    open: 'opened',
    closed: 'closed',
    merged: 'merged',
    all: 'all',
  };

  return state ? mapping[state] : undefined;
}

function extractGitLabRateLimit(result: {
  rateLimitRemaining?: number;
  rateLimitReset?: number;
  retryAfter?: number;
}): ProviderResponse<never>['rateLimit'] {
  const remaining = result.rateLimitRemaining;
  const reset = result.rateLimitReset;
  const retryAfter = result.retryAfter;

  if (
    remaining === undefined &&
    reset === undefined &&
    retryAfter === undefined
  ) {
    return undefined;
  }

  const computedReset =
    reset ??
    (retryAfter !== undefined
      ? Math.floor(Date.now() / 1000) + retryAfter
      : undefined);

  if (computedReset === undefined) {
    return undefined;
  }

  return {
    remaining: remaining ?? 0,
    reset: computedReset,
    retryAfter,
  };
}

function recordGitLabRateLimit(
  rateLimit: NonNullable<ProviderResponse<never>['rateLimit']>
): void {
  void logRateLimit({
    limit_type: 'primary',
    retry_after_seconds: rateLimit.retryAfter,
    rate_limit_remaining: rateLimit.remaining,
    rate_limit_reset_ms: rateLimit.reset * 1000,
    provider: 'gitlab',
  });
}

/**
 * Convert raw GitLab API responses into the shared provider response shape.
 */
export function handleGitLabAPIResponse<TData, TRaw>(
  result: GitLabAPIResponse<TRaw>,
  provider: 'gitlab',
  transform: (data: TRaw) => TData,
  options: HandleGitLabAPIResponseOptions = {}
): ProviderResponse<TData> {
  if ('error' in result && result.error) {
    const rateLimit = extractGitLabRateLimit(result as GitLabAPIError);
    if (rateLimit) {
      recordGitLabRateLimit(rateLimit);
    }

    const errorMessage =
      options.stringifyError || typeof result.error !== 'string'
        ? String(result.error)
        : result.error;

    return {
      error: errorMessage,
      status: result.status || 500,
      provider,
      hints: 'hints' in result ? (result as GitLabAPIError).hints : undefined,
      rateLimit,
    };
  }

  if (!('data' in result) || !result.data) {
    return {
      error: options.noDataMessage ?? 'No data returned from GitLab API',
      status: 500,
      provider,
    };
  }

  return {
    data: transform(result.data),
    status: 'status' in result ? result.status : 200,
    provider,
    rawResponseChars: countSerializedChars(result.data),
  };
}
