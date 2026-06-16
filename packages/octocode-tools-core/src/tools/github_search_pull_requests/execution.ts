import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubSearchPullRequestsToolResult } from '@octocodeai/octocode-core/extra-types';
import { GitHubPullRequestSearchQueryLocalSchema } from './scheme.js';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type GitHubPullRequestSearchInput = z.input<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;
type PartialPRQuery = WithOptionalMeta<GitHubPullRequestSearchQuery>;
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
import {
  hasExpensiveContentRequest,
  normalizePullRequestContentRequest,
} from './contentRequest.js';
import {
  buildContentHints,
  shapePullRequestForContent,
} from './contentResponse.js';

export async function searchMultipleGitHubPullRequests(
  args: ToolExecutionArgs<GitHubPullRequestSearchInput>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubPullRequestSearchInput, _index: number) => {
      try {
        const validation =
          GitHubPullRequestSearchQueryLocalSchema.safeParse(query);
        if (!validation.success) {
          const messages = validation.error.issues
            .map(i => i.message)
            .join('; ');
          return createErrorResult(`Validation error: ${messages}`, query);
        }

        const currentProviderContext = getProviderContext();
        const effectiveQuery: PartialPRQuery = { ...validation.data };
        const contentRequest = normalizePullRequestContentRequest(
          effectiveQuery as never
        );
        const downgradeHints: string[] = [];
        const hasPrNumber = effectiveQuery.prNumber !== undefined;

        if (!hasPrNumber && hasExpensiveContentRequest(contentRequest)) {
          downgradeHints.push(
            'Broad PR search returns metadata only. Re-call with prNumber and content selectors (body, changedFiles, patches, comments, commits) or reviewMode="full" to fetch PR content.'
          );
        }

        if (!hasPrNumber) {
          (effectiveQuery as { content?: unknown }).content = undefined;
          (effectiveQuery as { reviewMode?: unknown }).reviewMode = undefined;
        }

        const hasTextQuery =
          !hasPrNumber &&
          ((effectiveQuery.keywordsToSearch?.length ?? 0) > 0 ||
            Boolean(effectiveQuery.query));
        const looksLikeArchaeology =
          hasTextQuery &&
          !effectiveQuery.created &&
          (effectiveQuery.state === 'merged' ||
            (effectiveQuery as { merged?: boolean }).merged === true);
        if (
          looksLikeArchaeology &&
          !effectiveQuery.sort &&
          !effectiveQuery.order
        ) {
          downgradeHints.push(
            'Archaeology tip: to find the PR that *introduced* a feature, add sort:"created" order:"asc" — this surfaces the oldest merged PRs first. ' +
              'Also: scope with match:["title"] to restrict keyword matching to the title field only, and use a double-quoted phrase in `query` (e.g. query:\'"Partial Prerendering"\') for exact-phrase matching.'
          );
        } else if (
          hasTextQuery &&
          !effectiveQuery.created &&
          !effectiveQuery.sort &&
          !effectiveQuery.order
        ) {
          downgradeHints.push(
            'Archaeology tip: add state:"merged" sort:"created" order:"asc" to find the PR that first introduced a feature. ' +
              'Use match:["title"] to restrict to title-only, and quote multi-word phrases in `query` (e.g. query:\'"Server Actions"\').'
          );
        }

        const hasValidParams =
          effectiveQuery.keywordsToSearch?.length ||
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

        const includeFileChanges = hasPrNumber
          ? contentRequest.changedFiles ||
            contentRequest.patches.mode !== 'none'
          : false;
        const {
          pullRequests,
          resultData,
          pagination: rawPagination,
        } = mapPullRequestProviderResultData(providerResult.response.data, {
          includeFileChanges,
        });

        const pagination =
          effectiveQuery.prNumber !== undefined ? undefined : rawPagination;
        if (effectiveQuery.prNumber !== undefined) {
          delete (resultData as Record<string, unknown>).pagination;
        }

        const shouldLeanBroadShape =
          !hasPrNumber &&
          (Boolean((query as { content?: unknown }).content) ||
            Boolean((query as { reviewMode?: unknown }).reviewMode));
        const prMinify = effectiveQuery.minify === 'standard';
        const leanRequest = {
          ...contentRequest,
          body: false,
          changedFiles: false,
          patches: { mode: 'none' as const },
          comments: false as const,
          commits: false as const,
        };
        const showContentMap = hasPrNumber;
        const shapedPullRequests = pullRequests.map(pr =>
          shapePullRequestForContent(
            pr,
            effectiveQuery as never,
            shouldLeanBroadShape ? leanRequest : contentRequest,
            prMinify,
            showContentMap
          )
        );
        resultData.pull_requests = shapedPullRequests;

        const hasContent = shapedPullRequests.length > 0;
        const totalCount =
          (providerResult.response.data as { total_count?: number })
            .total_count ?? -1;
        const confirmedZero = !hasContent && totalCount === 0;

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

        const resultHints: string[] = hasContent
          ? [
              `Found ${shapedPullRequests.length} PR${shapedPullRequests.length === 1 ? '' : 's'}.`,
              ...(showContentMap
                ? buildContentHints(shapedPullRequests, contentRequest)
                : []),
            ]
          : [];

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
        const requestedAnyContent =
          contentRequest.body ||
          contentRequest.changedFiles ||
          contentRequest.patches.mode !== 'none' ||
          Boolean(contentRequest.comments) ||
          contentRequest.reviews ||
          Boolean(contentRequest.commits);
        const deliveredAnyContent = hasPrNumber && requestedAnyContent;
        if (!includeFileChanges && !deliveredAnyContent) {
          const withChanges = pullRequests.filter(
            (pr: Record<string, unknown>) =>
              typeof pr.changedFilesCount === 'number' &&
              pr.changedFilesCount > 0
          ).length;
          if (withChanges > 0) {
            fileChangeHints.push(
              'Metadata mode: changedFiles details omitted (changedFilesCount available). Re-call with prNumber + content.changedFiles=true for file paths, content.patches={mode:"selected",files:["src/foo.ts"]} for targeted diffs, or reviewMode="full" for all content in one call.'
            );
          }
        }

        const hasMore = Boolean(pagination?.hasMore);

        const matchStringHints =
          hasPrNumber &&
          typeof (effectiveQuery as { matchString?: string }).matchString ===
            'string' &&
          (effectiveQuery as { matchString: string }).matchString.trim()
            ? [
                `matchString filter active — pagination totals count only items matching "${(effectiveQuery as { matchString: string }).matchString.trim()}"; drop matchString for the full set.`,
              ]
            : [];

        const shaped = buildPRSearchOutput(
          {
            data: resultData,
            pullRequests,
            extraHints: [
              ...resultHints,
              ...paginationHints,
              ...downgradeHints,
              ...fileChangeHints,
              ...matchStringHints,
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
            hintContext: {
              matchCount: shapedPullRequests.length,
              state: effectiveQuery.state,
              owner: effectiveQuery.owner,
              repo: effectiveQuery.repo,
              author: effectiveQuery.author,
              keywords: effectiveQuery.keywordsToSearch,
              prNumber: effectiveQuery.prNumber,
              prMatch: effectiveQuery.match,
            },
            extraHints: shaped.extraHints,
            evidence: {
              kind: 'pr',
              answerReady: hasContent || confirmedZero,
              complete: hasContent ? !hasMore : confirmedZero,
              ...(confirmedZero
                ? {
                    reason:
                      '0 results confirmed — search returned zero matches.',
                  }
                : !hasContent
                  ? {
                      reason:
                        'No PRs matched the supplied filters; try widening the query or removing state/author/label filters.',
                    }
                  : {}),
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
    },
    args
  );
}

export function buildPRSearchOutput(
  input: {
    data: Record<string, unknown>;
    pullRequests: Array<Record<string, unknown>>;
    extraHints: string[];
  },
  _query: PartialPRQuery
): { data: Record<string, unknown>; extraHints: string[] } {
  return { data: input.data, extraHints: input.extraHints };
}
