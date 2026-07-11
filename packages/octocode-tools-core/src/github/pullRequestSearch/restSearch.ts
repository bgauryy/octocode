import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../../config.js';
import type {
  GitHubAPIError,
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  PullRequestSimple,
} from '../githubAPI.js';
import type { GitHubPullRequestSearchApiResult } from '../../tools/github_search_pull_requests/types.js';
import { SEARCH_ERRORS } from '../../errors/domainErrors.js';
import { OctokitWithThrottling } from '../client.js';
import { handleGitHubAPIError, isNoResultsSearchError } from '../errors.js';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

import { formatPRForResponse } from '../prTransformation.js';
import { transformPullRequestItemFromREST } from '../prContentFetcher.js';

export function createPullRequestErrorResult(
  apiError: GitHubAPIError,
  error: string,
  hints: string[]
): GitHubPullRequestSearchApiResult {
  return {
    pull_requests: [],
    total_count: 0,
    error,
    status: apiError.status,
    hints,
    rateLimitRemaining: apiError.rateLimitRemaining,
    rateLimitReset: apiError.rateLimitReset,
    retryAfter: apiError.retryAfter,
  };
}

export function createPullRequestEmptyResult(
  params: GitHubPullRequestsSearchParams
): GitHubPullRequestSearchApiResult {
  const perPage = Math.min(
    params.limit || GITHUB_SEARCH_DEFAULT_LIMIT,
    GITHUB_SEARCH_MAX_LIMIT
  );
  return {
    pull_requests: [],
    total_count: 0,
    pagination: {
      currentPage: params.page || 1,
      totalPages: 0,
      perPage,
      totalMatches: 0,
      reportedTotalMatches: 0,
      reachableTotalMatches: 0,
      totalMatchesKind: 'exact',
      totalMatchesCapped: false,
      hasMore: false,
    },
  };
}

export async function searchPullRequestsWithREST(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  params: GitHubPullRequestsSearchParams
): Promise<GitHubPullRequestSearchApiResult> {
  try {
    const owner = params.owner as string;
    const repo = params.repo as string;

    const perPage = Math.min(
      params.limit || GITHUB_SEARCH_DEFAULT_LIMIT,
      GITHUB_SEARCH_MAX_LIMIT
    );
    const currentPage = params.page || 1;

    const result = await octokit.rest.pulls.list({
      owner,
      repo,
      state: (params.state === 'merged' ? 'closed' : params.state) || 'open',
      per_page: perPage,
      page: currentPage,
      sort: params.sort === 'updated' ? 'updated' : 'created',
      direction: params.order || 'desc',
      ...(params.head && { head: params.head }),
      ...(params.base && { base: params.base }),
    });

    const transformedPRs: GitHubPullRequestItem[] = await Promise.all(
      result.data.map(async (item: PullRequestSimple) => {
        return await transformPullRequestItemFromREST(item, params, octokit);
      })
    );

    const transformedRawResponseChars = transformedPRs.reduce(
      (sum, pr) => sum + (getRawResponseChars(pr) ?? 0),
      0
    );
    const formattedPRs = transformedPRs.map(pr =>
      formatPRForResponse(pr, {
        includeFullBody: false,
        includeFullCommentDetails: false,
        charOffset: params.charOffset,
        charLength: params.charLength,
      })
    );

    const hasMore = result.data.length === perPage;
    const seenThroughPage = (currentPage - 1) * perPage + formattedPRs.length;

    return {
      pull_requests: formattedPRs,
      total_count: formattedPRs.length,
      pagination: {
        currentPage,
        totalPages: hasMore ? currentPage + 1 : currentPage,
        perPage,
        totalMatches: seenThroughPage + (hasMore ? 1 : 0),
        reachableTotalMatches: seenThroughPage,
        totalMatchesKind: hasMore ? 'lowerBound' : 'exact',
        hasMore,
        ...(hasMore ? { nextPage: currentPage + 1 } : {}),
      },
      rawResponseChars:
        countSerializedChars(result.data) + transformedRawResponseChars,
    };
  } catch (error: unknown) {
    if (isNoResultsSearchError(error)) {
      return createPullRequestEmptyResult(params);
    }
    const apiError = handleGitHubAPIError(error);
    return createPullRequestErrorResult(
      apiError,
      SEARCH_ERRORS.PULL_REQUEST_LIST_FAILED.message(apiError.error),
      ['Verify repository access and authentication']
    );
  }
}
