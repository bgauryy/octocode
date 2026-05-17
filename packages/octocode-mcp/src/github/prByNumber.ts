/**
 * Fetch a single pull request by number — public API + internal implementation.
 * Extracted from pullRequestSearch.ts.
 */
import type {
  GitHubAPIError,
  GitHubPullRequestItem,
  GitHubPullRequestsSearchParams,
} from './githubAPI.js';
import type { GitHubPullRequestSearchApiResult } from '../tools/github_search_pull_requests/types.js';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import { logSessionError } from '../session.js';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { getOctokit } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { formatPRForResponse } from './prTransformation.js';
import { transformPullRequestItemFromREST } from './prContentFetcher.js';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';

function createPullRequestByNumberErrorResult(
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

export async function fetchGitHubPullRequestByNumberAPI(
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubPullRequestSearchApiResult> {
  const cacheKey = generateCacheKey(
    'gh-api-prs',
    {
      owner: params.owner,
      repo: params.repo,
      prNumber: params.prNumber,
      type: params.type,
      partialContentMetadata: params.partialContentMetadata,
      withComments: params.withComments,
    },
    sessionId
  );

  const result = await withDataCache<GitHubPullRequestSearchApiResult>(
    cacheKey,
    async () => {
      return await fetchGitHubPullRequestByNumberAPIInternal(params, authInfo);
    },
    {
      shouldCache: (value: GitHubPullRequestSearchApiResult) => !value.error,
    }
  );

  return result;
}

export async function fetchGitHubPullRequestByNumberAPIInternal(
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo
): Promise<GitHubPullRequestSearchApiResult> {
  const { owner, repo, prNumber } = params;

  if (!owner || !repo || !prNumber) {
    await logSessionError(
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      SEARCH_ERRORS.PR_REQUIRED_PARAMS.code
    );
    return {
      pull_requests: [],
      total_count: 0,
      error: SEARCH_ERRORS.PR_REQUIRED_PARAMS.message,
      hints: ['Provide owner, repo, and prNumber'],
    };
  }

  if (Array.isArray(owner) || Array.isArray(repo)) {
    await logSessionError(
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      SEARCH_ERRORS.PR_SINGLE_VALUES.code
    );
    return {
      pull_requests: [],
      total_count: 0,
      error: SEARCH_ERRORS.PR_SINGLE_VALUES.message,
      hints: ['Do not use array for owner or repo when fetching by number'],
    };
  }

  try {
    const octokit = await getOctokit(authInfo);

    const result = await octokit.rest.pulls.get({
      owner,
      repo,
      pull_number: prNumber,
    });

    const pr = result.data;

    const transformedPR: GitHubPullRequestItem =
      await transformPullRequestItemFromREST(pr, params, octokit, authInfo);

    const formattedPR = formatPRForResponse(transformedPR);

    return {
      pull_requests: [formattedPR],
      total_count: 1,
      rawResponseChars:
        countSerializedChars(result.data) + (getRawResponseChars(transformedPR) ?? 0),
    };
  } catch (error: unknown) {
    const apiError = handleGitHubAPIError(error);

    await logSessionError(
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      SEARCH_ERRORS.PULL_REQUEST_FETCH_FAILED.code
    );
    return createPullRequestByNumberErrorResult(
      apiError,
      SEARCH_ERRORS.PULL_REQUEST_FETCH_FAILED.message(prNumber, apiError.error),
      [
        `Verify that pull request #${prNumber} exists in ${owner}/${repo}`,
        'Check if you have access to this repository',
        'Ensure the PR number is correct',
      ]
    );
  }
}
