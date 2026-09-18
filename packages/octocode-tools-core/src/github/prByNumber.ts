import type {
  GitHubAPIError,
  GitHubPullRequestItem,
  GitHubPullRequestsSearchParams,
  PullRequestItem,
} from './githubAPI.js';
import type { GitHubPullRequestSearchApiResult } from '../tools/github_search_pull_requests/types.js';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { handleGitHubAPIError } from './errors.js';
import { generateCacheKey } from '../utils/http/cache/key.js';
import { withDataCacheConditional } from '../utils/http/cache/conditional.js';
import { RequestError } from 'octokit';
import { extractEtag } from './responseHeaders.js';
import { AuthInfo } from '@modelcontextprotocol/server';
import { formatPRForResponse } from './prTransformation.js';
import { transformPullRequestItemFromREST } from './prContentFetcher/transform.js';
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
    pullRequests: [],
    totalCount: 0,
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
  authInfo?: AuthInfo
): Promise<GitHubPullRequestSearchApiResult> {
  const { owner, repo, prNumber } = params;

  if (!owner || !repo || !prNumber) {
    return {
      pullRequests: [],
      totalCount: 0,
      error: SEARCH_ERRORS.PR_REQUIRED_PARAMS.message,
      hints: ['Provide owner, repo, and prNumber'],
    };
  }

  if (Array.isArray(owner) || Array.isArray(repo)) {
    return {
      pullRequests: [],
      totalCount: 0,
      error: SEARCH_ERRORS.PR_SINGLE_VALUES.message,
      hints: ['Do not use array for owner or repo when fetching by number'],
    };
  }

  try {
    const octokit = await getOctokit(authInfo);

    const auth = await resolveCacheAuthFingerprint(authInfo);
    const pr = await withDataCacheConditional<PullRequestItem | undefined>(
      generateCacheKey('gh-api-prs', {
        kind: 'metadata',
        owner,
        repo,
        prNumber,
        auth,
      }),
      async ({ ifNoneMatch }) => {
        try {
          const result = await octokit.rest.pulls.get({
            owner,
            repo,
            pull_number: prNumber,
            ...(ifNoneMatch && { headers: { 'if-none-match': ifNoneMatch } }),
          });
          return { value: result.data, etag: extractEtag(result.headers) };
        } catch (error) {
          if (error instanceof RequestError && error.status === 304) {
            return { value: undefined, notModified: true };
          }
          throw error;
        }
      },
      { cacheRole: 'helper', shouldCache: value => value !== undefined }
    );
    if (!pr) throw new Error('GitHub returned no pull request metadata');

    const transformedPR: GitHubPullRequestItem =
      await transformPullRequestItemFromREST(pr, params, octokit, authInfo);

    const formattedPR = formatPRForResponse(transformedPR, {
      includeFullBody: true,
      includeFullCommentDetails: true,
    });

    return {
      pullRequests: [formattedPR],
      totalCount: 1,
      rawResponseChars:
        countSerializedChars(pr) + (getRawResponseChars(transformedPR) ?? 0),
    };
  } catch (error: unknown) {
    const apiError = handleGitHubAPIError(error);

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
