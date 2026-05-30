import type { GitHubAPIError } from '../../github/githubAPI.js';
import type { ProviderResponse } from '../types.js';

type GitHubProviderErrorSource = Pick<GitHubAPIError, 'error'> &
  Partial<
    Pick<
      GitHubAPIError,
      | 'status'
      | 'hints'
      | 'rateLimitRemaining'
      | 'rateLimitReset'
      | 'retryAfter'
    >
  >;

type GitHubProviderErrorLike = {
  error: string | { toString(): string };
  status?: number;
  hints?: string[];
  rateLimitRemaining?: number;
  rateLimitReset?: number;
  retryAfter?: number;
};

/**
 * Parse a GitHub projectId string into owner/repo components.
 * @throws {Error} if projectId is provided but not in 'owner/repo' format.
 */
export function parseGitHubProjectId(projectId?: string): {
  owner?: string;
  repo?: string;
} {
  if (!projectId) {
    return { owner: undefined, repo: undefined };
  }

  const parts = projectId.split('/');
  if (parts.length !== 2 || !parts[0] || !parts[1]) {
    throw new Error(
      `Invalid GitHub projectId format: '${projectId}'. Expected 'owner/repo'.`
    );
  }

  return { owner: parts[0], repo: parts[1] };
}

function extractGitHubRateLimit(apiError: {
  rateLimitRemaining?: number;
  rateLimitReset?: number;
  retryAfter?: number;
}): ProviderResponse<never>['rateLimit'] {
  if (
    apiError.rateLimitRemaining === undefined &&
    apiError.retryAfter === undefined &&
    apiError.rateLimitReset === undefined
  ) {
    return undefined;
  }

  const resetMs = apiError.rateLimitReset;
  const reset =
    resetMs && !isNaN(resetMs)
      ? Math.floor(resetMs / 1000)
      : Math.floor(Date.now() / 1000) + (apiError.retryAfter ?? 3600);

  return {
    remaining: apiError.rateLimitRemaining ?? 0,
    reset,
    retryAfter: apiError.retryAfter,
  };
}

export function createGitHubProviderError(
  apiError: GitHubProviderErrorSource
): ProviderResponse<never> {
  return {
    error: apiError.error,
    status: apiError.status || 500,
    provider: 'github',
    hints: apiError.hints,
    rateLimit: extractGitHubRateLimit(apiError),
  };
}

export function createGitHubProviderErrorFromResult(
  result: unknown
): ProviderResponse<never> | null {
  if (!isGitHubProviderErrorLike(result)) {
    return null;
  }

  return createGitHubProviderError({
    error:
      typeof result.error === 'string' ? result.error : String(result.error),
    status: result.status || 500,
    hints: result.hints,
    rateLimitRemaining: result.rateLimitRemaining,
    rateLimitReset: result.rateLimitReset,
    retryAfter: result.retryAfter,
  });
}

function isGitHubProviderErrorLike(
  value: unknown
): value is GitHubProviderErrorLike {
  if (typeof value !== 'object' || value === null || !('error' in value)) {
    return false;
  }

  const record = value as Record<string, unknown>;
  const error = record.error;
  return typeof error === 'string' || hasToString(error);
}

function hasToString(value: unknown): value is { toString(): string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { toString?: unknown }).toString === 'function'
  );
}
