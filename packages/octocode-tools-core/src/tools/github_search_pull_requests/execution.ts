import type { CallToolResult } from '@modelcontextprotocol/server';
import { GITHUB_SEARCH_HISTORY_TOOL_NAME } from '../toolNames.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { handleCatchError, safeParseOrError } from '../utils.js';
import { createLazyProviderContext } from '../providerExecution.js';
import { handleIssuesMode } from './execution/issuesMode.js';
import { handleCommitsMode } from './execution/commitsMode.js';
import { handlePullRequestsMode } from './execution/pullRequestsMode.js';
import { GitHubPullRequestSearchQueryLocalSchema } from './scheme.js';
import type { GitHubPullRequestSearchInput } from './execution/types.js';

export async function searchMultipleGitHubPullRequests(
  args: ToolExecutionArgs<GitHubPullRequestSearchInput>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubPullRequestSearchInput, _index: number) => {
      try {
        const parsed = safeParseOrError(
          GitHubPullRequestSearchQueryLocalSchema,
          query
        );
        if (parsed.ok === false) {
          return parsed.error;
        }

        // `type` is injected by the internal focused executors,
        // each backed by a FOCUSED schema that strips foreign fields before this
        // runs — so no cross-mode "ignored field" warning machinery is needed.
        const type = (parsed.data as { type?: string }).type;

        if (type === 'issues') {
          return await handleIssuesMode(query, parsed.data, authInfo);
        }

        if (type === 'commits') {
          return await handleCommitsMode(query, parsed.data, authInfo);
        }

        return await handlePullRequestsMode(
          query,
          parsed.data,
          getProviderContext
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          GITHUB_SEARCH_HISTORY_TOOL_NAME
        );
      }
    },
    {
      toolName: GITHUB_SEARCH_HISTORY_TOOL_NAME,
      keysPriority: [
        'pullRequests',
        'issues',
        'pagination',
        'totalCount',
        'error',
      ],
    },
    args
  );
}
