import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  handleCatchError,
  handleProviderError,
  createErrorResult,
  createSuccessResult,
} from '../utils.js';
import {
  mapRepoSearchProviderRepositories,
  mapRepoSearchToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperations,
} from '../providerExecution.js';
import {
  createSearchVariants,
  hasValidRepositorySearchParams,
  type PartialReposSearchQuery,
  type RepoSearchVariantExecution,
  type SuccessfulRepoSearchVariant,
} from './execution/queryVariants.js';
import {
  deduplicateRepositories,
  rankRepositoriesByRelevance,
} from './execution/ranking.js';
import {
  buildMergedPagination,
  buildPartialFailureWarnings,
  buildResultPagination,
  sumVariantRawResponseChars,
  type EffectivePagination,
} from './execution/pagination.js';

export {
  buildMergedPagination,
  buildPartialFailureWarnings,
  buildResultPagination,
} from './execution/pagination.js';

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
    // Date-only for ALL timestamps in discovery rows (one consistent format;
    // day precision is what ranking/recency decisions actually use).
    pushedAt: r.pushedAt ? r.pushedAt.slice(0, 10) : undefined,
    createdAt: r.createdAt ? r.createdAt.slice(0, 10) : undefined,
    updatedAt: r.updatedAt ? r.updatedAt.slice(0, 10) : undefined,
    defaultBranch:
      r.defaultBranch &&
      r.defaultBranch !== 'main' &&
      r.defaultBranch !== 'master'
        ? r.defaultBranch
        : undefined,
    topics: r.topics?.length ? r.topics : undefined,
    visibility:
      r.visibility && r.visibility !== 'public' ? r.visibility : undefined,
    // url intentionally omitted: derivable as https://github.com/{owner}/{repo}
    // (~40 bytes × every row of every page for zero information).
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
  // Ready-to-run follow-ups for the TOP result: discovery rows are leads, and
  // the natural next move is orienting inside (or code-searching) the best hit.
  const top = data.repositories[0];
  const next =
    top?.owner && top?.repo
      ? {
          viewStructure: {
            tool: 'ghViewRepoStructure',
            query: { owner: top.owner, repo: top.repo, path: '' },
            why: 'Orient in the top-ranked repository before reading code',
            confidence: 'heuristic',
          },
          searchCode: {
            tool: 'ghSearchCode',
            query: { owner: top.owner, repo: top.repo },
            why: 'Scope a code search to the top-ranked repository',
            confidence: 'heuristic',
          },
        }
      : undefined;
  return {
    data: {
      pagination: data.pagination,
      repositories: concise
        ? data.repositories.map(r => `${r.owner ? `${r.owner}/` : ''}${r.repo}`)
        : data.repositories.map(buildRepositoryDetail),
      ...(next ? { next } : {}),
    },
  };
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
