import type {
  SearchReposParameters,
  RepoSearchResultItem,
  GitHubAPIResponse,
} from './githubAPI.js';
import type { z } from 'zod';
import type { GitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';

type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
import type { WithOptionalMeta } from '../types/execution.js';
import { getOctokit } from './client.js';
import { handleGitHubAPIError, isNoResultsSearchError } from './errors.js';
import { buildRepoSearchQuery } from './queryBuilders.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import { SEARCH_ERRORS } from '../errors/domainErrors.js';
import { logSessionError } from '../session.js';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { countSerializedChars } from '../utils/response/charSavings.js';
import { normalizeResponseHeaders } from './responseHeaders.js';

import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
} from '../config.js';

const RAW_API_DEFAULT_LIMIT = GITHUB_SEARCH_DEFAULT_LIMIT;

function extractLicenseHomepage(repo: Record<string, unknown>): {
  license?: string;
  homepage?: string;
} {
  const result: { license?: string; homepage?: string } = {};
  const license = repo.license as Record<string, string> | null | undefined;
  if (license?.spdx_id && license.spdx_id !== 'NOASSERTION') {
    result.license = license.spdx_id;
  }
  const homepage = repo.homepage as string | null | undefined;
  if (homepage) result.homepage = homepage;
  return result;
}

interface RepoSearchPagination {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalMatches: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
  hasMore: boolean;
}

interface RepoSearchAPIData {
  repositories: GitHubRepositoryOutput[];
  pagination?: RepoSearchPagination;

  nonExistentScope?: boolean;
}

export async function searchGitHubReposAPI(
  params: WithOptionalMeta<GitHubReposSearchSingleQuery>,
  authInfo?: AuthInfo,
  sessionId?: string
): Promise<GitHubAPIResponse<RepoSearchAPIData>> {
  const cacheKey = generateCacheKey(
    'gh-api-repos',
    {
      keywords: params.keywords,
      topicsToSearch: params.topicsToSearch,
      owner: params.owner,
      stars: params.stars,
      size: params.size,
      created: params.created,
      updated: params.updated,
      language: (params as Record<string, unknown>).language,
      match: params.match,
      sort: params.sort,
      limit: params.limit,
      page: params.page,
    },
    sessionId
  );

  const result = await withDataCache<GitHubAPIResponse<RepoSearchAPIData>>(
    cacheKey,
    async () => {
      return await searchGitHubReposAPIInternal(params, authInfo);
    },
    {
      shouldCache: value =>
        'data' in value && !(value as { error?: unknown }).error,
    }
  );

  return result;
}

async function listGitHubOrgReposAPIInternal(
  params: {
    owner: string;
    sort?: 'stars' | 'updated';
    limit?: number;
    page?: number;
  },
  octokit: Awaited<ReturnType<typeof getOctokit>>
): Promise<GitHubAPIResponse<RepoSearchAPIData>> {
  const perPage = Math.min(
    params.limit || GITHUB_SEARCH_MAX_LIMIT,
    GITHUB_SEARCH_MAX_LIMIT
  );
  const currentPage = params.page || 1;

  const listSort =
    params.sort === 'updated' ? 'updated' : ('full_name' as const);

  let repoItems: RepoSearchResultItem[];
  let totalCount: number | undefined;

  try {
    const orgResult = await octokit.rest.repos.listForOrg({
      org: params.owner,
      per_page: perPage,
      page: currentPage,
      sort: listSort,
    });
    repoItems = orgResult.data as RepoSearchResultItem[];
    totalCount = undefined;
  } catch {
    try {
      const userResult = await octokit.rest.repos.listForUser({
        username: params.owner,
        per_page: perPage,
        page: currentPage,
        sort: listSort,
      });
      repoItems = userResult.data as RepoSearchResultItem[];
      totalCount = undefined;
    } catch (err: unknown) {
      return handleGitHubAPIError(err);
    }
  }

  const repositories = repoItems.map((repo: RepoSearchResultItem) => {
    const fullName = repo.full_name;
    const parts = fullName.split('/');
    const owner = parts[0] || '';
    const repoName = parts[1] || '';
    return {
      owner,
      repo: repoName,
      defaultBranch: repo.default_branch,
      stars: repo.stargazers_count || 0,
      description: repo.description
        ? repo.description.length > 150
          ? repo.description.substring(0, 150) + '...'
          : repo.description
        : 'No description',
      url: repo.html_url,
      createdAt: repo.created_at,
      updatedAt: repo.updated_at,
      pushedAt: repo.pushed_at,
      visibility: repo.visibility,
      ...(repo.topics && repo.topics.length > 0 && { topics: repo.topics }),
      ...(repo.forks_count &&
        repo.forks_count > 0 && { forksCount: repo.forks_count }),
      ...(repo.open_issues_count &&
        repo.open_issues_count > 0 && {
          openIssuesCount: repo.open_issues_count,
        }),
      ...(repo.language && { language: repo.language }),
      ...extractLicenseHomepage(repo as unknown as Record<string, unknown>),
    };
  });

  const fetchedCount = repositories.length;
  const hasMore = fetchedCount === perPage; // full page → there may be more
  const seenThroughPage = (currentPage - 1) * perPage + fetchedCount;
  const totalMatches = totalCount ?? seenThroughPage + (hasMore ? 1 : 0);
  const totalMatchesKind =
    totalCount !== undefined || !hasMore ? 'exact' : 'lowerBound';

  return {
    data: {
      repositories: repositories as GitHubRepositoryOutput[],
      pagination: {
        currentPage,
        totalPages: hasMore ? currentPage + 1 : currentPage,
        perPage,
        totalMatches,
        reachableTotalMatches: seenThroughPage,
        totalMatchesKind,
        hasMore,
      },
    },
    status: 200,
    rawResponseChars: countSerializedChars(repoItems),
  };
}

async function searchGitHubReposAPIInternal(
  params: WithOptionalMeta<GitHubReposSearchSingleQuery>,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<RepoSearchAPIData>> {
  try {
    const octokit = await getOctokit(authInfo);

    const hasSearchTerms =
      (params.keywords?.length ?? 0) > 0 ||
      (params.topicsToSearch?.length ?? 0) > 0;

    const ownerParam =
      typeof params.owner === 'string'
        ? params.owner
        : Array.isArray(params.owner)
          ? params.owner[0]
          : undefined;

    if (!hasSearchTerms && ownerParam) {
      return await listGitHubOrgReposAPIInternal(
        {
          owner: ownerParam,
          sort: params.sort as 'stars' | 'updated' | undefined,
          limit: params.limit,
          page: params.page,
        },
        octokit
      );
    }

    const query = buildRepoSearchQuery(params);

    if (!query.trim()) {
      await logSessionError(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        SEARCH_ERRORS.QUERY_EMPTY.code
      );
      return {
        error: SEARCH_ERRORS.QUERY_EMPTY.message,
        type: 'http',
        status: 400,
      };
    }

    const perPage = Math.min(
      params.limit || RAW_API_DEFAULT_LIMIT,
      GITHUB_SEARCH_MAX_LIMIT
    );
    const currentPage = params.page || 1;

    const searchParams: SearchReposParameters = {
      q: query,
      per_page: perPage,
      page: currentPage,
    };

    const API_SORTS = [
      'stars',
      'forks',
      'help-wanted-issues',
      'updated',
    ] as const;
    if (params.sort && (API_SORTS as readonly string[]).includes(params.sort)) {
      searchParams.sort = params.sort as SearchReposParameters['sort'];
    }

    const result = await octokit.rest.search.repos(searchParams);

    const repositories = result.data.items.map((repo: RepoSearchResultItem) => {
      const fullName = repo.full_name;
      const parts = fullName.split('/');
      const owner = parts[0] || '';
      const repoName = parts[1] || '';

      return {
        owner,
        repo: repoName,
        defaultBranch: repo.default_branch,
        stars: repo.stargazers_count || 0,
        description: repo.description
          ? repo.description.length > 150
            ? repo.description.substring(0, 150) + '...'
            : repo.description
          : 'No description',
        url: repo.html_url,
        createdAt: repo.created_at,
        updatedAt: repo.updated_at,
        pushedAt: repo.pushed_at,
        visibility: repo.visibility,
        ...(repo.topics && repo.topics.length > 0 && { topics: repo.topics }),
        ...(repo.forks_count &&
          repo.forks_count > 0 && {
            forksCount: repo.forks_count,
          }),
        ...(repo.open_issues_count &&
          repo.open_issues_count > 0 && {
            openIssuesCount: repo.open_issues_count,
          }),
        ...(repo.language && { language: repo.language }),
        ...extractLicenseHomepage(repo as unknown as Record<string, unknown>),
      };
    });

    const reportedTotalMatches = result.data.total_count;
    const totalMatches = Math.min(reportedTotalMatches, 1000);
    const totalPages = Math.min(Math.ceil(totalMatches / perPage), 10);
    const clampedPage = Math.min(currentPage, Math.max(1, totalPages));
    const hasMore = clampedPage < totalPages;
    const reachableTotalMatches = Math.min(totalMatches, totalPages * perPage);

    return {
      data: {
        repositories: repositories as GitHubRepositoryOutput[],
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
        },
      },
      status: 200,
      headers: normalizeResponseHeaders(result.headers),
      rawResponseChars: countSerializedChars(result.data),
    };
  } catch (error: unknown) {
    if (isNoResultsSearchError(error)) {
      const perPage = Math.min(
        params.limit || RAW_API_DEFAULT_LIMIT,
        GITHUB_SEARCH_MAX_LIMIT
      );
      return {
        data: {
          repositories: [],
          nonExistentScope: true,
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
        },
        status: 200,
        rawResponseChars: 0,
      };
    }
    return handleGitHubAPIError(error);
  }
}
