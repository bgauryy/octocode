import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';

type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { compareIsoDateDescending } from '../../utils/core/compare.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type RepositorySearchExtraFields = {
  archived?: boolean;
  visibility?: 'public' | 'private';
  forks?: string;
  license?: string;
  goodFirstIssues?: string;
};

type PartialReposSearchQuery = WithOptionalMeta<GitHubReposSearchSingleQuery> &
  RepositorySearchExtraFields;

type RepositoryDetail = {
  owner: string;
  repo: string;
  stars?: number;
  forks?: number;
  openIssuesCount?: number;
  language?: string;
  license?: string;
  description?: string;
  homepage?: string;
  pushedAt?: string;
  createdAt?: string;
  defaultBranch?: string;
  topics?: string[];
  visibility?: string;
  url?: string;
  updatedAt?: string;
};

function buildRepositoryDetail(repo: GitHubRepositoryOutput): RepositoryDetail {
  const r = repo as GitHubRepositoryOutput & {
    license?: string;
    homepage?: string;
  };
  const detail: RepositoryDetail = {
    owner: r.owner ?? '',
    repo: r.repo,
    stars: r.stars,
    forks: r.forksCount,
    openIssuesCount: r.openIssuesCount,
    language: r.language,
    license: r.license || undefined,
    description:
      r.description && r.description !== 'No description'
        ? r.description
        : undefined,
    homepage: r.homepage || undefined,
    pushedAt: r.pushedAt ? r.pushedAt.slice(0, 10) : undefined,
    createdAt: r.createdAt,
    defaultBranch:
      r.defaultBranch &&
      r.defaultBranch !== 'main' &&
      r.defaultBranch !== 'master'
        ? r.defaultBranch
        : undefined,
    topics: r.topics?.length ? r.topics : undefined,
    visibility:
      r.visibility && r.visibility !== 'public' ? r.visibility : undefined,
    url: r.url,
    updatedAt: r.updatedAt,
  };
  return Object.fromEntries(
    Object.entries(detail).filter(([, v]) => v !== undefined)
  ) as RepositoryDetail;
}

export function formatRepoLine(repo: GitHubRepositoryOutput): string {
  const r = repo as GitHubRepositoryOutput & {
    pushedAt?: string;
    visibility?: string;
    topics?: string[];
    forksCount?: number;
    openIssuesCount?: number;
    defaultBranch?: string;
    license?: string;
    homepage?: string;
  };

  const name = `${r.owner ? `${r.owner}/` : ''}${r.repo}`;
  const parts: string[] = [name];

  if (typeof r.stars === 'number') parts.push(`${r.stars} stars`);
  if (typeof r.forksCount === 'number' && r.forksCount > 0)
    parts.push(`${r.forksCount} forks`);
  if (typeof r.openIssuesCount === 'number' && r.openIssuesCount > 0)
    parts.push(`${r.openIssuesCount} issues`);
  if (r.language) parts.push(r.language);
  if (r.license) parts.push(r.license);
  if (r.pushedAt) parts.push(r.pushedAt.slice(0, 10));
  if (
    r.defaultBranch &&
    r.defaultBranch !== 'main' &&
    r.defaultBranch !== 'master'
  )
    parts.push(`@${r.defaultBranch}`);
  if (r.visibility && r.visibility !== 'public') parts.push(r.visibility);
  if (Array.isArray(r.topics) && r.topics.length > 0)
    parts.push(`#${r.topics.slice(0, 4).join(',')}`);
  if (r.description && r.description !== 'No description') {
    const desc = r.description.replace(/\s+/g, ' ').trim();
    parts.push(desc.length > 100 ? `${desc.slice(0, 99)}...` : desc);
  }

  return parts.join(' | ');
}

function buildReposSearchOutput(
  data: { repositories: GitHubRepositoryOutput[]; pagination?: unknown },
  query: PartialReposSearchQuery
): {
  data: {
    repositories: (string | RepositoryDetail)[];
    pagination?: unknown;
  };
} {
  const concise = (query as { concise?: boolean }).concise === true;
  return {
    data: {
      pagination: data.pagination,
      repositories: concise
        ? data.repositories.map(r => `${r.owner ? `${r.owner}/` : ''}${r.repo}`)
        : data.repositories.map(buildRepositoryDetail),
    },
  };
}
import {
  handleCatchError,
  handleProviderError,
  createErrorResult,
  createSuccessResult,
} from '../utils.js';
import type { RepoSearchResult as ProviderRepoSearchResult } from '../../providers/types.js';
import {
  mapRepoSearchProviderRepositories,
  mapRepoSearchToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperations,
  type ProviderOperationResult,
} from '../providerExecution.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

type RepoSearchVariantLabel = 'combined' | 'topics' | 'keywords';

interface RepoSearchVariant {
  label: RepoSearchVariantLabel;
  query: PartialReposSearchQuery;
}

interface RepoSearchVariantExecution {
  label: RepoSearchVariantLabel;
  query: PartialReposSearchQuery;
  response: ProviderOperationResult<
    RepoSearchVariant,
    ProviderRepoSearchResult
  >['response'];
}

type SuccessfulRepoSearchVariant = RepoSearchVariantExecution & {
  response: Extract<
    ProviderOperationResult<RepoSearchVariant, ProviderRepoSearchResult>,
    { response: { data: ProviderRepoSearchResult } }
  >['response'] & {
    data: ProviderRepoSearchResult;
  };
};

function hasValidTopics(query: PartialReposSearchQuery): boolean {
  return Boolean(
    query.topicsToSearch &&
    (Array.isArray(query.topicsToSearch)
      ? query.topicsToSearch.length > 0
      : query.topicsToSearch)
  );
}

function hasValidKeywords(query: PartialReposSearchQuery): boolean {
  return Boolean(query.keywords && query.keywords.length > 0);
}

function hasValidRepositorySearchParams(
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

function createSearchVariants(
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

function deduplicateRepositories(
  repositories: GitHubRepositoryOutput[]
): GitHubRepositoryOutput[] {
  const uniqueRepositories = new Map<string, GitHubRepositoryOutput>();

  for (const repo of repositories) {
    const key = `${repo.owner}/${repo.repo}`;
    if (!uniqueRepositories.has(key)) {
      uniqueRepositories.set(key, repo);
    }
  }

  return [...uniqueRepositories.values()];
}

function rankRepositoriesByRelevance(
  repositories: readonly GitHubRepositoryOutput[],
  query: PartialReposSearchQuery
): GitHubRepositoryOutput[] {
  return [...repositories].sort((left, right) => {
    const requestedSort = compareByRequestedSort(left, right, query.sort);
    if (requestedSort !== 0) return requestedSort;

    const relevanceDelta =
      scoreRepositoryRelevance(right, query) -
      scoreRepositoryRelevance(left, query);
    if (relevanceDelta !== 0) return relevanceDelta;

    const starsDelta = (right.stars ?? 0) - (left.stars ?? 0);
    if (starsDelta !== 0) return starsDelta;

    return repositoryFullName(left).localeCompare(repositoryFullName(right));
  });
}

function compareByRequestedSort(
  left: GitHubRepositoryOutput,
  right: GitHubRepositoryOutput,
  sort: PartialReposSearchQuery['sort']
): number {
  switch (sort) {
    case 'stars':
      return (right.stars ?? 0) - (left.stars ?? 0);
    case 'forks':
      return (right.forksCount ?? 0) - (left.forksCount ?? 0);
    case 'help-wanted-issues':
      return (right.openIssuesCount ?? 0) - (left.openIssuesCount ?? 0);
    case 'updated':
      return compareIsoDateDescending(left.updatedAt, right.updatedAt);
    case 'best-match':
    case undefined:
      return 0;
    default:
      return 0;
  }
}

function scoreRepositoryRelevance(
  repo: GitHubRepositoryOutput,
  query: PartialReposSearchQuery
): number {
  const terms = getRepositorySearchTerms(query);
  const fullName = repositoryFullName(repo).toLowerCase();
  const repoName = repo.repo.toLowerCase();
  const description = (repo.description ?? '').toLowerCase();
  const topics = (repo.topics ?? []).map(topic => topic.toLowerCase());
  const language = repo.language?.toLowerCase();
  const requestedLanguage = query.language?.toLowerCase();

  const termScore = terms.reduce((score, term) => {
    if (repoName === term || fullName === term) return score + 80;
    if (repoName.includes(term) || fullName.includes(term)) return score + 40;
    if (topics.includes(term)) return score + 35;
    if (description.includes(term)) return score + 10;
    return score;
  }, 0);

  return (
    termScore + (requestedLanguage && language === requestedLanguage ? 20 : 0)
  );
}

function getRepositorySearchTerms(
  query: PartialReposSearchQuery
): readonly string[] {
  const keywords = query.keywords ?? [];
  const topics = query.topicsToSearch ?? [];
  return [...keywords, ...topics]
    .map(term => term.trim().toLowerCase())
    .filter(term => term.length > 0);
}

function repositoryFullName(repo: GitHubRepositoryOutput): string {
  return `${repo.owner}/${repo.repo}`;
}

export function buildResultPagination(pagination: {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
}) {
  return {
    currentPage: pagination.currentPage,
    totalPages: pagination.totalPages,
    perPage: pagination.entriesPerPage ?? 10,
    ...(pagination.totalMatches !== undefined
      ? { totalMatches: pagination.totalMatches }
      : {}),
    ...(pagination.reportedTotalMatches !== undefined
      ? { reportedTotalMatches: pagination.reportedTotalMatches }
      : {}),
    ...(pagination.reachableTotalMatches !== undefined
      ? { reachableTotalMatches: pagination.reachableTotalMatches }
      : {}),
    ...(pagination.totalMatchesKind !== undefined
      ? { totalMatchesKind: pagination.totalMatchesKind }
      : {}),
    ...(pagination.totalMatchesCapped !== undefined
      ? { totalMatchesCapped: pagination.totalMatchesCapped }
      : {}),
    hasMore: pagination.hasMore,
    ...(pagination.hasMore ? { nextPage: pagination.currentPage + 1 } : {}),
  };
}

type EffectivePagination = {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
};

export function buildPartialFailureWarnings(
  failedVariants: readonly { label: RepoSearchVariantLabel }[]
): string[] | undefined {
  if (failedVariants.length === 0) return undefined;
  const labels = failedVariants.map(variant => `'${variant.label}'`).join(', ');
  return [
    `Repository search partially failed: the ${labels} query variant(s) returned an error. Results may be incomplete — retry or narrow the query.`,
  ];
}

export function buildMergedPagination(
  variants: SuccessfulRepoSearchVariant[],
  dedupedCount: number
): EffectivePagination | undefined {
  const pages = variants
    .map(variant => variant.response.data.pagination)
    .filter((p): p is NonNullable<typeof p> => Boolean(p));
  if (pages.length === 0) return undefined;

  // The variants are deduplicated before this point, so summing per-variant
  // totals overcounts every repository that appeared in more than one variant.
  // The count of distinct repositories we actually merged is a firm lower
  // bound on the true number of matches; report it as such.
  return {
    currentPage: pages[0]!.currentPage,
    totalPages: Math.max(...pages.map(p => p.totalPages)),
    hasMore: pages.some(p => p.hasMore),
    entriesPerPage: pages[0]!.entriesPerPage,
    totalMatches: dedupedCount,
    reachableTotalMatches: dedupedCount,
    totalMatchesKind: 'lowerBound',
    totalMatchesCapped: pages.some(p => p.totalMatchesCapped === true),
  };
}

function sumVariantRawResponseChars(
  variants: RepoSearchVariantExecution[]
): number {
  return variants.reduce(
    (sum, variant) =>
      sum +
      (variant.response.rawResponseChars ??
        countSerializedChars(variant.response.data ?? variant.response)),
    0
  );
}

export async function searchMultipleGitHubRepos(
  args: ToolExecutionArgs<PartialReposSearchQuery>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialReposSearchQuery, _index: number) => {
      try {
        if (!hasValidRepositorySearchParams(query)) {
          return createErrorResult(
            'At least one repository search term or filter is required.',
            query
          );
        }

        const currentProviderContext = getProviderContext();
        const variants = createSearchVariants(query);
        const { successes, failures } = await executeProviderOperations(
          variants.map(variant => ({
            meta: { label: variant.label, query: variant.query },
            operation: () =>
              currentProviderContext.provider.searchRepos(
                mapRepoSearchToolQuery(variant.query)
              ),
          }))
        );

        const successfulVariants: SuccessfulRepoSearchVariant[] = successes.map(
          success => ({
            label: success.meta.label,
            query: success.meta.query,
            response: success.response,
          })
        );
        const failedVariants: RepoSearchVariantExecution[] = failures.map(
          failure => ({
            label: failure.meta.label,
            query: failure.meta.query,
            response: failure.response,
          })
        );

        if (successfulVariants.length === 0) {
          const firstFailedVariant = failedVariants[0];
          if (!firstFailedVariant) {
            return handleCatchError(
              new Error('Repository search produced no provider results'),
              query,
              undefined,
              TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
            );
          }
          return handleProviderError(firstFailedVariant.response, query);
        }

        const mergedLimit = (query as { limit?: number }).limit;
        const rankedRepositories = rankRepositoriesByRelevance(
          deduplicateRepositories(
            successfulVariants.flatMap(variant =>
              mapRepoSearchProviderRepositories(
                variant.response.data.repositories
              )
            )
          ),
          query
        );
        const repositories =
          mergedLimit != null
            ? rankedRepositories.slice(0, mergedLimit)
            : rankedRepositories;

        const onlySuccessfulVariant =
          successfulVariants.length === 1 ? successfulVariants[0] : undefined;
        const isMergedResult = successfulVariants.length > 1;
        const effectivePagination: EffectivePagination | undefined =
          isMergedResult
            ? buildMergedPagination(
                successfulVariants,
                rankedRepositories.length
              )
            : onlySuccessfulVariant?.response.data.pagination;
        const resultPagination = effectivePagination
          ? buildResultPagination(effectivePagination)
          : undefined;

        const hasContent = repositories.length > 0;

        const shape = buildReposSearchOutput(
          { repositories, pagination: resultPagination },
          query
        );

        // Some query variants (e.g. the topics or keywords lane of a split
        // search) failed while others succeeded. Surface it so an empty or
        // thin result set isn't read as a confident, complete answer.
        const warnings = buildPartialFailureWarnings(failedVariants);

        const resultData = warnings ? { ...shape.data, warnings } : shape.data;

        return createSuccessResult(
          query,
          resultData,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
          {
            rawResponse: sumVariantRawResponseChars([
              ...successfulVariants,
              ...failedVariants,
            ]),
          }
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      keysPriority: ['repositories', 'pagination', 'error'] satisfies string[],
    },
    args
  );
}
