import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { resolveCacheAuthFingerprint } from '../client.js';
import { handleGitHubAPIError, isNoResultsSearchError } from '../errors.js';
import type { GitHubAPIResponse } from '../githubAPI.js';
import {
  shouldUseSearchForIssues,
  type IssueSearchParams,
} from '../queryBuilders.js';
import { withDataCache } from '../../utils/http/cache.js';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../../config.js';
import type { FetchIssuesParams, IssuesResult } from './types.js';
import {
  buildIssueSearchCacheKey,
  combineQuery,
  createIssueError,
  firstString,
} from './helpers.js';
import { fetchIssueByNumber, listIssues, searchIssues } from './fetchers.js';

export async function fetchIssues(
  params: FetchIssuesParams,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<IssuesResult>> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = buildIssueSearchCacheKey(params, sessionId, auth);
  return withDataCache<GitHubAPIResponse<IssuesResult>>(
    cacheKey,
    () => fetchIssuesInternal(params, authInfo),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function fetchIssuesInternal(
  params: FetchIssuesParams,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<IssuesResult>> {
  try {
    if (params.state === 'merged') {
      return createIssueError(
        'state:"merged" is not valid for type:"issues" — use "open" or "closed".',
        ['For merged PRs use type:"prs" with state:"merged".']
      );
    }

    if (params.issueNumber != null) {
      return await fetchIssueByNumber(params, authInfo);
    }

    const searchParams: IssueSearchParams = {
      ...params,
      query: combineQuery(params),
    };

    if (shouldUseSearchForIssues(searchParams)) {
      return await searchIssues(searchParams, params, authInfo);
    }

    return await listIssues(params, authInfo);
  } catch (error) {
    if (isNoResultsSearchError(error)) {
      return {
        data: {
          type: 'issues',
          owner: firstString(params.owner) ?? '',
          repo: firstString(params.repo) ?? '',
          issues: [],
          total_count: 0,
          pagination: {
            currentPage: params.page ?? 1,
            perPage: Math.min(
              params.limit ?? GITHUB_SEARCH_DEFAULT_LIMIT,
              GITHUB_SEARCH_MAX_LIMIT
            ),
            hasMore: false,
          },
        },
        status: 200,
      };
    }
    return handleGitHubAPIError(error);
  }
}
