import type { CallToolResult } from '@modelcontextprotocol/server';
import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import { getOctokit } from '../../github/client.js';
import { resolveCanonicalOwnerRepo } from '../../github/canonicalRepo.js';
import {
  handleCatchError,
  createErrorResult,
  createSuccessResult,
} from '../utils.js';
import {
  mapRepoSearchProviderRepositories,
  mapRepoSearchToolQuery,
} from '../providerMappers/repoSearch.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';
import {
  hasValidKeywords,
  hasValidRepositorySearchParams,
  type PartialReposSearchQuery,
} from './execution/queryVariants.js';
import { buildResultPagination } from './execution/pagination.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

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
            tool: 'github.tree',
            query: { owner: top.owner, repo: top.repo, path: '' },
            why: 'Orient in the top-ranked repository before reading code',
            confidence: 'low',
          },
          searchCode: {
            tool: 'github.code',
            query: { owner: top.owner, repo: top.repo },
            why: 'Scope a code search to the top-ranked repository',
            confidence: 'low',
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

export async function searchGitHubRepos(
  query: PartialReposSearchQuery,
  args: ToolExecutionArgs<PartialReposSearchQuery>,
  getProviderContext = createLazyProviderContext(args.authInfo)
): Promise<ProcessedBulkResult> {
  const { authInfo } = args;
  try {
    if (!hasValidRepositorySearchParams(query)) {
      return createErrorResult(
        'At least one repository search term or filter is required.',
        query
      );
    }

    const currentProviderContext = getProviderContext();
    // Keywords and topics are conjunctive filters. One provider query keeps
    // GitHub's ranking and cursor intact and avoids dropping merged rows.
    const operation = await executeProviderOperation(query, () =>
      currentProviderContext.provider.searchRepos(mapRepoSearchToolQuery(query))
    );
    if (operation.ok === false) return operation.result;
    const { response } = operation;
    const repositories = mapRepoSearchProviderRepositories(
      response.data.repositories
    );
    const effectivePagination = response.data.pagination;
    const resultPagination = effectivePagination
      ? buildResultPagination(effectivePagination)
      : undefined;

    const hasContent = repositories.length > 0;

    const shape = buildReposSearchOutput(
      { repositories, pagination: resultPagination },
      query
    );

    // An owner-scoped search whose keywords include a candidate repo name
    // with no exact-name hit among the results is ambiguous the same way
    // a scoped code-search miss is: true absence, a near-miss (other repos
    // just happen to match too), or the repo was transferred out from
    // under this owner (GitHub's search index has no redirect for that,
    // unlike `repos.get`) — the transferred repo silently vanishes behind
    // whatever else the owner still has matching the same keyword, so
    // this isn't only a zero-result symptom. Best-effort, never blocks or
    // fails the search over it; only probe valid names on the first page.
    let transferHint:
      { warning: string; next: Record<string, unknown> } | undefined;
    if (query.owner && hasValidKeywords(query) && (query.page ?? 1) === 1) {
      const candidates = (
        Array.isArray(query.keywords) ? query.keywords : [query.keywords]
      )
        .filter(
          (keyword): keyword is string =>
            typeof keyword === 'string' && /^[A-Za-z0-9_.-]+$/.test(keyword)
        )
        .slice(0, 3);
      const hasExactNameMatch = candidates.some(candidate =>
        repositories.some(
          r => r.repo?.toLowerCase() === candidate.toLowerCase()
        )
      );
      if (candidates.length > 0 && !hasExactNameMatch) {
        try {
          const octokit = await getOctokit(authInfo);
          for (const candidate of candidates) {
            const resolved = await resolveCanonicalOwnerRepo(
              octokit,
              String(query.owner),
              candidate,
              authInfo
            );
            if (resolved.renamed) {
              transferHint = {
                warning: `Repository "${query.owner}/${candidate}" now resolves to "${resolved.owner}/${resolved.repo}" — the repository may have been transferred. Retry scoped to owner:"${resolved.owner}" (see next.retryUnderCanonicalOwner).`,
                next: {
                  retryUnderCanonicalOwner: {
                    tool: 'github.repositories',
                    query: {
                      ...query,
                      owner: resolved.owner,
                      keywords: [resolved.repo],
                    },
                    why: "Re-run scoped to the repository's current owner after a detected transfer.",
                    confidence: 'exact',
                  },
                },
              };
              break;
            }
          }
        } catch {
          // Metadata probe is best-effort — never fail the search over it.
        }
      }
    }

    // A genuine zero-result response previously carried no guidance at
    // all (unlike local text search's in-band hints) — tell the agent how
    // to widen instead of leaving a bare status:"empty".
    const warnings = [
      ...(transferHint ? [transferHint.warning] : []),
      ...(!hasContent && !transferHint
        ? [
            'No repositories matched. Keywords are ANDed — try fewer or broader keywords, drop a topic/filter (topics are sparse), or add match:"readme" for full-text search.',
          ]
        : []),
    ];

    const resultData = {
      ...shape.data,
      ...(response.data.incompleteResults ? { incompleteResults: true } : {}),
      ...(warnings.length > 0 ? { warnings } : {}),
      ...(transferHint ? { next: transferHint.next } : {}),
    };

    return createSuccessResult(
      query,
      resultData,
      hasContent,
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      {
        rawResponse:
          response.rawResponseChars ?? countSerializedChars(response.data),
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
}

export async function searchMultipleGitHubRepos(
  args: ToolExecutionArgs<PartialReposSearchQuery>
): Promise<CallToolResult> {
  const getProviderContext = createLazyProviderContext(args.authInfo);
  return executeBulkOperation(
    args.queries,
    query => searchGitHubRepos(query, args, getProviderContext),
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      keysPriority: ['repositories', 'pagination', 'error'] satisfies string[],
    },
    args
  );
}
