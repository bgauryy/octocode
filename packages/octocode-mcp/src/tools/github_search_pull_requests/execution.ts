import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubSearchPullRequestsToolResult } from '@octocodeai/octocode-core/extra-types';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { isVerbose } from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
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
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialPRQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();
        const effectiveQuery: PartialPRQuery = { ...query };
        const verbosityDowngradeHints: string[] = [];

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
              'Metadata mode: file lists include paths + counts, no patches. Gradual workflow — re-call with type="partialContent" + partialContentMetadata=[{file:"src/foo.ts"},{file:"src/bar.ts"}] to fetch targeted patches. Add line filters via additions/deletions arrays. Use type="fullContent" with prNumber only for small PRs.'
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
      peerHints: true,
      peerEvidence: true,
    }
  );
}

/**
 * Verbosity shaping for githubSearchPullRequests.
 *
 * verbose=false (default): omit metadata-only PR fields (createdAt, updatedAt,
 *   closedAt, mergedAt, comments, reactions, labels, assignees, etc.).
 * verbose=true: include all fields.
 *
 * Items are never dropped. Hints are always returned fully.
 */
export function applyGithubSearchPullRequestsVerbosity(
  input: {
    data: Record<string, unknown>;
    pullRequests: Array<Record<string, unknown>>;
    extraHints: string[];
  },
  query: PartialPRQuery
): { data: Record<string, unknown>; extraHints: string[] } {
  const queryWithVerbosity = query as WithVerbosity<typeof query>;
  if (isVerbose(queryWithVerbosity)) {
    return { data: input.data, extraHints: input.extraHints };
  }

  // Strip metadata-only fields from each PR entry
  const METADATA_KEYS = new Set([
    'createdAt', 'updatedAt', 'closedAt', 'mergedAt',
    'comments', 'reactions', 'labels', 'assignees', 'reviewers',
    'commits', 'additions', 'deletions', 'changedFiles',
  ]);
  const strippedPrs = input.pullRequests.map(pr => {
    const result: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(pr)) {
      if (!METADATA_KEYS.has(key)) result[key] = val;
    }
    return result;
  });
  return {
    data: { ...input.data, pull_requests: strippedPrs },
    extraHints: input.extraHints,
  };
}
