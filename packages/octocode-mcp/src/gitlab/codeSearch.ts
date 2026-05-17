/**
 * GitLab Code Search
 *
 * Search for code within GitLab projects.
 * Note: Global/group search requires GitLab Premium.
 *
 * @module gitlab/codeSearch
 */

import type {
  GitLabAPIResponse,
  GitLabCodeSearchQuery,
  GitLabCodeSearchResult,
  GitLabCodeSearchItem,
} from './types.js';
import { getGitlab } from './client.js';
import { handleGitLabAPIError, createGitLabError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  hasGitLabSearchApi,
  isGitLabCodeSearchItem,
  parseGitLabArray,
} from './responseGuards.js';

interface GitLabCodeSearchOptions {
  searchText: string;
  path?: string;
  filename?: string;
  extension?: string;
}

function buildCodeSearchFilter(
  filterName: 'path' | 'filename' | 'extension',
  value?: string
): string | undefined {
  if (!value) {
    return undefined;
  }

  const normalized = value.trim();
  return normalized ? `${filterName}:${normalized}` : undefined;
}

function buildGitLabSearchQuery({
  searchText,
  path,
  filename,
  extension,
}: GitLabCodeSearchOptions): string {
  const terms = [searchText.trim()];
  const pathTerm = buildCodeSearchFilter('path', path);
  const filenameTerm = buildCodeSearchFilter('filename', filename);
  const extensionTerm = buildCodeSearchFilter('extension', extension);

  if (pathTerm) terms.push(pathTerm);
  if (filenameTerm) terms.push(filenameTerm);
  if (extensionTerm) terms.push(extensionTerm);

  return terms.join(' ');
}

/**
 * Search for code in GitLab.
 *
 * @param params - Search parameters
 * @param sessionId - Optional session ID for caching
 * @returns Search results
 */
export async function searchGitLabCodeAPI(
  params: GitLabCodeSearchQuery,
  sessionId?: string
): Promise<GitLabAPIResponse<GitLabCodeSearchResult>> {
  // Validate required parameters
  if (!params.search || !params.search.trim()) {
    return createGitLabError('Search query is required', 400);
  }

  if (!params.projectId && !params.groupId) {
    return createGitLabError(
      'Project ID or Group ID is required for GitLab code search',
      400,
      ['Global code search requires GitLab Premium tier.']
    );
  }

  // Generate cache key
  const cacheKey = generateCacheKey(
    'gl-api-code',
    {
      search: params.search,
      projectId: params.projectId,
      groupId: params.groupId,
      searchType: params.searchType,
      path: params.path,
      filename: params.filename,
      extension: params.extension,
      ref: params.ref,
      perPage: params.perPage,
      page: params.page,
    },
    sessionId
  );

  return withDataCache<GitLabAPIResponse<GitLabCodeSearchResult>>(
    cacheKey,
    async () => searchGitLabCodeAPIInternal(params),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function searchGitLabCodeAPIInternal(
  params: GitLabCodeSearchQuery
): Promise<GitLabAPIResponse<GitLabCodeSearchResult>> {
  try {
    const gitlab = await getGitlab();

    const perPage = Math.min(params.perPage || 20, 100);
    const page = params.page || 1;

    const searchQuery = buildGitLabSearchQuery({
      searchText: params.search,
      path: params.path,
      filename: params.filename,
      extension: params.extension,
    });

    if (!hasGitLabSearchApi(gitlab)) {
      return createGitLabError('GitLab search API is unavailable', 500);
    }

    const search = gitlab.Search;
    const searchOptions = {
      perPage,
      page,
    } as Record<string, unknown>;
    const scope: Record<string, unknown> = {};

    if (params.searchType) {
      searchOptions.searchType = params.searchType;
    }

    if (params.projectId) {
      scope.projectId = params.projectId;
      if (params.ref) {
        searchOptions.ref = params.ref;
      }
    } else if (params.groupId) {
      scope.groupId = params.groupId;
    }

    const results = await search.all('blobs', searchQuery, {
      ...scope,
      ...searchOptions,
    });

    const items = parseGitLabArray(results, isGitLabCodeSearchItem);
    if (!items) {
      return createGitLabError(
        'Unexpected GitLab code search response shape',
        502
      );
    }

    const hasMore = items.length === perPage;

    return {
      data: {
        items,
        totalCount: items.length,
        pagination: {
          currentPage: page,
          totalPages: hasMore ? page + 1 : page,
          perPage,
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
 * Transform GitLab code search results to unified format.
 */
export function transformGitLabCodeSearchItem(
  item: GitLabCodeSearchItem,
  projectPath?: string
): {
  path: string;
  content: string;
  lineNumber: number;
  repository: {
    id: string;
    name: string;
  };
} {
  return {
    path: item.path,
    content: item.data,
    lineNumber: item.startline,
    repository: {
      id: String(item.project_id),
      name: projectPath || String(item.project_id),
    },
  };
}
