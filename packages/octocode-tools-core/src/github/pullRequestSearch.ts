import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../config.js';
import type {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  IssueSearchResultItem,
} from './githubAPI.js';
import type { GitHubPullRequestSearchApiResult } from '../tools/github_search_pull_requests/types.js';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import {
  getOctokit,
  OctokitWithThrottling,
  resolveCacheAuthFingerprint,
} from './client.js';
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
import { transformPullRequestItemFromSearch } from './prContentFetcher.js';
import { fetchGitHubPullRequestByNumberAPIInternal } from './prByNumber.js';
import {
  createPullRequestEmptyResult,
  createPullRequestErrorResult,
  searchPullRequestsWithREST,
} from './pullRequestSearch/restSearch.js';

async function resolveCanonicalSearchRepo(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  params: GitHubPullRequestsSearchParams
): Promise<GitHubPullRequestsSearchParams> {
  if (
    !params.owner ||
    !params.repo ||
    Array.isArray(params.owner) ||
    Array.isArray(params.repo)
  ) {
    return params;
  }

  try {
    const response = await octokit.rest.repos.get({
      owner: params.owner,
      repo: params.repo,
    });
    const [owner, repo] = response.data.full_name?.split('/') ?? [];
    if (!owner || !repo) return params;
    if (owner === params.owner && repo === params.repo) return params;
    return { ...params, owner, repo };
  } catch {
    return params;
  }
}

// Every field here must cover every param that affects the built search
// query (see PullRequestQueryBuilder in queryBuilders.ts) plus the pagination
// knobs — omitting a query-affecting field lets two different searches collide
// on one cache entry and serve stale results.
export function buildPullRequestSearchCacheKey(
  params: GitHubPullRequestsSearchParams,
  sessionId?: string,
  authFingerprint: string = 'anon'
): string {
  return generateCacheKey(
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
      milestone: params.milestone,
      language: params.language,
      checks: params.checks,
      review: params.review,
      locked: params.locked,
      visibility: params.visibility,
      'team-mentions': params['team-mentions'],
      project: params.project,
      archived: params.archived,
      'no-assignee': params['no-assignee'],
      'no-label': params['no-label'],
      'no-milestone': params['no-milestone'],
      'no-project': params['no-project'],
      match: params.match,
      sort: params.sort,
      order: params.order,
      limit: params.limit,
      page: params.page,
      content: params.content,
      reviewMode: params.reviewMode,
      filePage: params.filePage,
      commentPage: params.commentPage,
      commitPage: params.commitPage,
      itemsPerPage: params.itemsPerPage,
      auth: authFingerprint,
    },
    sessionId
  );
}

export async function searchGitHubPullRequestsAPI(
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubPullRequestSearchApiResult> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = buildPullRequestSearchCacheKey(params, sessionId, auth);

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

    const searchParams = await resolveCanonicalSearchRepo(octokit, params);
    const searchQuery = buildPullRequestSearchQuery(searchParams);

    if (!searchQuery) {
      return {
        pull_requests: [],
        total_count: 0,
        error: SEARCH_ERRORS.NO_VALID_PARAMETERS.message,
        hints: ['Provide search query or filters like owner/repo'],
      };
    }

    const sortValue =
      searchParams.sort && searchParams.sort !== 'best-match'
        ? searchParams.sort
        : undefined;

    const perPage = Math.min(
      searchParams.limit || GITHUB_SEARCH_DEFAULT_LIMIT,
      GITHUB_SEARCH_MAX_LIMIT
    );
    const currentPage = searchParams.page || 1;

    const effectiveQuery = searchQuery;

    const searchResult = await octokit.rest.search.issuesAndPullRequests({
      q: searchQuery,
      sort: sortValue as
        'comments' | 'reactions' | 'created' | 'updated' | undefined,
      order: params.order || 'desc',
      per_page: perPage,
      page: currentPage,
    });

    const pullRequests = (searchResult.data.items?.filter(
      (item: IssueSearchResultItem) => !!item.pull_request
    ) || []) as IssueSearchResultItem[];

    const transformedPRs: GitHubPullRequestItem[] = await Promise.all(
      pullRequests.map(async (item: IssueSearchResultItem) => {
        return await transformPullRequestItemFromSearch(
          item,
          searchParams,
          octokit
        );
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
        charOffset: searchParams.charOffset,
        charLength: searchParams.charLength,
      })
    );

    const reportedTotalMatches = searchResult.data.total_count;
    const totalMatches = Math.min(reportedTotalMatches, 1000);
    const totalPages = Math.min(Math.ceil(totalMatches / perPage), 10);
    const clampedPage = Math.min(currentPage, Math.max(1, totalPages));
    const hasMore = clampedPage < totalPages;
    const reachableTotalMatches = Math.min(totalMatches, totalPages * perPage);

    return {
      pull_requests: formattedPRs,
      total_count: searchResult.data.total_count,
      effectiveQuery,
      ...(searchResult.data.incomplete_results && { incomplete_results: true }),
      pagination: {
        currentPage: clampedPage,
        totalPages,
        perPage,
        totalMatches,
        reportedTotalMatches,
        reachableTotalMatches,
        totalMatchesKind: 'reported',
        totalMatchesCapped: reportedTotalMatches > totalMatches,
        hasMore,
        ...(hasMore ? { nextPage: clampedPage + 1 } : {}),
      },
      rawResponseChars:
        countSerializedChars(searchResult.data) + transformedRawResponseChars,
    };
  } catch (error: unknown) {
    if (isNoResultsSearchError(error)) {
      return createPullRequestEmptyResult(params);
    }
    const apiError = handleGitHubAPIError(error);
    return createPullRequestErrorResult(
      apiError,
      SEARCH_ERRORS.PULL_REQUEST_SEARCH_FAILED.message(apiError.error),
      ['Verify authentication and search parameters']
    );
  }
}
