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

const RAW_API_DEFAULT_LIMIT = 30;

interface RepoSearchPagination {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalMatches: number;
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
      keywordsToSearch: params.keywordsToSearch,
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

/**
 * Lists all repositories for an owner (org or user) using the REST listing
 * endpoint. Unlike the search API (capped at 1 000 results), this endpoint
 * paginates without a hard limit, enabling full enumeration of large orgs.
 */
async function listGitHubOrgReposAPIInternal(
  params: {
    owner: string;
    sort?: 'stars' | 'updated';
    limit?: number;
    page?: number;
  },
  octokit: Awaited<ReturnType<typeof getOctokit>>
): Promise<GitHubAPIResponse<RepoSearchAPIData>> {
  const perPage = Math.min(params.limit || 100, 100);
  const currentPage = params.page || 1;

  // Accepted sort values differ between org and user listing endpoints.
  // 'stars' is not a valid listing sort (only search supports it); fall back
  // to 'updated' as the nearest equivalent.
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
    repoItems = orgResult.data as unknown as RepoSearchResultItem[];
    // The listing endpoints don't return a total_count — use the Link header
    // heuristic: if we got a full page there are likely more results.
    totalCount = undefined;
  } catch {
    // Not an org (or no access) — try the user listing endpoint.
    try {
      const userResult = await octokit.rest.repos.listForUser({
        username: params.owner,
        per_page: perPage,
        page: currentPage,
        sort: listSort,
      });
      repoItems = userResult.data as unknown as RepoSearchResultItem[];
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
    };
  });

  const fetchedCount = repositories.length;
  const hasMore = fetchedCount === perPage; // full page → there may be more
  const totalMatches = totalCount ?? fetchedCount + (hasMore ? 1 : 0);

  return {
    data: {
      repositories: repositories as GitHubRepositoryOutput[],
      pagination: {
        currentPage,
        totalPages: hasMore ? currentPage + 1 : currentPage,
        perPage,
        totalMatches,
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

    // Owner-only mode: when the caller supplies just owner (no keywords or
    // topics), use the REST listing endpoint instead of the search API.
    // The search API is capped at 1 000 results and cannot enumerate every
    // repository in a large organisation. repos.listForOrg / listForUser
    // paginates exhaustively with no hard cap.
    const hasSearchTerms =
      (params.keywordsToSearch?.length ?? 0) > 0 ||
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

    const perPage = Math.min(params.limit || RAW_API_DEFAULT_LIMIT, 100);
    const currentPage = params.page || 1;

    const searchParams: SearchReposParameters = {
      q: query,
      per_page: perPage,
      page: currentPage,
    };

    const API_SORTS = ['stars', 'forks', 'updated'] as const;
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
      };
    });

    const totalMatches = Math.min(result.data.total_count, 1000);
    const totalPages = Math.min(Math.ceil(totalMatches / perPage), 10);
    const clampedPage = Math.min(currentPage, Math.max(1, totalPages));
    const hasMore = clampedPage < totalPages;

    return {
      data: {
        repositories: repositories as GitHubRepositoryOutput[],
        pagination: {
          currentPage: clampedPage,
          totalPages,
          perPage,
          totalMatches,
          hasMore,
        },
      },
      status: 200,
      headers: normalizeResponseHeaders(result.headers),
      rawResponseChars: countSerializedChars(result.data),
    };
  } catch (error: unknown) {
    if (isNoResultsSearchError(error)) {
      const perPage = Math.min(params.limit || RAW_API_DEFAULT_LIMIT, 100);
      return {
        data: {
          repositories: [],
          nonExistentScope: true,
          pagination: {
            currentPage: params.page || 1,
            totalPages: 0,
            perPage,
            totalMatches: 0,
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
