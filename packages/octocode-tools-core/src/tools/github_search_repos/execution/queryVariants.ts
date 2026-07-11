import type { z } from 'zod';
import type { GitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { RepoSearchResult as ProviderRepoSearchResult } from '../../../providers/types.js';
import type { ProviderOperationResult } from '../../providerExecution.js';
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
};

export type PartialReposSearchQuery =
  WithOptionalMeta<GitHubReposSearchSingleQuery> & RepositorySearchExtraFields;

export type RepoSearchVariantLabel = 'combined' | 'topics' | 'keywords';

export interface RepoSearchVariant {
  label: RepoSearchVariantLabel;
  query: PartialReposSearchQuery;
}

export interface RepoSearchVariantExecution {
  label: RepoSearchVariantLabel;
  query: PartialReposSearchQuery;
  response: ProviderOperationResult<
    RepoSearchVariant,
    ProviderRepoSearchResult
  >['response'];
}

export type SuccessfulRepoSearchVariant = RepoSearchVariantExecution & {
  response: Extract<
    ProviderOperationResult<RepoSearchVariant, ProviderRepoSearchResult>,
    { response: { data: ProviderRepoSearchResult } }
  >['response'] & {
    data: ProviderRepoSearchResult;
  };
};

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
    query.created ||
    query.updated ||
    query.size ||
    query.forks ||
    query.license ||
    query.goodFirstIssues ||
    query.visibility ||
    query.archived !== undefined
  );
}

function createSearchReasoning(
  originalReasoning: string | undefined,
  searchType: 'topics' | 'keywords'
): string {
  const suffix =
    searchType === 'topics' ? 'topics-based search' : 'keywords-based search';
  return originalReasoning
    ? `${originalReasoning} (${suffix})`
    : `${searchType.charAt(0).toUpperCase() + searchType.slice(1)}-based repository search`;
}

export function createSearchVariants(
  query: PartialReposSearchQuery
): RepoSearchVariant[] {
  const hasTopics = hasValidTopics(query);
  const hasKeywords = hasValidKeywords(query);

  if (hasTopics && hasKeywords) {
    const { topicsToSearch, keywords, ...baseQuery } = query;
    return [
      {
        label: 'topics',
        query: {
          ...baseQuery,
          reasoning: createSearchReasoning(query.reasoning, 'topics'),
          topicsToSearch,
        },
      },
      {
        label: 'keywords',
        query: {
          ...baseQuery,
          reasoning: createSearchReasoning(query.reasoning, 'keywords'),
          keywords,
        },
      },
    ];
  }

  return [{ label: 'combined', query }];
}
