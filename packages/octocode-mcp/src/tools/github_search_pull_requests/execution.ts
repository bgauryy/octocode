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
import { applyOutputSizeLimit } from '../../utils/pagination/outputSizeLimit.js';
import { serializeForPagination } from '../../utils/pagination/core.js';
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
  const { queries, authInfo, responseCharOffset, responseCharLength, format } =
    args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialPRQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();

        // Pre-flight verbosity caps under concise: cap limit to 3; coerce
        // type→"metadata" unless caller passed prNumber + explicit type;
        // drop partialContentMetadata when type is coerced. Record what
        // fired so we can emit a verbosity-downgrade warning later.
        const prVerbosityIsConcise = isConcise(
          (query as WithVerbosity<typeof query>).verbosity
        );
        if (prVerbosityIsConcise) {
          const userLimit = (query as { limit?: number }).limit;
          if (typeof userLimit === 'number' && userLimit > CONCISE_PR_LIMIT) {
            (query as { limit?: number }).limit = CONCISE_PR_LIMIT;
          }
          const hasExplicitType =
            (query as { type?: string }).type !== undefined;
          const hasPrNumber = query.prNumber !== undefined;
          const shouldCoerceType = !(hasPrNumber && hasExplicitType);
          if (shouldCoerceType) {
            const currentType = (query as { type?: string }).type;
            if (currentType && currentType !== 'metadata') {
              (query as { type?: string }).type = 'metadata';
            } else if (!currentType) {
              (query as { type?: string }).type = 'metadata';
            }
            if (
              (query as { partialContentMetadata?: unknown })
                .partialContentMetadata !== undefined
            ) {
              delete (query as { partialContentMetadata?: unknown })
                .partialContentMetadata;
            }
          }
        }

        if (query.query && String(query.query).length > 256) {
          return createErrorResult(
            'Query too long. Maximum 256 characters allowed.',
            query
          );
        }

        const hasValidParams =
          query.query?.trim() ||
          query.owner ||
          query.repo ||
          query.author ||
          query.assignee ||
          (query.prNumber && query.owner && query.repo);

        if (!hasValidParams) {
          return createErrorResult(
            'At least one valid search parameter, filter, or PR number is required.',
            query
          );
        }

        const providerResult = await executeProviderOperation(query, () =>
          currentProviderContext.provider.searchPullRequests(
            mapPullRequestToolQuery(query)
          )
        );

        if (providerResult.ok === false) {
          return providerResult.result;
        }

        // A prNumber lookup targets one PR — return its full body, not the
        // 500-char search preview (and make the truncation hint truthful).
        const { pullRequests, resultData, pagination } =
          mapPullRequestProviderResultData(providerResult.response.data, {
            fullBody: query.prNumber !== undefined,
          });

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

        const serialized = serializeForPagination(resultData, true);
        const sizeLimitResult = applyOutputSizeLimit(serialized, {
          charOffset: query.charOffset,
          charLength: query.charLength,
        });

        let outputLimitData: Record<string, unknown> = resultData;
        if (sizeLimitResult.wasLimited && sizeLimitResult.pagination) {
          const pg = sizeLimitResult.pagination;
          outputLimitData = {
            ...resultData,
            outputPagination: {
              charOffset: pg.charOffset!,
              charLength: pg.charLength!,
              totalChars: pg.totalChars!,
              hasMore: pg.hasMore,
              currentPage: pg.currentPage,
              totalPages: pg.totalPages,
            },
          };
        }

        const outputLimitHints = [
          ...sizeLimitResult.warnings,
          ...sizeLimitResult.paginationHints,
        ];

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

        const hasMore = Boolean(
          pagination?.hasMore ||
          (sizeLimitResult.wasLimited && sizeLimitResult.pagination?.hasMore)
        );

        const shaped = applyGithubSearchPullRequestsVerbosity(
          {
            data: outputLimitData,
            pullRequests,
            extraHints: [
              ...paginationHints,
              ...outputLimitHints,
              ...fileChangeHints,
            ],
          },
          query as PartialPRQuery
        );

        return createSuccessResult(
          query,
          shaped.data,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
          {
            // Pass query-shape fields so the per-tool empty branch can
            // name the actual filters that produced zero results
            // (state, author, prNumber, query) instead of generic prose.
            hintContext: {
              matchCount: pullRequests.length,
              state: query.state,
              owner: query.owner,
              repo: query.repo,
              author: query.author,
              query: query.query,
              prNumber: query.prNumber,
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
        'outputPagination',
        'total_count',
        'error',
      ] satisfies Array<keyof GitHubSearchPullRequestsToolResult>,
      responseCharOffset,
      responseCharLength,

      format,
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
