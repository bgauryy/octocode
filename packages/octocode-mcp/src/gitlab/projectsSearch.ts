/**
 * GitLab Projects Search
 *
 * Search for projects/repositories in GitLab.
 * Note: Some filters (stars, created date) require client-side filtering.
 *
 * @module gitlab/projectsSearch
 */

import type {
  GitLabAPIResponse,
  GitLabProjectsSearchQuery,
  GitLabProject,
} from './types.js';
import { getGitlab } from './client.js';
import { handleGitLabAPIError, createGitLabError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import { isGitLabProject, parseGitLabArray } from './responseGuards.js';

/**
 * Projects search result.
 */
export interface GitLabProjectsSearchResult {
  projects: GitLabProject[];
  pagination: {
    currentPage: number;
    totalPages?: number;
    perPage: number;
    totalMatches?: number;
    hasMore: boolean;
  };
}

/**
 * Search for GitLab projects.
 *
 * @param params - Search parameters
 * @param sessionId - Optional session ID for caching
 * @returns Projects search results
 */
export async function searchGitLabProjectsAPI(
  params: GitLabProjectsSearchQuery,
  sessionId?: string
): Promise<GitLabAPIResponse<GitLabProjectsSearchResult>> {
  // Generate cache key
  const cacheKey = generateCacheKey(
    'gl-api-projects',
    {
      search: params.search,
      topic: params.topic,
      visibility: params.visibility,
      owned: params.owned,
      starred: params.starred,
      archived: params.archived,
      orderBy: params.orderBy,
      sort: params.sort,
      perPage: params.perPage,
      page: params.page,
    },
    sessionId
  );

  return withDataCache<GitLabAPIResponse<GitLabProjectsSearchResult>>(
    cacheKey,
    async () => searchGitLabProjectsAPIInternal(params),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function searchGitLabProjectsAPIInternal(
  params: GitLabProjectsSearchQuery
): Promise<GitLabAPIResponse<GitLabProjectsSearchResult>> {
  try {
    const gitlab = await getGitlab();

    const perPage = Math.min(params.perPage || 20, 100);
    const page = params.page || 1;

    const queryOptions: Record<string, unknown> = {
      search: params.search,
      topic: params.topic,
      visibility: params.visibility,
      owned: params.owned,
      starred: params.starred,
      archived: params.archived,
      orderBy: params.orderBy || 'star_count',
      sort: params.sort || 'desc',
      perPage,
      page,
    };

    Object.keys(queryOptions).forEach(key => {
      if (queryOptions[key] === undefined) {
        delete queryOptions[key];
      }
    });

    const rawProjects = parseGitLabArray(
      await gitlab.Projects.all(queryOptions),
      isGitLabProject
    );
    if (!rawProjects) {
      return createGitLabError(
        'Unexpected GitLab projects response shape',
        502
      );
    }
    let projects = rawProjects;

    // Apply client-side filters (GitLab API doesn't support these)
    if (params.minStars !== undefined || params.maxStars !== undefined) {
      projects = projects.filter(project => {
        if (
          params.minStars !== undefined &&
          project.star_count < params.minStars
        ) {
          return false;
        }
        if (
          params.maxStars !== undefined &&
          project.star_count > params.maxStars
        ) {
          return false;
        }
        return true;
      });
    }

    if (params.createdAfter || params.createdBefore) {
      projects = projects.filter(project => {
        const createdAt = new Date(project.created_at);
        if (params.createdAfter && createdAt < new Date(params.createdAfter)) {
          return false;
        }
        if (
          params.createdBefore &&
          createdAt > new Date(params.createdBefore)
        ) {
          return false;
        }
        return true;
      });
    }

    const hasMore = projects.length === perPage;

    return {
      data: {
        projects,
        pagination: {
          currentPage: page,
          totalPages: hasMore ? page + 1 : page,
          perPage,
          totalMatches: projects.length,
          hasMore,
        },
      },
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

/**
 * Get a single GitLab project by ID or path.
 *
 * @param projectId - Project ID (numeric) or path (URL-encoded)
 * @returns Project details
 */
export async function getGitLabProject(
  projectId: number | string
): Promise<GitLabAPIResponse<GitLabProject>> {
  try {
    const gitlab = await getGitlab();
    const project = await gitlab.Projects.show(projectId);
    if (!isGitLabProject(project)) {
      return createGitLabError('Unexpected GitLab project response shape', 502);
    }

    return {
      data: project,
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

/**
 * Transform GitLab project to unified format.
 */
export function transformGitLabProject(project: GitLabProject): {
  id: string;
  name: string;
  fullPath: string;
  description: string | null;
  url: string;
  cloneUrl: string;
  defaultBranch: string;
  stars: number;
  forks: number;
  visibility: string;
  topics: string[];
  createdAt: string;
  updatedAt: string;
  lastActivityAt: string;
} {
  return {
    id: String(project.id),
    name: project.name,
    fullPath: project.path_with_namespace,
    description: project.description,
    url: project.web_url,
    cloneUrl: project.http_url_to_repo,
    defaultBranch: project.default_branch,
    stars: project.star_count,
    forks: project.forks_count,
    visibility: project.visibility,
    topics: project.topics || project.tag_list || [],
    createdAt: project.created_at,
    updatedAt: project.updated_at,
    lastActivityAt: project.last_activity_at,
  };
}
