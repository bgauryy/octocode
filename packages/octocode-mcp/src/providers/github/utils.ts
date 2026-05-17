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

export function extractGitHubRateLimit(apiError: {
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
