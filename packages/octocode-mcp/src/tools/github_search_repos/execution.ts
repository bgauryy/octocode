import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
import type { GitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';

type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { compareIsoDateDescending } from '../../utils/core/compare.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';

const CONCISE_REPOS_LIMIT = 3;

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

type PartialReposSearchQuery = WithOptionalMeta<GitHubReposSearchSingleQuery>;
type ReposQueryWithVerbosity = WithVerbosity<PartialReposSearchQuery>;

/**
 * Per-tool verbosity shaping for githubSearchRepositories. Under concise,
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
  if (isConcise(query.verbosity)) {
    const projected = (data.repositories ?? [])
      .slice(0, CONCISE_REPOS_LIMIT)
      .map(r => {
        const owner = (r as { owner?: string }).owner;
        const repo = (r as { repo?: string }).repo;
        const full_name =
          (r as { full_name?: string }).full_name ??
          (owner && repo ? `${owner}/${repo}` : undefined);
        return {
          full_name,
          stars: (r as { stars?: number }).stars,
          language: (r as { language?: string }).language,
        };
      });
    const summary = `${data.repositories?.length ?? 0} repos${
      projected[0]?.full_name ? ` (top: ${projected[0].full_name})` : ''
    }`;
    return {
      data: { repositories: projected },
      extraHints: [summary],
    };
  }
  return { data, extraHints: [] };
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
  return Boolean(query.keywordsToSearch && query.keywordsToSearch.length > 0);
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
    query.size
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
    case 'updated':
      return compareIsoDateDescending(left.updatedAt, right.updatedAt);
    case 'created':
      return compareIsoDateDescending(left.createdAt, right.createdAt);
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
  const keywords = query.keywordsToSearch ?? [];
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
};

/**
 * Merge the per-variant pagination of a topics+keywords combined search into a
 * single paginable signal. Both variants share the requested `page`, so the
 * merged set is still paginable: `hasMore` if EITHER variant has more, and
 * `totalMatches` is the SUM — an upper bound, since a repo matching both topics
 * AND keywords is counted in each variant's total.
 */
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
  };
}

/**
 * Pagination hint for a merged combined search. Unlike the single-variant hint,
 * we don't claim a precise "showing X–Y" range (a merged page can return up to
 * 2× perPage rows); we give the actionable next-page + the upper-bound total.
 */
function buildMergedPaginationHints(pagination: EffectivePagination): string[] {
  if (!pagination.hasMore) return [];
  return [
    `More topic+keyword results — fetch page ${pagination.currentPage + 1}; ~${pagination.totalMatches ?? 0} total (upper bound, repos matching both counted twice).`,
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
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
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
        // Pre-flight: cap the effective per_page under concise so the upstream
        // fetch reflects the trimmed response. No downgrade hint is emitted —
        // concise's cap is its documented contract and pagination.totalMatches
        // keeps the true count visible. Cap BOTH per_page knobs.
        const verbosityIsConcise = isConcise(
          (query as WithVerbosity<typeof query>).verbosity
        );
        if (verbosityIsConcise) {
          const q = query as {
            itemsPerPage?: number;
            githubAPILimit?: number;
          };
          q.itemsPerPage = Math.min(
            q.itemsPerPage ?? CONCISE_REPOS_LIMIT,
            CONCISE_REPOS_LIMIT
          );
          if (typeof q.githubAPILimit === 'number') {
            q.githubAPILimit = Math.min(q.githubAPILimit, CONCISE_REPOS_LIMIT);
          }
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

        const repositories = rankRepositoriesByRelevance(
          deduplicateRepositories(
            successfulVariants.flatMap(variant =>
              mapRepoSearchProviderRepositories(
                variant.response.data.repositories
              )
            )
          ),
          query
        );

        // GitHub reported the searched owner/user does not exist (422), as
        // opposed to a valid scope that matched nothing. Lead recovery with the
        // scope rather than filter-widening when this is the cause.
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
        const searchHints = generateSearchSpecificHints(
          query,
          repositories.length > 0
        );
        const onlySuccessfulVariant =
          successfulVariants.length === 1 ? successfulVariants[0] : undefined;
        const isMergedResult = successfulVariants.length > 1;
        // Combined topics+keywords searches are now paginable: merge the
        // per-variant pagination into a single upper-bound signal instead of
        // dropping it.
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
        const partialFailureHints =
          variants.length > 1 && successfulVariants.length === 1
            ? [
                `Only ${onlySuccessfulVariant?.label ?? 'one'} search succeeded; pagination reflects that subset.`,
                ...createVariantFailureHints(failedVariants),
              ]
            : createVariantFailureHints(failedVariants);

        const hasContent = repositories.length > 0;
        const hasMore = Boolean(effectivePagination?.hasMore);
        const variantsPartial =
          variants.length > 1 && successfulVariants.length < variants.length;

        const verbosityShape = applyGithubSearchReposVerbosity(
          { repositories, pagination: resultPagination },
          query as ReposQueryWithVerbosity
        );

        // No verbosity-feature hint: concise's limit cap is its documented
        // contract and pagination.totalMatches keeps the full count visible.
        // Escalation hints guide agents to the next research step when repos
        // are found — the top result is the most actionable anchor.
        const escalationHints: string[] = [];
        if (hasContent) {
          const top = repositories[0];
          if (top?.owner && top?.repo) {
            escalationHints.push(
              `Top result: ${top.owner}/${top.repo} — use githubViewRepoStructure to browse or githubSearchCode to search within it.`
            );
          }
        }
        const allExtraHints = [
          ...scopeHints,
          ...verbosityShape.extraHints,
          ...partialFailureHints,
          ...paginationHints,
          ...(searchHints || []),
          ...escalationHints,
        ];
        // Compact trim: drop advisory hints (recovery prose, synonym
        // suggestions) while keeping pagination + downgrade + drill-back.
        const compactMode = isCompact(
          (query as WithVerbosity<typeof query>).verbosity
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
                    reason: nonExistentScope
                      ? `Owner "${query.owner ?? '?'}" doesn't exist or isn't searchable — verify the scope, not filters.`
                      : 'No repositories matched the supplied filters; consider dropping topics/keywords or widening stars/created/updated ranges.',
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
      peerHints: true,
      peerEvidence: true,
    }
  );
}
