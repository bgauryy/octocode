import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { createSuccessResult, handleCatchError } from '../utils.js';
import { createErrorResult } from '../../utils/response/error.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';
import { fetchHistory } from '../../github/history.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { GITHUB_HISTORY_TOOL_NAME } from './toolName.js';
import type { HistoryQueryInput } from './scheme.js';

export async function getMultipleHistories(
  args: ToolExecutionArgs<HistoryQueryInput>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;

  return executeBulkOperation(
    queries,
    async (query: HistoryQueryInput, _index: number) => {
      try {
        if (query.type === 'file' && !query.path) {
          return createErrorResult(
            'path is required when type is "file". Provide a repo-relative file path (e.g. "src/auth/session.ts").',
            query
          );
        }

        const result = await fetchHistory(
          {
            type: query.type,
            owner: query.owner,
            repo: query.repo,
            path: query.path,
            branch: query.branch,
            since: query.since,
            until: query.until,
            author: query.author,
            page: Number(query.page) || 1,
            perPage: Number(query.perPage) || 30,
            includeDiff: Boolean(query.includeDiff),
            charLength:
              typeof query.charLength === 'number' ? query.charLength : undefined,
          },
          authInfo
        );

        if (isGitHubAPIError(result)) {
          const isRateLimited =
            result.status === 429 ||
            result.error?.toString().toLowerCase().includes('rate limit') ||
            false;
          return createErrorResult(result, query, {
            toolName: GITHUB_HISTORY_TOOL_NAME,
            hintContext: {
              type: query.type,
              path: query.path,
              isRateLimited,
              status: result.status,
              retryAfter: result.retryAfter,
            },
            hintSourceError: result,
          });
        }

        const { commits, pagination } = result.data;
        const hasContent = commits.length > 0;

        const extraHints: string[] = [];
        if (pagination.hasMore && pagination.nextPage) {
          extraHints.push(
            `${commits.length} commit${commits.length === 1 ? '' : 's'} returned — re-call with page:${pagination.nextPage} for more.`
          );
        }

        return createSuccessResult(
          query,
          result.data as unknown as Record<string, unknown>,
          hasContent,
          GITHUB_HISTORY_TOOL_NAME,
          {
            hintContext: {
              path: query.path,
              matchCount: commits.length,
              hasMorePages: pagination.hasMore,
            },
            extraHints,
          }
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          'ghHistory fetch failed',
          GITHUB_HISTORY_TOOL_NAME
        );
      }
    },
    {
      toolName: GITHUB_HISTORY_TOOL_NAME,
      keysPriority: ['commits', 'pagination', 'type', 'path', 'owner', 'repo'],
      peerHints: true,
    },
    args
  );
}
