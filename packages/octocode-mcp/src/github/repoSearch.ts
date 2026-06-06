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

async function searchGitHubReposAPIInternal(
  params: WithOptionalMeta<GitHubReposSearchSingleQuery>,
  authInfo?: AuthInfo
): Promise<GitHubAPIResponse<RepoSearchAPIData>> {
  try {
    const octokit = await getOctokit(authInfo);
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
