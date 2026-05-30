/**
 * GitHub Pull Request Search Operations
 * Orchestrates searching/listing pull requests.
 * Split into focused modules:
 *   - prTransformation.ts: PR data transformation and formatting
 *   - prContentFetcher.ts: comments, commits, file changes, item transforms
 *   - prByNumber.ts: fetch single PR by number
 */
import type {
  GitHubAPIError,
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  IssueSearchResultItem,
  PullRequestSimple,
} from './githubAPI.js';
import type { GitHubPullRequestSearchApiResult } from '../tools/github_search_pull_requests/types.js';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import { logSessionError } from '../session.js';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { getOctokit, OctokitWithThrottling } from './client.js';
import { handleGitHubAPIError, isNoResultsSearchError } from './errors.js';
import {
  buildPullRequestSearchQuery,
  shouldUseSearchForPRs,
} from './queryBuilders.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import {
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';

import { formatPRForResponse } from './prTransformation.js';
import {
  transformPullRequestItemFromSearch,
  transformPullRequestItemFromREST,
} from './prContentFetcher.js';
import { fetchGitHubPullRequestByNumberAPIInternal } from './prByNumber.js';

function createPullRequestErrorResult(
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

/**
 * Clean empty result (no `error` field) for searches whose filters reference a
 * nonexistent entity — GitHub 422s these even though "no matches" is the
 * truthful answer. Mirrors the zero-result success shape so the tool layer
 * classifies it as `empty`, not `error`.
 */
function createPullRequestEmptyResult(
  params: GitHubPullRequestsSearchParams
): GitHubPullRequestSearchApiResult {
  const perPage = Math.min(params.limit || 30, 100);
  return {
    pull_requests: [],
    total_count: 0,
    pagination: {
      currentPage: params.page || 1,
      totalPages: 0,
      perPage,
      totalMatches: 0,
      hasMore: false,
    },
  };
}

export async function searchGitHubPullRequestsAPI(
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubPullRequestSearchApiResult> {
  const cacheKey = generateCacheKey(
    'gh-api-prs',
    {
      query: params.query,
      owner: params.owner,
      repo: params.repo,
      prNumber: params.prNumber,
      state: params.state,
      draft: params.draft,
      merged: params.merged,
      author: params.author,
      assignee: params.assignee,
      mentions: params.mentions,
      commenter: params.commenter,
      involves: params.involves,
      'reviewed-by': params['reviewed-by'],
      'review-requested': params['review-requested'],
      head: params.head,
      base: params.base,
      created: params.created,
      updated: params.updated,
      'merged-at': params['merged-at'],
      closed: params.closed,
      comments: params.comments,
      reactions: params.reactions,
      interactions: params.interactions,
      label: params.label,
      'no-assignee': params['no-assignee'],
      'no-label': params['no-label'],
      'no-milestone': params['no-milestone'],
      'no-project': params['no-project'],
      match: params.match,
      sort: params.sort,
      order: params.order,
      limit: params.limit,
      page: params.page,
      withComments: params.withComments,
      withCommits: params.withCommits,
      type: params.type,
      partialContentMetadata: params.partialContentMetadata,
    },
    sessionId
  );

  const result = await withDataCache<GitHubPullRequestSearchApiResult>(
    cacheKey,
    async () => {
      return await searchGitHubPullRequestsAPIInternal(
        params,
        authInfo,
        sessionId
      );
    },
    {
      shouldCache: (value: GitHubPullRequestSearchApiResult) => !value.error,
    }
  );

  return result;
}

async function searchGitHubPullRequestsAPIInternal(
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo,
  _sessionId?: string
): Promise<GitHubPullRequestSearchApiResult> {
  try {
    if (
      params.prNumber &&
      params.owner &&
      params.repo &&
      !Array.isArray(params.owner) &&
      !Array.isArray(params.repo)
    ) {
      return await fetchGitHubPullRequestByNumberAPIInternal(params, authInfo);
    }

    const octokit = await getOctokit(authInfo);

    const shouldUseSearch = shouldUseSearchForPRs(params);

    if (
      !shouldUseSearch &&
      params.owner &&
      params.repo &&
      !Array.isArray(params.owner) &&
      !Array.isArray(params.repo)
    ) {
      return await searchPullRequestsWithREST(octokit, params);
    }

    const searchQuery = buildPullRequestSearchQuery(params);

    if (!searchQuery) {
      await logSessionError(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
        SEARCH_ERRORS.NO_VALID_PARAMETERS.code
      );
      return {
        pull_requests: [],
        total_count: 0,
        error: SEARCH_ERRORS.NO_VALID_PARAMETERS.message,
        hints: ['Provide search query or filters like owner/repo'],
      };
    }

    const sortValue =
      params.sort && params.sort !== 'best-match' && params.sort !== 'created'
        ? params.sort
        : undefined;

    const perPage = Math.min(params.limit || 30, 100);
    const currentPage = params.page || 1;

    const searchResult = await octokit.rest.search.issuesAndPullRequests({
      q: searchQuery,
      sort: sortValue as
        | 'comments'
        | 'reactions'
        | 'created'
        | 'updated'
        | undefined,
      order: params.order || 'desc',
      per_page: perPage,
      page: currentPage,
    });

    const pullRequests = (searchResult.data.items?.filter(
      (item: IssueSearchResultItem) => !!item.pull_request
    ) || []) as IssueSearchResultItem[];

    const transformedPRs: GitHubPullRequestItem[] = await Promise.all(
      pullRequests.map(async (item: IssueSearchResultItem) => {
        return await transformPullRequestItemFromSearch(item, params, octokit);
      })
    );

    const transformedRawResponseChars = transformedPRs.reduce(
      (sum, pr) => sum + (getRawResponseChars(pr) ?? 0),
      0
    );
    const formattedPRs = transformedPRs.map(formatPRForResponse);

    const totalMatches = Math.min(searchResult.data.total_count, 1000);
    const totalPages = Math.min(Math.ceil(totalMatches / perPage), 10);
    const clampedPage = Math.min(currentPage, Math.max(1, totalPages));
    const hasMore = clampedPage < totalPages;

    return {
      pull_requests: formattedPRs,
      total_count: searchResult.data.total_count,
      ...(searchResult.data.incomplete_results && { incomplete_results: true }),
      pagination: {
        currentPage: clampedPage,
        totalPages,
        perPage,
        totalMatches,
        hasMore,
      },
      rawResponseChars:
        countSerializedChars(searchResult.data) + transformedRawResponseChars,
    };
  } catch (error: unknown) {
    // A 422 that names a nonexistent searchable entity (e.g. author:ghost) is
    // semantically "no matches", not a failure — degrade to a clean empty.
    if (isNoResultsSearchError(error)) {
      return createPullRequestEmptyResult(params);
    }
    const apiError = handleGitHubAPIError(error);
    await logSessionError(
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      SEARCH_ERRORS.PULL_REQUEST_SEARCH_FAILED.code
    );
    return createPullRequestErrorResult(
      apiError,
      SEARCH_ERRORS.PULL_REQUEST_SEARCH_FAILED.message(apiError.error),
      ['Verify authentication and search parameters']
    );
  }
}

async function searchPullRequestsWithREST(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  params: GitHubPullRequestsSearchParams
): Promise<GitHubPullRequestSearchApiResult> {
  try {
    const owner = params.owner as string;
    const repo = params.repo as string;

    const perPage = Math.min(params.limit || 30, 100);
    const currentPage = params.page || 1;

    const result = await octokit.rest.pulls.list({
      owner,
      repo,
      state: params.state || 'open',
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
    const formattedPRs = transformedPRs.map(formatPRForResponse);

    const hasMore = result.data.length === perPage;

    return {
      pull_requests: formattedPRs,
      total_count: formattedPRs.length,
      pagination: {
        currentPage,
        totalPages: hasMore ? currentPage + 1 : currentPage,
        perPage,
        totalMatches: formattedPRs.length,
        hasMore,
      },
      rawResponseChars:
        countSerializedChars(result.data) + transformedRawResponseChars,
    };
  } catch (error: unknown) {
    if (isNoResultsSearchError(error)) {
      return createPullRequestEmptyResult(params);
    }
    const apiError = handleGitHubAPIError(error);
    await logSessionError(
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      SEARCH_ERRORS.PULL_REQUEST_LIST_FAILED.code
    );
    return createPullRequestErrorResult(
      apiError,
      SEARCH_ERRORS.PULL_REQUEST_LIST_FAILED.message(apiError.error),
      ['Verify repository access and authentication']
    );
  }
}
