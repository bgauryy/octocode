import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type {
  GitHubReposSearchQuery,
  GitHubRepositoryOutput,
} from '@octocodeai/octocode-core';
import {
  TOOL_NAMES,
  getDynamicHints as getMetadataDynamicHints,
} from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type PartialReposSearchQuery = WithOptionalMeta<GitHubReposSearchQuery>;
import {
  handleCatchError,
  handleProviderError,
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
  return Boolean(query.keywordsToSearch && query.keywordsToSearch.length > 0);
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
    const { topicsToSearch, keywordsToSearch, ...baseQuery } = query;
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
          keywordsToSearch,
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

function buildResultPagination(pagination: {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
}) {
  return {
    currentPage: pagination.currentPage,
    totalPages: pagination.totalPages,
    perPage: pagination.entriesPerPage || 10,
    totalMatches: pagination.totalMatches || 0,
    hasMore: pagination.hasMore,
  };
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

function generateSearchSpecificHints(
  query: PartialReposSearchQuery,
  hasResults: boolean
): string[] | undefined {
  const hints: string[] = [];
  const hasTopics = hasValidTopics(query);
  const hasKeywords = hasValidKeywords(query);

  if (hasTopics && hasResults) {
    hints.push(
      ...getMetadataDynamicHints(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        'topicsHasResults'
      )
    );
  } else if (hasTopics && !hasResults) {
    hints.push(
      ...getMetadataDynamicHints(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        'topicsEmpty'
      )
    );
  } else if (hasKeywords && !hasResults && !hasTopics) {
    hints.push(
      ...getMetadataDynamicHints(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        'keywordsEmpty'
      )
    );
  }

  return hints.length > 0 ? hints : undefined;
}

export async function searchMultipleGitHubRepos(
  args: ToolExecutionArgs<PartialReposSearchQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialReposSearchQuery, _index: number) => {
      try {
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

        const repositories = deduplicateRepositories(
          successfulVariants.flatMap(variant =>
            mapRepoSearchProviderRepositories(
              variant.response.data.repositories
            )
          )
        );

        const searchHints = generateSearchSpecificHints(
          query,
          repositories.length > 0
        );
        const onlySuccessfulVariant =
          successfulVariants.length === 1 ? successfulVariants[0] : undefined;
        const successfulPagination =
          onlySuccessfulVariant?.response.data.pagination;
        const paginationHints = successfulPagination
          ? buildPaginationHints(successfulPagination, 'repos')
          : [];
        const resultPagination = successfulPagination
          ? buildResultPagination(successfulPagination)
          : undefined;
        const mergeHints =
          successfulVariants.length > 1
            ? [
                'Combined topic and keyword searches into one result; pagination is omitted because multiple result sets were merged.',
              ]
            : [];
        const partialFailureHints =
          variants.length > 1 && successfulVariants.length === 1
            ? [
                `Only ${onlySuccessfulVariant?.label ?? 'one'} search succeeded; pagination reflects that subset.`,
                ...createVariantFailureHints(failedVariants),
              ]
            : createVariantFailureHints(failedVariants);

        return createSuccessResult(
          query,
          { repositories, pagination: resultPagination },
          repositories.length > 0,
          TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
          {
            extraHints: [
              ...mergeHints,
              ...partialFailureHints,
              ...paginationHints,
              ...(searchHints || []),
            ],
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
      responseCharOffset,
      responseCharLength,
    }
  );
}
