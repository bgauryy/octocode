import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type {
  GitHubPullRequestSearchQuery,
  GitHubSearchPullRequestsToolResult,
} from '@octocodeai/octocode-core';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
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
  args: ToolExecutionArgs<GitHubPullRequestSearchQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubPullRequestSearchQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();

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

        const { pullRequests, resultData, pagination } =
          mapPullRequestProviderResultData(providerResult.response.data);

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
            `Large PR(s) ${prNumbers} have ${maxFiles}+ file changes`,
            'Use charOffset/charLength to paginate through full output',
            'Or use type=\'partialContent\' with partialContentMetadata=[{file: "path/to/file.ts"}] for targeted file diffs'
          );
        }

        return createSuccessResult(
          query,
          outputLimitData,
          hasContent,
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
          {
            hintContext: { matchCount: pullRequests.length },
            extraHints: [
              ...paginationHints,
              ...outputLimitHints,
              ...fileChangeHints,
              "file_changes[].patch = diff hunks; use prNumber + type='partialContent' for full file diffs",
            ],
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
    }
  );
}
