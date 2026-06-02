import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
import type { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubSearchPullRequestsToolResult } from '@octocodeai/octocode-core/extra-types';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';

const CONCISE_PR_LIMIT = 3;

/** Advisory hints githubSearchPullRequests emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisorySearchPRsHint = makeAdvisoryPredicate([
  'pr archaeology',
  'title-only',
  'withcomments',
  'withcommits',
  'add tokens',
  'start with type',
  'merged shorthand',
]);
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

/** Fields that have ZodDefault values and can be omitted by callers */
type PRDefaultKeys =
  | 'order'
  | 'limit'
  | 'page'
  | 'withComments'
  | 'withCommits'
  | 'type';
type PartialPRQuery = WithOptionalMeta<
  Omit<GitHubPullRequestSearchQuery, PRDefaultKeys> &
    Partial<Pick<GitHubPullRequestSearchQuery, PRDefaultKeys>>
>;
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
} from '../utils.js';
import {
  buildPaginationHints,
  mapPullRequestProviderResultData,
  mapPullRequestToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';

export async function searchMultipleGitHubPullRequests(
  args: ToolExecutionArgs<PartialPRQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialPRQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();
        let effectiveQuery: PartialPRQuery = { ...query };
        const verbosityDowngradeHints: string[] = [];

        // Pre-flight verbosity caps under concise: cap page size to 3; coerce
        // type→"metadata" unless caller passed prNumber + explicit type;
        // drop partialContentMetadata when type is coerced. Record what
        // fired so we can emit a verbosity-downgrade warning later.
        const prVerbosityIsConcise = isConcise(
          (effectiveQuery as WithVerbosity<typeof effectiveQuery>).verbosity
        );
        if (prVerbosityIsConcise) {
          // Cap the effective per_page to the concise probe size via both knobs
          // the resolver reads (itemsPerPage drives per_page; githubAPILimit
          // overrides it when present).
          const q = effectiveQuery as {
            itemsPerPage?: number;
            githubAPILimit?: number;
          };
          if (typeof q.itemsPerPage === 'number') {
            q.itemsPerPage = Math.min(q.itemsPerPage, CONCISE_PR_LIMIT);
          } else {
            q.itemsPerPage = CONCISE_PR_LIMIT;
          }
          if (typeof q.githubAPILimit === 'number') {
            q.githubAPILimit = Math.min(q.githubAPILimit, CONCISE_PR_LIMIT);
          }
          const hasExplicitType =
            (effectiveQuery as { type?: string }).type !== undefined;
          const hasPrNumber = effectiveQuery.prNumber !== undefined;
          const shouldCoerceType = !(hasPrNumber && hasExplicitType);
          if (shouldCoerceType) {
            const currentType = (effectiveQuery as { type?: string }).type;
            const hadPartialContentMetadata =
              (effectiveQuery as { partialContentMetadata?: unknown })
                .partialContentMetadata !== undefined;
            if (currentType && currentType !== 'metadata') {
              (effectiveQuery as { type?: string }).type = 'metadata';
              verbosityDowngradeHints.push(
                "type coerced to 'metadata' under concise verbosity"
              );
            } else if (!currentType) {
              (effectiveQuery as { type?: string }).type = 'metadata';
            }
            if (hadPartialContentMetadata) {
              const {
                partialContentMetadata: _partialContentMetadata,
                ...rest
              } = effectiveQuery as PartialPRQuery & {
                partialContentMetadata?: unknown;
              };
              effectiveQuery = rest as PartialPRQuery;
              verbosityDowngradeHints.push(
                'partialContentMetadata dropped under concise metadata mode'
              );
            }
          }
        }

        if (effectiveQuery.query && String(effectiveQuery.query).length > 256) {
          return createErrorResult(
            'Query too long. Maximum 256 characters allowed.',
            query
          );
        }

        const hasValidParams =
          effectiveQuery.query?.trim() ||
          effectiveQuery.owner ||
          effectiveQuery.repo ||
          effectiveQuery.author ||
          effectiveQuery.assignee ||
          (effectiveQuery.prNumber &&
            effectiveQuery.owner &&
            effectiveQuery.repo);

        if (!hasValidParams) {
          return createErrorResult(
            'At least one valid search parameter, filter, or PR number is required.',
            query
          );
        }

        const providerResult = await executeProviderOperation(
          effectiveQuery,
          () =>
            currentProviderContext.provider.searchPullRequests(
              mapPullRequestToolQuery(effectiveQuery)
            )
        );

        if (providerResult.ok === false) {
          return providerResult.result;
        }

        // A prNumber lookup targets one PR — return its full body, not the
        // 500-char search preview (and make the truncation hint truthful).
        // type="metadata" (the triage default) drops the per-PR file list to
        // keep the payload lean; counts (changedFilesCount) are retained.
        const prType = (effectiveQuery as { type?: string }).type;
        const includeFileChanges =
          prType === 'fullContent' || prType === 'partialContent';
        const {
          pullRequests,
          resultData,
          pagination: rawPagination,
        } = mapPullRequestProviderResultData(providerResult.response.data, {
          includeFileChanges,
        });

        // A direct prNumber lookup resolves a single PR — the search-style
        // pagination block (totalMatches:0, totalPages:1, hasMore:false) is
        // meaningless noise, so drop it from both the hints and the payload.
        const pagination =
          effectiveQuery.prNumber !== undefined ? undefined : rawPagination;
        if (effectiveQuery.prNumber !== undefined) {
          delete (resultData as Record<string, unknown>).pagination;
        }

        const hasContent = pullRequests.length > 0;

        const paginationHints = pagination
          ? buildPaginationHints(
              {
                currentPage: pagination.currentPage,
                totalPages: pagination.totalPages,
                hasMore: pagination.hasMore,
                totalMatches: pagination.totalMatches,
                entriesPerPage: pagination.perPage,
              },
              'PRs'
            )
          : [];

        // Char-pagination is owned by the unified bulk engine: per-query
        // charOffset/charLength flow through applyQueryOutputPagination and the
        // aggregate through applyBulkResponsePagination (see
        // structuredPagination.ts). Both slice the `pull_requests` array
        // losslessly against the single getOutputCharLimit() and expose a
        // cursor — so this tool no longer pre-paginates (the old per-query
        // applyOutputSizeLimit attached metadata but never clipped the body,
        // emitting contradictory totals). resultData is passed through as-is.

        const fileChangeHints: string[] = [];
        const largeFileChangePRs = pullRequests.filter(
          (pr: Record<string, unknown>) => {
            const count =
              typeof pr.changedFilesCount === 'number'
                ? pr.changedFilesCount
                : Array.isArray(pr.fileChanges)
                  ? (pr.fileChanges as unknown[]).length
                  : 0;
            return count > 30;
          }
        );
        if (largeFileChangePRs.length > 0) {
          const prNumbers = largeFileChangePRs
            .map((pr: Record<string, unknown>) => `#${pr.number}`)
            .join(', ');
          const maxFiles = Math.max(
            ...largeFileChangePRs.map((pr: Record<string, unknown>) => {
              if (typeof pr.changedFilesCount === 'number')
                return pr.changedFilesCount;
              return Array.isArray(pr.fileChanges)
                ? (pr.fileChanges as unknown[]).length
                : 0;
            })
          );
          fileChangeHints.push(
            `Large PR(s) ${prNumbers} have ${maxFiles}+ file changes.`
          );
        }
        if (!includeFileChanges) {
          const withChanges = pullRequests.filter(
            (pr: Record<string, unknown>) =>
              typeof pr.changedFilesCount === 'number' &&
              pr.changedFilesCount > 0
          ).length;
          if (withChanges > 0) {
            fileChangeHints.push(
              'Metadata mode: file lists include paths + counts only (no diffs). Use type="partialContent" (with partialContentMetadata) or type="fullContent" to fetch patches.'
            );
          }
        }

        // Result-page completeness only; char-pagination completeness is
        // reflected by the engine via responsePagination + evidence reasons.
        const hasMore = Boolean(pagination?.hasMore);

        const shaped = applyGithubSearchPullRequestsVerbosity(
          {
            data: resultData,
            pullRequests,
            extraHints: [
              ...paginationHints,
              ...verbosityDowngradeHints,
              ...fileChangeHints,
            ],
          },
          effectiveQuery as PartialPRQuery
        );

        return createSuccessResult(
          effectiveQuery,
          shaped.data,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
          {
            // Pass query-shape fields so the per-tool empty branch can
            // name the actual filters that produced zero results
            // (state, author, prNumber, query) instead of generic prose.
            hintContext: {
              matchCount: pullRequests.length,
              state: effectiveQuery.state,
              owner: effectiveQuery.owner,
              repo: effectiveQuery.repo,
              author: effectiveQuery.author,
              query: effectiveQuery.query,
              prNumber: effectiveQuery.prNumber,
            },
            extraHints: shaped.extraHints,
            evidence: {
              kind: 'pr',
              answerReady: hasContent,
              complete: hasContent && !hasMore,
              ...(hasContent
                ? {}
                : {
                    reason:
                      'No PRs matched the supplied filters; try widening the query or removing state/author/label filters.',
                  }),
            },
            rawResponse: providerResult.response.rawResponseChars,
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      keysPriority: [
        'pull_requests',
        'pagination',
        'total_count',
        'error',
      ] satisfies Array<keyof GitHubSearchPullRequestsToolResult>,
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}

/**
 * Per-tool verbosity shaping for githubSearchPullRequests. Under concise,
 * projects each PR to {number, title, state, merged} (cap 3) and emits a
 * summary + drill-back hint. Under compact, advisory hints are trimmed to 2.
 * Basic / omitted: passthrough.
 */
export function applyGithubSearchPullRequestsVerbosity(
  input: {
    data: Record<string, unknown>;
    pullRequests: Array<Record<string, unknown>>;
    extraHints: string[];
  },
  query: PartialPRQuery
): { data: Record<string, unknown>; extraHints: string[] } {
  const verbosity = (query as WithVerbosity<typeof query>).verbosity;

  if (isConcise(verbosity)) {
    const conciseData = {
      ...input.data,
      pull_requests: input.pullRequests.slice(0, 3).map(pr => ({
        number: (pr as { number?: number }).number,
        title: (pr as { title?: string }).title,
        state: (pr as { state?: string }).state,
        merged: (pr as { merged?: boolean }).merged,
      })),
    };
    const summary = `${input.pullRequests.length} PRs (top: #${
      (input.pullRequests[0] as { number?: number })?.number ?? '?'
    })`;
    return {
      data: conciseData,
      extraHints: [summary, ...input.extraHints],
    };
  }

  const allHints = [...input.extraHints];
  if (isCompact(verbosity)) {
    return {
      data: input.data,
      extraHints: compactTrimHints(allHints, isAdvisorySearchPRsHint, 2) ?? [],
    };
  }
  return { data: input.data, extraHints: allHints };
}
