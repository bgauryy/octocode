import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type {
  GitHubReposSearchQuery,
  GitHubRepositoryOutput,
} from '@octocodeai/octocode-core';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import {
  isUltra,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
  ultraDrillBackHint,
} from '../../scheme/verbosity.js';
import type { Verbosity } from '../../scheme/localSchemaOverlay.js';

const ULTRA_REPOS_LIMIT = 3;

/** Advisory hints githubSearchRepositories emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisorySearchReposHint = makeAdvisoryPredicate([
  'synonym',
  'high star filter',
  'language filtering',
  'topics are self-reported',
  'sparse',
]);
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type PartialReposSearchQuery = WithOptionalMeta<GitHubReposSearchQuery>;
type ReposQueryWithVerbosity = PartialReposSearchQuery & {
  verbosity?: Verbosity;
};

/**
 * Per-tool verbosity shaping for githubSearchRepositories. Under ultra,
 * projects each repo to {full_name, stars, language?} and caps to 3, and
 * emits a drill-back hint. Basic / compact pass through (compact-trim of
 * advisory hints is handled at the bulk-finalizer pass).
 */
export function applyGithubSearchReposVerbosity(
  data: { repositories: GitHubRepositoryOutput[]; pagination?: unknown },
  query: ReposQueryWithVerbosity
): {
  data: { repositories: unknown[]; pagination?: unknown };
  extraHints: string[];
} {
  if (isUltra(query.verbosity)) {
    const projected = (data.repositories ?? [])
      .slice(0, 3)
      .map(r => ({
        full_name: (r as { full_name?: string }).full_name,
        stars: (r as { stars?: number }).stars,
        language: (r as { language?: string }).language,
      }));
    const summary = `${data.repositories?.length ?? 0} repos${
      projected[0]?.full_name ? ` (top: ${projected[0].full_name})` : ''
    }`;
    return {
      data: { repositories: projected },
      extraHints: [
        summary,
        ...ultraDrillBackHint(
          're-call with verbosity:"basic" (default) or narrow keywordsToSearch'
        ),
      ],
    };
  }
  return { data, extraHints: [] };
}
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
  // Local recovery hints only — name the actual filters in play so the
  // agent can drop them one by one. No upstream static guidance.
  if (hasResults) return undefined;
  const hasTopics = hasValidTopics(query);
  const hasKeywords = hasValidKeywords(query);
  const stars = typeof query.stars === 'string' ? query.stars : undefined;
  const created = typeof query.created === 'string' ? query.created : undefined;
  const updated = typeof query.updated === 'string' ? query.updated : undefined;
  const hints: string[] = [];

  if (hasTopics && hasKeywords) {
    hints.push(
      'No repos match topics AND keywords. Drop topics first, then keywords.'
    );
  } else if (hasTopics) {
    hints.push(
      'No repos for these topics. Drop a topic, try synonyms, or switch to a keywords search.'
    );
  } else if (hasKeywords) {
    hints.push(
      'No repos for these keywords. Drop the rarest keyword, broaden synonyms, or switch to topics.'
    );
  }

  const filters: string[] = [];
  if (stars) filters.push(`stars="${stars}"`);
  if (created) filters.push(`created="${created}"`);
  if (updated) filters.push(`updated="${updated}"`);
  if (filters.length > 0) {
    hints.push(
      `Numeric/date filters applied (${filters.join(', ')}) — try widening or removing them.`
    );
  }

  if (hints.length === 0) {
    return undefined;
  }
  return hints;
}

export async function searchMultipleGitHubRepos(
  args: ToolExecutionArgs<PartialReposSearchQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength, format } =
    args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialReposSearchQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();
        // Pre-flight: cap user-passed `limit` under ultra so the upstream
        // fetch reflects the trimmed response. Emit verbosity-downgrade
        // warning when the cap actually fires.
        const userLimit = (query as { limit?: number }).limit;
        const verbosityIsUltra = isUltra(
          (query as { verbosity?: Verbosity }).verbosity
        );
        const capFired =
          verbosityIsUltra &&
          typeof userLimit === 'number' &&
          userLimit > ULTRA_REPOS_LIMIT;
        if (verbosityIsUltra) {
          (query as { limit?: number }).limit = Math.min(
            userLimit ?? ULTRA_REPOS_LIMIT,
            ULTRA_REPOS_LIMIT
          );
        }
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

        const hasContent = repositories.length > 0;
        const hasMore = Boolean(successfulPagination?.hasMore);
        const variantsPartial =
          variants.length > 1 && successfulVariants.length < variants.length;

        const verbosityShape = applyGithubSearchReposVerbosity(
          { repositories, pagination: resultPagination },
          query as ReposQueryWithVerbosity
        );

        const downgradeHint =
          capFired && verbosityIsUltra
            ? [
                `verbosity-downgrade: limit capped to ${ULTRA_REPOS_LIMIT} (ultra); caller passed ${userLimit}`,
              ]
            : [];

        const allExtraHints = [
          ...verbosityShape.extraHints,
          ...downgradeHint,
          ...mergeHints,
          ...partialFailureHints,
          ...paginationHints,
          ...(searchHints || []),
        ];
        // Compact trim: drop advisory hints (recovery prose, synonym
        // suggestions) while keeping pagination + downgrade + drill-back.
        const compactMode = isCompact(
          (query as { verbosity?: Verbosity }).verbosity
        );
        const finalExtraHints = compactMode
          ? (compactTrimHints(allExtraHints, isAdvisorySearchReposHint, 2) ??
            [])
          : allExtraHints;

        return createSuccessResult(
          query,
          verbosityShape.data,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
          {
            extraHints: finalExtraHints,
            evidence: {
              kind: 'repo',
              answerReady: hasContent,
              complete: hasContent && !hasMore && !variantsPartial,
              confidence: variantsPartial ? 'medium' : undefined,
              ...(hasContent
                ? {}
                : {
                    reason:
                      'No repositories matched the supplied filters; consider dropping topics/keywords or widening stars/created/updated ranges.',
                  }),
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
      responseCharOffset,
      responseCharLength,

      format,
      peerHints: true,
      peerEvidence: true,
    }
  );
}
