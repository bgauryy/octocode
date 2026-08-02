import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  handleCatchError,
  createErrorResult,
  createSuccessResult,
} from '../utils.js';
import { fetchDiscussions } from '../../github/discussions.js';
import { isGitHubAPIError } from '../../github/githubAPI.js';

const DISCUSSIONS_PAGE_SIZE_DEFAULT = 30;

type PartialDiscussionsQuery = {
  owner?: string;
  repo?: string;
  keywordsToSearch?: string[];
  itemsPerPage?: number;
  limit?: number;
  after?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
};

export async function searchMultipleGitHubDiscussions(
  args: ToolExecutionArgs<PartialDiscussionsQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, sessionId } = args;

  return executeBulkOperation(
    queries,
    async (query: PartialDiscussionsQuery, _index: number) => {
      try {
        if (!query.owner || !query.repo) {
          return createErrorResult(
            'owner and repo are required to search discussions.',
            query
          );
        }

        const perPage =
          Number(query.limit ?? query.itemsPerPage) ||
          DISCUSSIONS_PAGE_SIZE_DEFAULT;
        const result = await fetchDiscussions(
          {
            owner: query.owner,
            repo: query.repo,
            keywords: query.keywordsToSearch,
            perPage,
            ...(query.after ? { after: query.after } : {}),
          },
          authInfo,
          sessionId
        );

        if (isGitHubAPIError(result)) {
          return createErrorResult(result, query, {
            toolName: TOOL_NAMES.GITHUB_DISCUSSIONS,
          });
        }

        const hasContent = result.data.discussions.length > 0;

        // `totalCount:0` alone can't distinguish "Discussions disabled" from
        // "enabled, nothing posted yet" — surface the repo-level flag (now
        // threaded through from the GraphQL query) explicitly when it's the
        // more likely explanation for an empty result.
        const discussionsWarnings =
          !hasContent && result.data.hasDiscussionsEnabled === false
            ? [
                'This repository has Discussions disabled — an empty result here means the feature is off, not that no matching discussions exist.',
              ]
            : [];

        // Cursor-based continuation: hand back a ready-to-run next page when
        // GitHub reports more, matching the next-hint convention of other tools.
        const nextCursor = result.data.pagination.nextCursor;
        const dataWithNext = {
          ...(result.data as unknown as Record<string, unknown>),
          ...(discussionsWarnings.length > 0
            ? { warnings: discussionsWarnings }
            : {}),
          ...(nextCursor
            ? {
                next: {
                  nextPage: {
                    tool: 'ghSearchDiscussions',
                    query: {
                      owner: query.owner,
                      repo: query.repo,
                      ...(query.keywordsToSearch
                        ? { keywordsToSearch: query.keywordsToSearch }
                        : {}),
                      itemsPerPage: perPage,
                      after: nextCursor,
                    },
                    why: 'Fetch the next page of discussions',
                    confidence: 'exact',
                  },
                },
              }
            : {}),
        };

        return createSuccessResult(
          query,
          dataWithNext,
          hasContent,
          TOOL_NAMES.GITHUB_DISCUSSIONS,
          { rawResponse: result.rawResponseChars }
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          'Failed to search repository discussions',
          TOOL_NAMES.GITHUB_DISCUSSIONS
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_DISCUSSIONS,
      keysPriority: [
        'totalCount',
        'discussions',
        'pagination',
        'error',
      ] satisfies string[],
    },
    args
  );
}
