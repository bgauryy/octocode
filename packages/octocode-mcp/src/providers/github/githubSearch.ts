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

import type { z } from 'zod';
import type {
  GitHubCodeSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import type {
  GitHubRepositoryOutput,
  GitHubSearchRepositoriesData,
} from '@octocodeai/octocode-core/extra-types';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
import type { OptimizedCodeSearchResult } from '../../github/githubAPI.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

import {
  createGitHubProviderError,
  createGitHubProviderErrorFromResult,
  parseGitHubProjectId,
} from './utils.js';
export { parseGitHubProjectId } from './utils.js';

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
      entriesPerPage: (data.pagination as { perPage?: number } | undefined)
        ?.perPage,
    },
    repositoryContext: data._researchContext?.repositoryContext,
    nonExistentScope: data.nonExistentScope,
  };
}

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
      entriesPerPage: (data.pagination as { perPage?: number } | undefined)
        ?.perPage,
    },
    nonExistentScope: (data as { nonExistentScope?: boolean }).nonExistentScope,
  };
}

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
    language: query.language,
    match: query.match,
    archived: query.archived,
    sort:
      query.sort === 'best-match'
        ? undefined
        : (query.sort as 'stars' | 'forks' | 'updated' | undefined),
    limit: query.limit,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  } as GitHubReposSearchSingleQuery & { archived?: boolean };

  const result = await searchGitHubReposAPI(githubQuery, authInfo);

  if ('error' in result) {
    return (
      createGitHubProviderErrorFromResult(result) ?? {
        error: 'Unknown GitHub API error',
        status: 500,
        provider: 'github',
      }
    );
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
