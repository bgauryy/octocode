import type { RepoSearchResult as ProviderRepoSearchResult } from '../../providers/types.js';
import type { z } from 'zod';
import type { GitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
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
    size: query.size,
    created: query.created,
    updated: query.updated,
    language: query.language,
    archived: extra.archived as boolean | undefined,
    visibility: extra.visibility as 'public' | 'private' | undefined,
    forks: extra.forks as string | undefined,
    license: extra.license as string | undefined,
    goodFirstIssues: extra.goodFirstIssues as string | undefined,
    match: query.match,
    sort: query.sort as
      'stars' | 'forks' | 'updated' | 'created' | 'best-match' | undefined,
    limit: (query as Record<string, unknown>).limit as number | undefined,
    page: query.page,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
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
    };
  });
}
