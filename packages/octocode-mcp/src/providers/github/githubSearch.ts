/**
 * GitHub Code Search and Repository Search
 *
 * Extracted from GitHubProvider for better modularity.
 *
 * @module providers/github/githubSearch
 */

import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ProviderResponse,
  CodeSearchQuery,
  CodeSearchResult,
  CodeSearchItem,
  RepoSearchQuery,
  RepoSearchResult,
  UnifiedRepository,
} from '../types.js';

import { searchGitHubCodeAPI } from '../../github/codeSearch.js';
import { searchGitHubReposAPI } from '../../github/repoSearch.js';

import type {
  GitHubCodeSearchQuery,
  GitHubReposSearchQuery,
  GitHubRepositoryOutput,
  GitHubSearchRepositoriesData,
} from '@octocodeai/octocode-core';
import type { OptimizedCodeSearchResult } from '../../github/githubAPI.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import { createGitHubProviderError, parseGitHubProjectId } from './utils.js';
export { parseGitHubProjectId } from './utils.js';

/**
 * Transform GitHub code search result to unified format.
 */
export function transformCodeSearchResult(
  data: OptimizedCodeSearchResult
): CodeSearchResult {
  const items: CodeSearchItem[] = data.items.map(item => ({
    path: item.path,
    matches: item.matches.map(m => ({
      context: m.context,
      positions: m.positions,
    })),
    url: item.url || '',
    repository: {
      id: item.repository.nameWithOwner,
      name: item.repository.nameWithOwner,
      url: item.repository.url,
    },
    lastModifiedAt: item.lastModifiedAt,
  }));

  return {
    items,
    totalCount: data.total_count,
    pagination: {
      currentPage: data.pagination?.currentPage || 1,
      totalPages: data.pagination?.totalPages || 1,
      hasMore: data.pagination?.hasMore || false,
      totalMatches: data.pagination?.totalMatches,
    },
    repositoryContext: data._researchContext?.repositoryContext,
  };
}

/**
 * Transform GitHub repo search result to unified format.
 */
export function transformRepoSearchResult(
  data: GitHubSearchRepositoriesData
): RepoSearchResult {
  const repositories: UnifiedRepository[] = data.repositories.map(
    (repo: GitHubRepositoryOutput) => ({
      id: `${repo.owner}/${repo.repo}`,
      name: repo.repo,
      fullPath: `${repo.owner}/${repo.repo}`,
      description: repo.description || null,
      url: repo.url,
      cloneUrl: `https://github.com/${repo.owner}/${repo.repo}.git`,
      defaultBranch: repo.defaultBranch || 'main',
      stars: repo.stars || 0,
      forks: repo.forksCount || 0,
      visibility:
        (repo.visibility as 'public' | 'private' | 'internal') || 'public',
      topics: repo.topics || [],
      createdAt: repo.createdAt,
      updatedAt: repo.updatedAt,
      lastActivityAt: repo.pushedAt || repo.updatedAt,
      openIssuesCount: repo.openIssuesCount,
      language: repo.language,
    })
  );

  return {
    repositories,
    totalCount: data.pagination?.totalMatches || repositories.length,
    pagination: {
      currentPage: data.pagination?.currentPage || 1,
      totalPages: data.pagination?.totalPages || 1,
      hasMore: data.pagination?.hasMore || false,
      totalMatches: data.pagination?.totalMatches,
    },
  };
}

/**
 * Search code on GitHub.
 */
export async function searchCode(
  query: CodeSearchQuery,
  authInfo?: AuthInfo,
  parseProjectId: (projectId?: string) => {
    owner?: string;
    repo?: string;
  } = parseGitHubProjectId
): Promise<ProviderResponse<CodeSearchResult>> {
  const { owner: projectOwner, repo } = parseProjectId(query.projectId);
  const owner = projectOwner || query.owner;

  const githubQuery = {
    keywordsToSearch: query.keywords,
    owner,
    repo,
    extension: query.extension,
    filename: query.filename,
    path: query.path,
    match: query.match,
    limit: query.limit,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  } as GitHubCodeSearchQuery;

  const result = await searchGitHubCodeAPI(githubQuery, authInfo);

  if (isGitHubAPIError(result)) {
    return createGitHubProviderError(result);
  }

  if (!result.data) {
    return {
      error: 'No data returned from GitHub API',
      status: 500,
      provider: 'github',
    };
  }

  return {
    data: transformCodeSearchResult(result.data),
    status: 200,
    provider: 'github',
    rawResponseChars:
      result.rawResponseChars ?? countSerializedChars(result.data),
  };
}

/**
 * Search repositories on GitHub.
 */
export async function searchRepos(
  query: RepoSearchQuery,
  authInfo?: AuthInfo
): Promise<ProviderResponse<RepoSearchResult>> {
  const githubQuery = {
    keywordsToSearch: query.keywords,
    topicsToSearch: query.topics,
    owner: query.owner,
    stars: query.stars ?? (query.minStars ? `>=${query.minStars}` : undefined),
    size: query.size,
    created: query.created,
    updated: query.updated,
    match: query.match,
    sort:
      query.sort === 'best-match'
        ? undefined
        : (query.sort as 'stars' | 'forks' | 'updated' | undefined),
    limit: query.limit,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  } as GitHubReposSearchQuery;

  const result = await searchGitHubReposAPI(githubQuery, authInfo);

  if ('error' in result) {
    const errorResult = result as {
      error: string | { toString(): string };
      status?: number;
      hints?: string[];
      rateLimitRemaining?: number;
      rateLimitReset?: number;
      retryAfter?: number;
    };
    return createGitHubProviderError({
      error:
        typeof errorResult.error === 'string'
          ? errorResult.error
          : String(errorResult.error),
      status: errorResult.status || 500,
      hints: errorResult.hints,
      rateLimitRemaining: errorResult.rateLimitRemaining,
      rateLimitReset: errorResult.rateLimitReset,
      retryAfter: errorResult.retryAfter,
    });
  }

  if (!('data' in result) || !result.data) {
    return {
      error: 'No data returned from GitHub API',
      status: 500,
      provider: 'github',
    };
  }

  return {
    data: transformRepoSearchResult(result.data),
    status: 200,
    provider: 'github',
    rawResponseChars:
      result.rawResponseChars ?? countSerializedChars(result.data),
  };
}
