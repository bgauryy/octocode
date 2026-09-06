import type { RepoSearchResult as ProviderRepoSearchResult } from '../../providers/providerResults.js';
import type { z } from 'zod';
import type { GitHubReposSearchSingleQuerySchema } from '../../toolContract/input/resources/tools/githubRepositoriesOperation.js';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import type { WithOptionalMeta } from '../../types/execution.js';

import { splitRepositoryPath } from './shared.js';

type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;

export function mapRepoSearchToolQuery(
  query: WithOptionalMeta<GitHubReposSearchSingleQuery>
) {
  const extra = query as Record<string, unknown>;
  return {
    keywords: query.keywords,
    topics: query.topicsToSearch,
    owner: query.owner,
    stars: query.stars,
    size: extra.size as string | undefined,
    created: extra.created as string | undefined,
    updated: query.updated,
    language: query.language,
    archived: extra.archived as boolean | undefined,
    visibility: extra.visibility as 'public' | 'private' | undefined,
    license: extra.license as string | undefined,
    forks: extra.forks as string | undefined,
    goodFirstIssues: extra.goodFirstIssues as string | undefined,
    match: query.match,
    sort: query.sort,
    limit: (query as Record<string, unknown>).limit as number | undefined,
    page: query.page,
    goal: query.goal,
    reasoning: query.reasoning,
  };
}

export function mapRepoSearchProviderRepositories(
  repositories: ProviderRepoSearchResult['repositories']
): GitHubRepositoryOutput[] {
  return repositories.map(repo => {
    const { owner, repo: repoName } = splitRepositoryPath(repo.fullPath);
    return {
      owner: owner || '',
      repo: repoName || repo.name,
      defaultBranch: repo.defaultBranch,
      stars: repo.stars,
      description: repo.description || '',
      url: repo.url,
      createdAt: repo.createdAt,
      updatedAt: repo.updatedAt,
      pushedAt: repo.lastActivityAt,
      visibility: repo.visibility,
      topics: repo.topics,
      forksCount: repo.forks,
      openIssuesCount: repo.openIssuesCount,
      ...(repo.language && { language: repo.language }),
      ...(repo.license && { license: repo.license }),
      ...(repo.homepage && { homepage: repo.homepage }),
    };
  });
}
