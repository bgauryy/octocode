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
  extraHints: string[];
} {
  const concise = (query as { concise?: boolean }).concise === true;
  return {
    data: {
      pagination: data.pagination,
      repositories: concise
        ? data.repositories.map(r => `${r.owner ? `${r.owner}/` : ''}${r.repo}`)
        : data.repositories.map(buildRepositoryDetail),
    },
    extraHints: concise
      ? [
          'Minimal owner/repo list — re-run without concise (or ghViewRepoStructure) to dive into a chosen repo.',
        ]
      : [],
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
  buildPaginationHints,
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

function buildResultPagination(pagination: {
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
    perPage: pagination.entriesPerPage || 10,
    totalMatches: pagination.totalMatches || 0,
    hasMore: pagination.hasMore,
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

function buildMergedPagination(
  variants: SuccessfulRepoSearchVariant[]
): EffectivePagination | undefined {
  const pages = variants
    .map(variant => variant.response.data.pagination)
    .filter((p): p is NonNullable<typeof p> => Boolean(p));
  if (pages.length === 0) return undefined;

  return {
    currentPage: pages[0]!.currentPage,
    totalPages: Math.max(...pages.map(p => p.totalPages)),
    hasMore: pages.some(p => p.hasMore),
    entriesPerPage: pages[0]!.entriesPerPage,
    totalMatches: pages.reduce((sum, p) => sum + (p.totalMatches ?? 0), 0),
    reachableTotalMatches: pages.reduce(
      (sum, p) => sum + (p.reachableTotalMatches ?? p.totalMatches ?? 0),
      0
    ),
    totalMatchesKind: pages.some(p => p.totalMatchesKind === 'lowerBound')
      ? 'lowerBound'
      : pages.some(p => p.totalMatchesKind === 'reported')
        ? 'reported'
        : 'exact',
    totalMatchesCapped: pages.some(p => p.totalMatchesCapped === true),
  };
}

function buildMergedPaginationHints(pagination: EffectivePagination): string[] {
  if (!pagination.hasMore) return [];
  return [
    `Page ${pagination.currentPage}/${pagination.totalPages} for merged topic+keyword results (~${pagination.totalMatches ?? 0} upper-bound total). Next: page=${pagination.currentPage + 1}.`,
  ];
}

function createVariantFailureHints(
  failures: RepoSearchVariantExecution[]
): string[] {
  return failures.flatMap(failure => {
    const label =
      failure.label === 'topics'
        ? 'Topic search'
        : failure.label === 'keywords'
          ? 'Keyword search'
          : 'Search';
    const error = failure.response.error || 'Provider error';
    return `${label} failed: ${error}`;
  });
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

const LARGE_RESULT_THRESHOLD = 100;

function generateSearchSpecificHints(
  query: PartialReposSearchQuery,
  hasResults: boolean,
  hasMore = false,
  totalMatches = 0
): string[] | undefined {
  if (hasResults) {
    if (
      hasMore &&
      totalMatches > LARGE_RESULT_THRESHOLD &&
      !query.owner &&
      !query.language &&
      !query.stars
    ) {
      return [
        'Large result set with no owner/language/stars filter — add owner="<org>" to scope to a specific org, language="<lang>" to restrict by language, or stars=">100" to surface established repos.',
      ];
    }
    return undefined;
  }
  const hasTopics = hasValidTopics(query);
  const hasKeywords = hasValidKeywords(query);
  const stars = typeof query.stars === 'string' ? query.stars : undefined;
  const created = typeof query.created === 'string' ? query.created : undefined;
  const updated = typeof query.updated === 'string' ? query.updated : undefined;
  const hints: string[] = [];

  if (hasTopics && hasKeywords) {
    hints.push('No match for topics AND keywords. Drop topics, then keywords.');
  } else if (hasTopics) {
    hints.push('No topic match. Drop one, try synonyms, or use keywords.');
  } else if (hasKeywords) {
    hints.push(
      'No keyword match. Drop the rarest, try synonyms, or use topics.'
    );
  }

  const filters: string[] = [];
  if (stars) filters.push(`stars="${stars}"`);
  if (created) filters.push(`created="${created}"`);
  if (updated) filters.push(`updated="${updated}"`);
  if (filters.length > 0) {
    hints.push(`Filters (${filters.join(', ')}) — try widening/removing.`);
  }

  if (hints.length === 0) {
    return undefined;
  }
  return hints;
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
              query
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

        const nonExistentScope = successfulVariants.some(
          variant =>
            (variant.response.data as { nonExistentScope?: boolean })
              .nonExistentScope
        );
        const scopeHints =
          repositories.length === 0 && nonExistentScope
            ? [
                `Owner "${query.owner ?? '?'}" doesn't exist or isn't searchable — verify spelling/access, not filters.`,
              ]
            : [];
        const onlySuccessfulVariant =
          successfulVariants.length === 1 ? successfulVariants[0] : undefined;
        const isMergedResult = successfulVariants.length > 1;
        const effectivePagination: EffectivePagination | undefined =
          isMergedResult
            ? buildMergedPagination(successfulVariants)
            : onlySuccessfulVariant?.response.data.pagination;
        const paginationHints = effectivePagination
          ? isMergedResult
            ? buildMergedPaginationHints(effectivePagination)
            : buildPaginationHints(effectivePagination, 'repos')
          : [];
        const resultPagination = effectivePagination
          ? buildResultPagination(effectivePagination)
          : undefined;
        const requestedPage = (query as { page?: number }).page;
        const lastAvailablePage = effectivePagination?.totalPages ?? 0;
        const pageExceedsTotal = Boolean(
          typeof requestedPage === 'number' &&
          lastAvailablePage > 0 &&
          requestedPage > lastAvailablePage &&
          repositories.length === 0
        );
        const pageExceededHints = pageExceedsTotal
          ? [
              `page ${requestedPage} exceeds totalPages ${lastAvailablePage} — last page is ${lastAvailablePage}.`,
            ]
          : [];

        const hasContent = repositories.length > 0;
        const hasMore = Boolean(effectivePagination?.hasMore);
        const totalMatchesForHint =
          effectivePagination?.totalMatches ??
          effectivePagination?.reachableTotalMatches ??
          0;
        const searchHints = pageExceedsTotal
          ? undefined
          : generateSearchSpecificHints(
              query,
              hasContent,
              hasMore,
              totalMatchesForHint
            );
        const partialFailureHints =
          variants.length > 1 && successfulVariants.length === 1
            ? [
                `Only ${onlySuccessfulVariant?.label ?? 'one'} search succeeded; pagination reflects that subset.`,
                ...createVariantFailureHints(failedVariants),
              ]
            : createVariantFailureHints(failedVariants);

        const shape = buildReposSearchOutput(
          { repositories, pagination: resultPagination },
          query
        );

        const escalationHints: string[] = [];
        if (hasContent) {
          const top = repositories[0];
          if (top?.owner && top?.repo) {
            escalationHints.push(
              `Top result: ${top.owner}/${top.repo} — use ghViewRepoStructure to browse or ghSearchCode to search within it.`
            );
          }
          if (repositories.length >= 3) {
            escalationHints.push(
              'Use multiple ghViewRepoStructure queries in parallel to compare the layouts of top results.'
            );
          }
        }
        const allExtraHints = [
          ...pageExceededHints,
          ...scopeHints,
          ...shape.extraHints,
          ...partialFailureHints,
          ...paginationHints,
          ...(searchHints || []),
          ...escalationHints,
        ];
        const finalExtraHints = allExtraHints;

        return createSuccessResult(
          query,
          shape.data,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
          {
            extraHints: finalExtraHints,
            hintContext: pageExceedsTotal
              ? {}
              : {
                  keywords: query.keywords,
                  owner: query.owner,
                  language: query.language,
                  topic: query.topicsToSearch?.[0],
                },
            rawResponse: sumVariantRawResponseChars([
              ...successfulVariants,
              ...failedVariants,
            ]),
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      keysPriority: ['repositories', 'pagination', 'error'] satisfies string[],
      peerHints: true,
    },
    args
  );
}
