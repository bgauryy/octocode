import type { z } from 'zod';
import type { GitHubReposSearchSingleQuerySchema } from '../../../toolContract/input/resources/tools/githubRepositoriesOperation.js';
import type { WithOptionalMeta } from '../../../types/execution.js';

type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;

export type RepositorySearchExtraFields = {
  archived?: boolean;
  visibility?: 'public' | 'private';
  forks?: string;
  license?: string;
  goodFirstIssues?: string;
  created?: string;
  size?: string;
};

export type PartialReposSearchQuery =
  WithOptionalMeta<GitHubReposSearchSingleQuery> & RepositorySearchExtraFields;

export function hasValidTopics(query: PartialReposSearchQuery): boolean {
  return Boolean(
    query.topicsToSearch &&
    (Array.isArray(query.topicsToSearch)
      ? query.topicsToSearch.length > 0
      : query.topicsToSearch)
  );
}

export function hasValidKeywords(query: PartialReposSearchQuery): boolean {
  return Boolean(query.keywords && query.keywords.length > 0);
}

export function hasValidRepositorySearchParams(
  query: PartialReposSearchQuery
): boolean {
  return Boolean(
    hasValidKeywords(query) ||
    hasValidTopics(query) ||
    query.owner ||
    query.language ||
    query.stars ||
    query.updated ||
    query.created ||
    query.size ||
    query.forks ||
    query.license ||
    query.goodFirstIssues ||
    query.visibility ||
    query.archived !== undefined
  );
}
