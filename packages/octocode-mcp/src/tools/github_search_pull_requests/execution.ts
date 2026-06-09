import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { GitHubSearchPullRequestsToolResult } from '@octocodeai/octocode-core/extra-types';

type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type {
  ToolExecutionArgs,
  WithOptionalMeta,
} from '../../types/execution.js';

type PRDefaultKeys = 'order' | 'limit' | 'page';
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
import {
  hasExpensiveContentRequest,
  normalizePullRequestContentRequest,
} from './contentRequest.js';
import {
  buildContentHints,
  shapePullRequestForContent,
} from './contentResponse.js';

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
        const prMinify =
          (effectiveQuery as { minify?: boolean }).minify !== false;
        const leanRequest = {
          ...contentRequest,
          body: false,
          changedFiles: false,
          patches: { mode: 'none' as const },
          comments: false as const,
          commits: false as const,
        };
        const showContentMap =
          shouldLeanBroadShape || hasExpensiveContentRequest(contentRequest);
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
        if (!includeFileChanges) {
          const withChanges = pullRequests.filter(
            (pr: Record<string, unknown>) =>
              typeof pr.changedFilesCount === 'number' &&
              pr.changedFilesCount > 0
          ).length;
          if (withChanges > 0) {
            fileChangeHints.push(
              'Metadata mode: changedFiles details omitted (changedFilesCount available). Re-call with prNumber + content.changedFiles=true for file paths, or content.patches={mode:"selected",files:["src/foo.ts"]} for targeted diffs.'
            );
          }
        }

        const hasMore = Boolean(pagination?.hasMore);

        const shaped = buildPRSearchOutput(
          {
            data: resultData,
            pullRequests,
            extraHints: [
              ...resultHints,
              ...paginationHints,
              ...downgradeHints,
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
            hintContext: {
              matchCount: shapedPullRequests.length,
              state: effectiveQuery.state,
              owner: effectiveQuery.owner,
              repo: effectiveQuery.repo,
              author: effectiveQuery.author,
              query: effectiveQuery.query,
              prNumber: effectiveQuery.prNumber,
              matchScope: effectiveQuery.matchScope,
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
